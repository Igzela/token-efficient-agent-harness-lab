//! Store-owned managed-acceptance decision, risk acknowledgement, spend authorization,
//! and exactly-once attempt admission.
//!
//! Production principals are derived only from verified `api_key_metadata` records.
//! Free-form strings never create authority. Fixture principals are test-only and cannot
//! authorize production live starts.

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ffi::CString;
use std::io::{Read, Seek, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use crate::infrastructure::auth::{LOCAL_BOOTSTRAP_API_KEY_ID, LOCAL_BOOTSTRAP_TENANT_ID};
use crate::node_executor::ProcessOutcome;
use crate::provider::redaction::contains_sensitive_patterns;

/// Required scopes for managed-acceptance authority operations.
pub const SCOPE_RISK_ACKNOWLEDGE: &str = "managed_acceptance:risk_acknowledge";
pub const SCOPE_SPEND_AUTHORIZE: &str = "managed_acceptance:spend_authorize";
pub const SCOPE_ATTEMPT_ADMIT: &str = "managed_acceptance:attempt_admit";
pub const SCOPE_REVOKE: &str = "managed_acceptance:revoke";
pub const SCOPE_DELEGATED_AUTONOMY: &str = "managed_acceptance:delegated_autonomy";
pub const SCOPE_DELEGATED_MANIFEST_APPROVE: &str = "managed_acceptance:delegated_manifest_approve";
pub const SCOPE_DELEGATED_EXECUTE: &str = "managed_acceptance:delegated_execute";
pub const SCOPE_DELEGATED_ARTIFACT_CONFIRM: &str = "managed_acceptance:delegated_artifact_confirm";

/// Capability held only by the environment bootstrap authority. It is not a
/// managed-operation scope and is never included in child principals.
pub const SCOPE_IDENTITY_DELEGATE: &str = "managed_acceptance:identity_delegate";
pub const BOOTSTRAP_MANAGED_ACCEPTANCE_DELEGATION_SCOPES: &[&str] = &[SCOPE_IDENTITY_DELEGATE];

/// Least-privilege API-key ceilings for the two managed identities used by
/// the delegated ProductTask path. Reviewer approval uses its dedicated
/// capability rather than the broad tenant-admin scope. These are ceilings,
/// not a grant to the bootstrap key; callers may request a subset, but never
/// an unrelated managed or ordinary authority scope.
pub const MANAGED_REVIEWER_KEY_SCOPES: &[&str] = &[
    SCOPE_RISK_ACKNOWLEDGE,
    SCOPE_DELEGATED_AUTONOMY,
    SCOPE_DELEGATED_MANIFEST_APPROVE,
    SCOPE_SPEND_AUTHORIZE,
    SCOPE_DELEGATED_ARTIFACT_CONFIRM,
];
pub const MANAGED_OUTPUT_OPERATOR_KEY_SCOPES: &[&str] = &[
    "dispatch:execute",
    SCOPE_RISK_ACKNOWLEDGE,
    SCOPE_DELEGATED_ARTIFACT_CONFIRM,
    SCOPE_DELEGATED_EXECUTE,
    SCOPE_ATTEMPT_ADMIT,
];
/// Least-privilege RWE run authority. This identity may acknowledge risk,
/// authorize the frozen spend envelope, admit its one-use run, and revoke it;
/// cell execution and artifact confirmation remain separate identities.
pub const MANAGED_RWE_OPERATOR_KEY_SCOPES: &[&str] = &[
    SCOPE_RISK_ACKNOWLEDGE,
    SCOPE_SPEND_AUTHORIZE,
    SCOPE_ATTEMPT_ADMIT,
    SCOPE_REVOKE,
    SCOPE_DELEGATED_AUTONOMY,
    SCOPE_DELEGATED_MANIFEST_APPROVE,
];

pub const ALL_MANAGED_ACCEPTANCE_SCOPES: &[&str] = &[
    SCOPE_RISK_ACKNOWLEDGE,
    SCOPE_SPEND_AUTHORIZE,
    SCOPE_ATTEMPT_ADMIT,
    SCOPE_REVOKE,
    SCOPE_DELEGATED_AUTONOMY,
    SCOPE_DELEGATED_MANIFEST_APPROVE,
    SCOPE_DELEGATED_EXECUTE,
    SCOPE_DELEGATED_ARTIFACT_CONFIRM,
];

/// Validate the only role-specific managed-acceptance delegation profiles.
/// The environment bootstrap authority owns this policy; ordinary tenants
/// cannot reach this function through the key API because the handler first
/// requires the immutable bootstrap key identity and delegation capability.
pub fn validate_managed_acceptance_role_scopes(
    role: &str,
    scopes: &[String],
) -> Result<(), String> {
    let managed_role = matches!(role, "reviewer" | "output_operator" | "operator");
    let managed_requested = scopes.iter().any(|scope| {
        ALL_MANAGED_ACCEPTANCE_SCOPES.contains(&scope.as_str())
            || scope.starts_with("managed_acceptance:")
    });
    if !managed_role && !managed_requested {
        return Ok(());
    }
    let allowed = match role {
        "reviewer" => MANAGED_REVIEWER_KEY_SCOPES,
        "output_operator" => MANAGED_OUTPUT_OPERATOR_KEY_SCOPES,
        "operator" => MANAGED_RWE_OPERATOR_KEY_SCOPES,
        _ => {
            return Err(format!(
                "managed-acceptance scopes require reviewer, output_operator, or operator role; got {role:?}"
            ));
        }
    };
    if let Some(scope) = scopes
        .iter()
        .find(|scope| !allowed.contains(&scope.as_str()))
    {
        return Err(format!(
            "scope {scope:?} is outside the least-privilege {role} managed-acceptance profile"
        ));
    }
    Ok(())
}

fn validate_bootstrap_identity_delegation_scopes(scopes: &[String]) -> Result<(), String> {
    if !BOOTSTRAP_MANAGED_ACCEPTANCE_DELEGATION_SCOPES
        .iter()
        .all(|required| scopes.iter().any(|scope| scope == required))
    {
        return Err("canonical bootstrap identity-delegation scope is missing".into());
    }
    if let Some(scope) = scopes.iter().find(|scope| {
        scope.starts_with("managed_acceptance:")
            && !BOOTSTRAP_MANAGED_ACCEPTANCE_DELEGATION_SCOPES.contains(&scope.as_str())
    }) {
        return Err(format!(
            "canonical bootstrap carries a managed-operation scope outside its delegation ceiling: {scope:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod managed_identity_scope_profile_tests {
    use super::{
        validate_managed_acceptance_role_scopes, MANAGED_OUTPUT_OPERATOR_KEY_SCOPES,
        MANAGED_REVIEWER_KEY_SCOPES, MANAGED_RWE_OPERATOR_KEY_SCOPES, SCOPE_ATTEMPT_ADMIT,
        SCOPE_DELEGATED_ARTIFACT_CONFIRM, SCOPE_DELEGATED_AUTONOMY, SCOPE_DELEGATED_EXECUTE,
        SCOPE_DELEGATED_MANIFEST_APPROVE, SCOPE_REVOKE, SCOPE_RISK_ACKNOWLEDGE,
        SCOPE_SPEND_AUTHORIZE,
    };

    fn strings(scopes: &[&str]) -> Vec<String> {
        scopes.iter().map(|scope| (*scope).to_string()).collect()
    }

    #[test]
    fn reviewer_profiles_are_stage_specific_and_never_team_admin() {
        assert!(MANAGED_REVIEWER_KEY_SCOPES.contains(&SCOPE_DELEGATED_MANIFEST_APPROVE));
        assert!(MANAGED_REVIEWER_KEY_SCOPES.contains(&SCOPE_DELEGATED_ARTIFACT_CONFIRM));
        assert!(!MANAGED_REVIEWER_KEY_SCOPES.contains(&"team:admin"));

        validate_managed_acceptance_role_scopes(
            "reviewer",
            &strings(&[
                SCOPE_RISK_ACKNOWLEDGE,
                SCOPE_DELEGATED_AUTONOMY,
                SCOPE_DELEGATED_MANIFEST_APPROVE,
                SCOPE_SPEND_AUTHORIZE,
            ]),
        )
        .unwrap();
        validate_managed_acceptance_role_scopes(
            "reviewer",
            &strings(&[SCOPE_RISK_ACKNOWLEDGE, SCOPE_DELEGATED_ARTIFACT_CONFIRM]),
        )
        .unwrap();
        assert!(validate_managed_acceptance_role_scopes(
            "reviewer",
            &strings(&[SCOPE_RISK_ACKNOWLEDGE, "team:admin"]),
        )
        .is_err());
    }

    #[test]
    fn output_operator_profile_is_execution_only_and_rejects_manifest_authority() {
        assert!(MANAGED_OUTPUT_OPERATOR_KEY_SCOPES.contains(&SCOPE_DELEGATED_EXECUTE));
        assert!(MANAGED_OUTPUT_OPERATOR_KEY_SCOPES.contains(&SCOPE_ATTEMPT_ADMIT));
        assert!(!MANAGED_OUTPUT_OPERATOR_KEY_SCOPES.contains(&SCOPE_DELEGATED_MANIFEST_APPROVE));
        assert!(validate_managed_acceptance_role_scopes(
            "output_operator",
            &strings(&[
                "dispatch:execute",
                SCOPE_RISK_ACKNOWLEDGE,
                SCOPE_DELEGATED_EXECUTE,
                SCOPE_ATTEMPT_ADMIT,
            ]),
        )
        .is_ok());
        assert!(validate_managed_acceptance_role_scopes(
            "output_operator",
            &strings(&[SCOPE_DELEGATED_MANIFEST_APPROVE]),
        )
        .is_err());
    }

    #[test]
    fn rwe_operator_profile_is_run_authority_only() {
        assert_eq!(
            MANAGED_RWE_OPERATOR_KEY_SCOPES,
            &[
                SCOPE_RISK_ACKNOWLEDGE,
                SCOPE_SPEND_AUTHORIZE,
                SCOPE_ATTEMPT_ADMIT,
                SCOPE_REVOKE,
                SCOPE_DELEGATED_AUTONOMY,
                SCOPE_DELEGATED_MANIFEST_APPROVE,
            ]
        );
        assert!(validate_managed_acceptance_role_scopes(
            "operator",
            &strings(MANAGED_RWE_OPERATOR_KEY_SCOPES),
        )
        .is_ok());
        assert!(validate_managed_acceptance_role_scopes(
            "operator",
            &strings(&[SCOPE_RISK_ACKNOWLEDGE, "team:admin"]),
        )
        .is_err());
        assert!(validate_managed_acceptance_role_scopes(
            "operator",
            &strings(&[SCOPE_DELEGATED_EXECUTE]),
        )
        .is_err());
    }
}

pub const DELEGATION_SCHEMA_VERSION: &str = "managed_autonomous_delegation.v1";
pub const FINAL_MANIFEST_SCHEMA_VERSION: &str = "managed_final_execution_manifest.v1";
pub const DELEGATED_ARTIFACT_CONFIRMATION_SCHEMA_VERSION: &str =
    "managed_delegated_artifact_confirmation.v1";
/// Immutable parent authority for an external effect.  The parent is stored
/// in the existing managed-acceptance decision owner; this packet adds no
/// second effect ledger or authority store.
pub const EFFECT_ENVELOPE_SCHEMA_VERSION: &str = "managed_effect_envelope.v1";
/// One-use child authority derived from an accepted effect envelope.  It is
/// persisted in the existing managed-acceptance spend owner.
pub const EFFECT_CHILD_AUTHORIZATION_SCHEMA_VERSION: &str = "managed_effect_child_authorization.v1";
/// A possible-send effect is terminally uncertain and never retryable.
pub const EFFECT_OUTCOME_UNKNOWN: &str = "outcome_unknown";
/// The parent approval phrase is fixed by the store contract, not supplied as
/// an authority-bearing free-form scope by a caller.
pub const EFFECT_ENVELOPE_APPROVAL_PHRASE: &str = "APPROVE_MANAGED_EFFECT_ENVELOPE_V1";
const MAX_EFFECT_CHILD_AUTHORIZATIONS: u64 = 64;
const MAX_EFFECT_OUTCOME_EVIDENCE_BYTES: usize = 2048;
const DELEGATED_MANIFEST_APPROVAL_SCHEMA_VERSION: &str = "managed_delegated_manifest_approval.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EffectEnvelopeContract {
    pub schema_version: String,
    pub decision_id: String,
    pub envelope_id: String,
    pub owner_principal_id: String,
    pub effect_kind: String,
    pub target_repository: String,
    pub target_main_sha: String,
    pub max_total_cost_usd: f64,
    pub max_child_authorizations: u64,
    pub created_at: String,
    pub expires_at: String,
}

impl EffectEnvelopeContract {
    pub fn validate(&self, now: &str) -> Result<(), String> {
        if self.schema_version != EFFECT_ENVELOPE_SCHEMA_VERSION {
            return Err("unsupported effect envelope schema_version".into());
        }
        for (name, value) in [
            ("decision_id", self.decision_id.as_str()),
            ("envelope_id", self.envelope_id.as_str()),
            ("owner_principal_id", self.owner_principal_id.as_str()),
            ("effect_kind", self.effect_kind.as_str()),
            ("target_repository", self.target_repository.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("effect envelope {name} is required"));
            }
            if value.chars().count() > 256 {
                return Err(format!("effect envelope {name} is too long"));
            }
        }
        if self.effect_kind == EFFECT_OUTCOME_UNKNOWN {
            return Err("effect envelope effect_kind cannot be outcome_unknown".into());
        }
        if (self.target_main_sha.len() != 40 && self.target_main_sha.len() != 64)
            || !self.target_main_sha.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err("effect envelope target_main_sha must be a 40- or 64-hex identity".into());
        }
        if !self.max_total_cost_usd.is_finite() || self.max_total_cost_usd < 0.0 {
            return Err(
                "effect envelope max_total_cost_usd must be finite and non-negative".into(),
            );
        }
        if self.max_child_authorizations == 0
            || self.max_child_authorizations > MAX_EFFECT_CHILD_AUTHORIZATIONS
        {
            return Err(format!(
                "effect envelope max_child_authorizations must be between 1 and {MAX_EFFECT_CHILD_AUTHORIZATIONS}"
            ));
        }
        let created_at = parse_rfc3339_utc("created_at", &self.created_at)?;
        let created_canonical = created_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        if created_canonical != self.created_at {
            return Err("effect envelope created_at must use canonical UTC form".into());
        }
        let expires_at = require_finite_expiry(Some(&self.expires_at))?;
        if expires_at != self.expires_at {
            return Err("effect envelope expires_at must use canonical UTC form".into());
        }
        let now = parse_rfc3339_utc("now", now)?;
        if created_at > now {
            return Err("effect envelope created_at cannot be after store time".into());
        }
        let expires_at_dt = parse_rfc3339_utc("expires_at", &expires_at)?;
        if expires_at_dt <= created_at {
            return Err("effect envelope expires_at must be after created_at".into());
        }
        if is_at_or_before(&expires_at, &now.format("%Y-%m-%dT%H:%M:%SZ").to_string())? {
            return Err("effect envelope is expired".into());
        }
        Ok(())
    }

    pub fn body(&self) -> Value {
        let value = serde_json::to_value(self).expect("effect envelope serializes");
        sort_value(&value)
    }

    pub fn sha256(&self) -> Result<String, String> {
        Ok(sha256_hex(canonical_json(&self.body())?.as_bytes()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EffectChildAuthorizationRequest {
    pub child_authorization_id: String,
    pub operation_id: String,
    pub effect_kind: String,
    pub target_repository: String,
    pub target_main_sha: String,
    pub max_cost_usd: f64,
    pub expires_at: String,
}

impl EffectChildAuthorizationRequest {
    fn validate_against(
        &self,
        parent: &EffectEnvelopeContract,
        now: &str,
    ) -> Result<String, String> {
        for (name, value) in [
            (
                "child_authorization_id",
                self.child_authorization_id.as_str(),
            ),
            ("operation_id", self.operation_id.as_str()),
            ("effect_kind", self.effect_kind.as_str()),
            ("target_repository", self.target_repository.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("effect child {name} is required"));
            }
            if value.chars().count() > 256 {
                return Err(format!("effect child {name} is too long"));
            }
        }
        if self.effect_kind != parent.effect_kind {
            return Err("effect child kind exceeds or differs from parent envelope".into());
        }
        if self.target_repository != parent.target_repository
            || self.target_main_sha != parent.target_main_sha
        {
            return Err("effect child target does not exactly match parent envelope".into());
        }
        if !self.max_cost_usd.is_finite()
            || self.max_cost_usd < 0.0
            || self.max_cost_usd > parent.max_total_cost_usd
        {
            return Err("effect child cost exceeds parent envelope".into());
        }
        let expires_at = require_finite_expiry(Some(&self.expires_at))?;
        let parent_expires_at = require_finite_expiry(Some(&parent.expires_at))?;
        if expires_at > parent_expires_at {
            return Err("effect child expiry exceeds parent envelope".into());
        }
        if is_at_or_before(&expires_at, now)? {
            return Err("effect child is expired".into());
        }
        Ok(expires_at)
    }
}

fn validate_effect_envelope_body(
    body: &Value,
    tenant_id: &str,
    principal: Option<&AuthenticatedPrincipal>,
    now: &str,
) -> Result<EffectEnvelopeContract, String> {
    if body.get("schema_version").and_then(Value::as_str) != Some(EFFECT_ENVELOPE_SCHEMA_VERSION) {
        return Err("effect envelope body schema_version is invalid".into());
    }
    if body.get("status").and_then(Value::as_str) != Some("draft_pending_operator") {
        return Err("effect envelope body must be a draft before owner acceptance".into());
    }
    let envelope: EffectEnvelopeContract = serde_json::from_value(
        body.get("effect_envelope")
            .cloned()
            .ok_or("effect envelope body is missing effect_envelope")?,
    )
    .map_err(|error| format!("effect envelope body is malformed: {error}"))?;
    envelope.validate(now)?;
    if body.get("decision_id").and_then(Value::as_str) != Some(envelope.decision_id.as_str()) {
        return Err("effect envelope decision_id is not bound to its decision body".into());
    }
    if let Some(principal) = principal {
        if principal.tenant_id != tenant_id {
            return Err("effect envelope principal tenant does not match decision tenant".into());
        }
        if principal.principal_id != envelope.owner_principal_id {
            return Err(
                "effect envelope owner principal does not match authenticated owner".into(),
            );
        }
    }
    Ok(envelope)
}

type DelegatedSpendSqliteRow = (
    String,
    String,
    String,
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

type DelegatedAttemptSqliteRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
);

type DelegatedArtifactConfirmationSqliteRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

/// Hash-bound policy delegated by an authenticated operator. This is a policy
/// receipt, not an execution or output authority; both are issued separately
/// and only after the exact derived manifest is checked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DelegationContract {
    pub schema_version: String,
    pub delegation_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub executions: u64,
    pub repositories: Vec<String>,
    pub task_classes: Vec<String>,
    pub allowed_paths: Vec<String>,
    pub max_changed_files: u64,
    pub max_changed_lines: u64,
    pub max_cost_usd_per_run: f64,
    pub max_total_cost_usd: f64,
    pub protocol: String,
    pub models: Value,
    pub output: Value,
    pub forbidden: Vec<String>,
}

impl DelegationContract {
    pub fn validate(&self, now: &str) -> Result<(), String> {
        if self.schema_version != DELEGATION_SCHEMA_VERSION {
            return Err("unsupported delegation schema_version".into());
        }
        if self.delegation_id.trim().is_empty() || self.executions != 1 {
            return Err("delegation must contain one execution and an id".into());
        }
        let docs_scope = self.repositories == ["Igzela/alters-lab"]
            && self.task_classes == ["documentation"]
            && self.allowed_paths == ["docs/USER_GUIDE.md"]
            && self.max_changed_files == 1
            && self.max_changed_lines == 100
            && (self.max_cost_usd_per_run - 0.50).abs() <= f64::EPSILON
            && (self.max_total_cost_usd - 0.50).abs() <= f64::EPSILON;
        let rwe_scope = {
            let union = crate::rwe::frozen_rwe_bindings::frozen_rwe_union_allowed_paths()
                .unwrap_or_default();
            let (max_files, max_lines) =
                crate::rwe::frozen_rwe_bindings::frozen_rwe_max_patch_limits().unwrap_or((0, 0));
            // One-execution RWE cell: monetary ceilings equal frozen schedule cell max_cost.
            let cell_cost = crate::rwe::operator_corpus::freeze_current_operator_contract_set()
                .ok()
                .and_then(|frozen| {
                    frozen
                        .schedule
                        .body
                        .get("cells")
                        .and_then(Value::as_array)
                        .and_then(|cells| cells.first())
                        .and_then(|cell| {
                            crate::rwe::frozen_rwe_bindings::frozen_schedule_cell_max_cost(cell)
                                .ok()
                                .flatten()
                        })
                });
            let cost_ok = cell_cost.is_some_and(|c| {
                (self.max_cost_usd_per_run - c).abs() <= f64::EPSILON
                    && (self.max_total_cost_usd - c).abs() <= f64::EPSILON
            });
            self.repositories == ["Igzela/alters-lab"]
                && self.task_classes == ["rwe"]
                && !union.is_empty()
                && self.allowed_paths == union
                && self.max_changed_files == max_files
                && self.max_changed_lines == max_lines
                && cost_ok
        };
        if !docs_scope && !rwe_scope {
            return Err(
                "delegation scope is outside the bounded documentation or exact frozen RWE policy"
                    .into(),
            );
        }
        if self.protocol != "openai_compatible" {
            return Err("delegation spend policy is invalid".into());
        }
        let default_models = json!({
            "planner": "deepseek-v4-pro",
            "implementer": "deepseek-v4-flash",
            "reviewer": "deepseek-v4-pro"
        });
        let valid_models = self.models == default_models;
        if !valid_models
            || self.output
                != json!({
                    "draft_pr_only": true,
                    "target_main_write": false,
                    "merge": false,
                    "auto_merge": false
                })
        {
            return Err("delegation model or output policy is invalid".into());
        }
        if self.forbidden
            != vec![
                "credential changes",
                "authentication or permission changes",
                "schema or database migrations",
                "dependency changes",
                "executable or workflow changes",
                "destructive operations",
                "release",
                "deployment",
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
        {
            return Err("delegation forbidden-action policy is invalid".into());
        }
        if is_at_or_before(&self.expires_at, now)?
            || !is_strictly_after(&self.expires_at, &self.created_at)?
        {
            return Err("delegation expiry is not finite and future-dated".into());
        }
        let created = parse_rfc3339_utc("created_at", &self.created_at)?;
        let expires = parse_rfc3339_utc("expires_at", &self.expires_at)?;
        if expires - created != chrono::Duration::hours(24) {
            return Err("delegation expiry must be exactly 24 hours after creation".into());
        }
        if !self.max_cost_usd_per_run.is_finite() || !self.max_total_cost_usd.is_finite() {
            return Err("delegation cost limits must be finite".into());
        }
        Ok(())
    }

    pub fn body(&self) -> Value {
        sort_value(&serde_json::to_value(self).expect("delegation serializes"))
    }

    pub fn sha256(&self) -> Result<String, String> {
        Ok(sha256_hex(canonical_json(&self.body())?.as_bytes()))
    }
}

/// Derive an executable manifest from an immutable proposal. The proposal is
/// re-hashed before use, and only the explicitly permitted non-null cap is
/// added; all execution and recovery identities must be supplied by the owner.
pub fn derive_final_execution_manifest(
    proposal: &Value,
    delegation: &DelegationContract,
    execution: &Value,
) -> Result<Value, String> {
    let proposal_sha = proposal
        .get("manifest_sha256")
        .and_then(Value::as_str)
        .ok_or("proposal manifest_sha256 is required")?;
    if proposal_sha != compute_attempt_manifest_sha256(proposal)? {
        return Err("immutable proposal manifest hash mismatch".into());
    }
    let now = execution
        .get("observed_at")
        .and_then(Value::as_str)
        .ok_or("execution observed_at is required")?;
    delegation.validate(now)?;
    for key in [
        "target_repository",
        "target_main_sha",
        "tenant_id",
        "product_task_id",
        "workflow_id",
        "workflow_node_ids",
        "attempt_id",
        "verifier",
        "mutable_paths",
        "cancellation_identity",
        "rollback_identity",
    ] {
        if execution.get(key).is_none() {
            return Err(format!("execution identity {key} is required"));
        }
    }
    let mutable = execution
        .get("mutable_paths")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let verifier = execution
        .get("verifier")
        .and_then(Value::as_str)
        .unwrap_or("");
    let docs_exec = execution.get("target_repository").and_then(Value::as_str)
        == Some("Igzela/alters-lab")
        && mutable == ["docs/USER_GUIDE.md"]
        && verifier == crate::rwe::frozen_rwe_bindings::DOCS_GP_VERIFIER_IDENTITY;
    let rwe_exec = execution.get("target_repository").and_then(Value::as_str)
        == Some("Igzela/alters-lab")
        && crate::rwe::frozen_rwe_bindings::is_exact_frozen_rwe_allowed_paths(&mutable)
        && verifier == crate::rwe::frozen_rwe_bindings::FROZEN_RWE_VERIFIER_IDENTITY;
    if !docs_exec && !rwe_exec {
        return Err("execution identity is outside the delegation".into());
    }
    let (proposal_repository, proposal_main_sha, proposal_paths, proposal_cap, proposal_verifier) =
        match proposal.get("schema_version").and_then(Value::as_str) {
            Some("managed_proposal_manifest.v1") => (
                proposal.get("target_repository"),
                proposal.get("target_main_sha"),
                proposal.get("mutable_paths"),
                proposal.get("max_cost_usd"),
                proposal.get("verifier"),
            ),
            Some("pe7_product_golden_path_live_seal_manifest.v1") => {
                if proposal
                    .pointer("/provider/protocol")
                    .and_then(Value::as_str)
                    != Some("openai_compatible")
                    || proposal
                        .pointer("/role_models/planner")
                        .and_then(Value::as_str)
                        != Some("deepseek-v4-pro")
                    || proposal
                        .pointer("/role_models/implementer")
                        .and_then(Value::as_str)
                        != Some("deepseek-v4-flash")
                    || proposal
                        .pointer("/role_models/reviewer")
                        .and_then(Value::as_str)
                        != Some("deepseek-v4-pro")
                {
                    return Err("immutable legacy proposal provider route is not exact".into());
                }
                (
                    proposal.pointer("/target/repository"),
                    proposal.pointer("/target/default_branch_sha"),
                    proposal.pointer("/target/allowed_mutable_paths"),
                    proposal.pointer("/limits/max_cost_usd"),
                    None,
                )
            }
            _ => return Err("immutable proposal schema is not admitted".into()),
        };
    if proposal_cap != Some(&Value::Null)
        || proposal_repository != execution.get("target_repository")
        || proposal_main_sha != execution.get("target_main_sha")
        || proposal_paths != execution.get("mutable_paths")
        || proposal_verifier.is_some_and(|verifier| Some(verifier) != execution.get("verifier"))
    {
        return Err("immutable proposal does not exactly bind the final execution".into());
    }
    let binding_value = proposal
        .get("provider_execution_binding")
        .or_else(|| proposal.pointer("/provider/execution_binding"))
        .ok_or("immutable proposal provider execution binding is required")?;
    let binding =
        crate::rwe::campaign_package::FrozenProviderExecutionBinding::from_json(binding_value)?;
    let canonical_binding = crate::rwe::campaign_package::canonical_deepseek_provider_binding();
    if binding != canonical_binding {
        return Err("immutable proposal provider execution binding is not canonical".into());
    }
    let provider = canonical_managed_deepseek_provider_value();
    let is_deepseek = true;
    let mut manifest = sort_value(&json!({
        "schema_version": FINAL_MANIFEST_SCHEMA_VERSION,
        "proposal_manifest_sha256": proposal_sha,
        "delegation_sha256": delegation.sha256()?,
        "delegation": {
            "delegation_id": delegation.delegation_id,
            "expires_at": delegation.expires_at
        },
        "execution": execution,
        "target": {
            "repository": "Igzela/alters-lab",
            "main_sha": execution.get("target_main_sha"),
            "mutable_paths": execution.get("mutable_paths")
        },
        "models": delegation.models.clone(),
        "protocol": delegation.protocol.clone(),
        "provider_execution_binding": binding.to_json(),
        "provider": provider,
        "verifier": execution.get("verifier"),
        "limits": {
            "max_changed_files": delegation.max_changed_files,
            "max_changed_lines": delegation.max_changed_lines,
            "max_cost_usd": if is_deepseek { json!(delegation.max_cost_usd_per_run) } else { Value::Null },
            "max_total_cost_usd": if is_deepseek { json!(delegation.max_total_cost_usd) } else { Value::Null },
            "max_provider_requests": 3,
            "max_retries": 0,
            // Docs GP classic envelope vs exact frozen RWE schedule cell ceilings.
            "max_input_tokens": if execution.get("verifier").and_then(Value::as_str)
                == Some(crate::rwe::frozen_rwe_bindings::FROZEN_RWE_VERIFIER_IDENTITY)
            {
                12_000
            } else {
                8_000
            },
            "max_output_tokens": 4_000,
            "max_cumulative_tokens": if execution.get("verifier").and_then(Value::as_str)
                == Some(crate::rwe::frozen_rwe_bindings::FROZEN_RWE_VERIFIER_IDENTITY)
            {
                16_000
            } else {
                24_000
            },
            "timeout_ms": if execution.get("verifier").and_then(Value::as_str)
                == Some(crate::rwe::frozen_rwe_bindings::FROZEN_RWE_VERIFIER_IDENTITY)
            {
                900_000
            } else {
                30_000
            }
        },
        "output": delegation.output,
        "recovery": {
            "cancellation_identity": execution.get("cancellation_identity"),
            "rollback_identity": execution.get("rollback_identity"),
            "cleanup_owner": "LocalProductStore.managed_acceptance"
        }
    }));
    let sha = compute_attempt_manifest_sha256(&manifest)?;
    manifest["manifest_sha256"] = json!(sha);
    Ok(manifest)
}

fn canonical_managed_deepseek_provider_value() -> Value {
    let binding = crate::rwe::campaign_package::canonical_deepseek_provider_binding();
    let mut provider = binding.to_json();
    provider["kind"] = json!(binding.provider_kind);
    provider["price_profile"] =
        serde_json::to_value(crate::provider::managed_deepseek::DeepSeekPriceProfile::default())
            .expect("DeepSeek price profile serializes");
    provider
}

/// Independent artifact/output authority. It accepts only redacted hashes and
/// deterministic results; it cannot execute a provider or merge a PR.
pub fn confirm_delegated_artifact_output(
    delegation: &DelegationContract,
    manifest: &Value,
    artifact: &Value,
    verification: &Value,
    review: &Value,
    provider_execution: &Value,
    target_main_sha: &str,
    realized_cost_usd: f64,
) -> Result<Value, String> {
    delegation.validate(
        manifest
            .pointer("/execution/observed_at")
            .and_then(Value::as_str)
            .ok_or("manifest observed_at is required")?,
    )?;
    let expected_manifest_sha = compute_attempt_manifest_sha256(manifest)?;
    if manifest.get("manifest_sha256").and_then(Value::as_str)
        != Some(expected_manifest_sha.as_str())
        || manifest.pointer("/target/main_sha").and_then(Value::as_str) != Some(target_main_sha)
    {
        return Err("artifact confirmation target or manifest hash is stale".into());
    }
    if verification.get("status").and_then(Value::as_str) != Some("succeeded")
        || verification
            .get("verification_sha256")
            .and_then(Value::as_str)
            .is_none_or(|sha| sha.len() != 64 || !sha.chars().all(|c| c.is_ascii_hexdigit()))
        || review.get("schema_version").and_then(Value::as_str)
            != Some("managed_deepseek_review_receipt.v1")
        || review.get("status").and_then(Value::as_str) != Some("accepted")
        || review
            .get("material_objection_count")
            .and_then(Value::as_u64)
            != Some(0)
        || review.get("resolved_model").and_then(Value::as_str) != Some("deepseek-v4-pro")
    {
        return Err("deterministic verification and bounded Pro review are required".into());
    }
    let provider_requests = provider_execution
        .get("requests")
        .and_then(Value::as_array)
        .ok_or("provider execution requests are required")?;
    let expected_route = [
        ("planning", "planner", "deepseek-v4-pro"),
        ("implementation", "implementer", "deepseek-v4-flash"),
        ("review", "reviewer", "deepseek-v4-pro"),
    ];
    let mut request_ids = std::collections::HashSet::new();
    let mut summed_request_cost_usd = 0.0;
    let mut summed_cumulative_tokens = 0_u64;
    let requests_valid =
        provider_requests
            .iter()
            .zip(expected_route)
            .all(|(request, (stage, role, model))| {
                let request_id = request
                    .get("request_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty());
                let Ok(usage) =
                    serde_json::from_value::<crate::provider::managed_deepseek::ManagedUsage>(
                        request.get("usage").cloned().unwrap_or(Value::Null),
                    )
                else {
                    return false;
                };
                let Some(request_cost) = request
                    .get("realized_cost_usd")
                    .and_then(Value::as_f64)
                    .filter(|cost| cost.is_finite() && *cost >= 0.0)
                else {
                    return false;
                };
                let Some(new_cumulative_tokens) =
                    summed_cumulative_tokens.checked_add(usage.cumulative_tokens)
                else {
                    return false;
                };
                summed_cumulative_tokens = new_cumulative_tokens;
                summed_request_cost_usd += request_cost;
                request.get("stage").and_then(Value::as_str) == Some(stage)
                    && request.get("role").and_then(Value::as_str) == Some(role)
                    && request.get("protocol").and_then(Value::as_str) == Some("openai_compatible")
                    && request.get("requested_model").and_then(Value::as_str) == Some(model)
                    && request.get("resolved_model").and_then(Value::as_str) == Some(model)
                    && usage.model == model
                    && request_id == Some(usage.request_id.as_str())
                    && request_id.is_some_and(|value| request_ids.insert(value.to_string()))
            });
    let provider_identity_valid = provider_execution
        .get("schema_version")
        .and_then(Value::as_str)
        == Some("managed_deepseek_execution_evidence.v1")
        && provider_execution
            .get("provider_request_count")
            .and_then(Value::as_u64)
            == Some(3)
        && provider_requests.len() == 3
        && provider_execution
            .get("cumulative_tokens")
            .and_then(Value::as_u64)
            .is_some_and(|tokens| {
                // Docs envelope 24k; frozen RWE cell envelope uses schedule max_total_tokens.
                let ceiling = if delegation.task_classes == ["rwe"] {
                    16_000
                } else {
                    24_000
                };
                tokens <= ceiling
            })
        && provider_execution
            .get("realized_cost_usd")
            .and_then(Value::as_f64)
            .is_some_and(|cost| (cost - realized_cost_usd).abs() <= 1e-12)
        && provider_execution
            .get("cumulative_tokens")
            .and_then(Value::as_u64)
            == Some(summed_cumulative_tokens)
        && (summed_request_cost_usd - realized_cost_usd).abs() <= 1e-12
        && requests_valid;
    if !provider_identity_valid {
        return Err("exact provider request, usage, and cost evidence is required".into());
    }
    let changed_files = artifact
        .get("changed_files")
        .and_then(Value::as_array)
        .ok_or("artifact changed_files is required")?;
    if changed_files.len() as u64 > delegation.max_changed_files
        || changed_files.iter().any(|path| {
            let p = path.as_str().unwrap_or("");
            !crate::rwe::frozen_rwe_bindings::path_under_allowed_paths(p, &delegation.allowed_paths)
        })
    {
        return Err("artifact path or file-count policy failed".into());
    }
    let changed_lines = artifact
        .get("changed_lines")
        .and_then(Value::as_u64)
        .ok_or("artifact changed_lines is required")?;
    if changed_lines > delegation.max_changed_lines
        || !realized_cost_usd.is_finite()
        || realized_cost_usd > delegation.max_cost_usd_per_run
    {
        return Err("artifact cost or line-count policy failed".into());
    }
    let artifact_sha = artifact
        .get("artifact_sha256")
        .and_then(Value::as_str)
        .ok_or("artifact_sha256 is required")?;
    if artifact_sha.len() != 64 || !artifact_sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("artifact_sha256 must be 64 hex characters".into());
    }
    Ok(sort_value(&json!({
        "schema_version": DELEGATED_ARTIFACT_CONFIRMATION_SCHEMA_VERSION,
        "manifest_sha256": manifest.get("manifest_sha256"),
        "artifact_sha256": artifact_sha,
        "verification": verification,
        "review": review,
        "provider_execution": provider_execution,
        "target_main_sha": target_main_sha,
        "realized_cost_usd": realized_cost_usd,
        "output": {"draft_pr_only": true, "merged": false, "authorized": true}
    })))
}

fn replay_delegated_artifact_confirmation(
    existing_sha: &str,
    existing_json: &str,
    candidate: &Value,
) -> Result<Value, String> {
    let existing: Value = serde_json::from_str(existing_json).map_err(|error| {
        format!("persisted delegated artifact confirmation is invalid: {error}")
    })?;
    if existing
        .get("artifact_confirmation_sha256")
        .and_then(Value::as_str)
        != Some(existing_sha)
    {
        return Err("persisted delegated artifact confirmation hash is invalid".into());
    }
    let confirmed_at = existing
        .get("confirmed_at")
        .cloned()
        .ok_or("persisted delegated artifact confirmation time is missing")?;
    let mut replay = candidate.clone();
    let object = replay
        .as_object_mut()
        .ok_or("delegated artifact confirmation must be an object")?;
    object.insert("confirmed_at".into(), confirmed_at);
    object.remove("artifact_confirmation_sha256");
    let replay_sha = sha256_hex(canonical_json(&sort_value(&replay))?.as_bytes());
    replay["artifact_confirmation_sha256"] = json!(replay_sha);
    let replay = sort_value(&replay);
    if replay != existing || replay_sha != existing_sha {
        return Err("delegated artifact confirmation replay conflicts".into());
    }
    Ok(existing)
}

fn validate_delegated_manifest_policy(manifest: &Value) -> Result<&str, String> {
    let manifest_sha256 = manifest
        .get("manifest_sha256")
        .and_then(Value::as_str)
        .ok_or("final manifest hash is required")?;
    if manifest_sha256 != compute_attempt_manifest_sha256(manifest)? {
        return Err("final manifest hash mismatch".into());
    }
    let expected_models = json!({
        "planner": "deepseek-v4-pro",
        "implementer": "deepseek-v4-flash",
        "reviewer": "deepseek-v4-pro"
    });
    let expected_output = json!({
        "draft_pr_only": true,
        "target_main_write": false,
        "merge": false,
        "auto_merge": false
    });
    let expected_provider = canonical_managed_deepseek_provider_value();
    let expected_binding =
        crate::rwe::campaign_package::canonical_deepseek_provider_binding().to_json();
    let mutable = manifest
        .pointer("/target/mutable_paths")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let verifier = manifest.pointer("/verifier").and_then(Value::as_str);
    let max_files = manifest
        .pointer("/limits/max_changed_files")
        .and_then(Value::as_u64);
    let max_lines = manifest
        .pointer("/limits/max_changed_lines")
        .and_then(Value::as_u64);
    let max_cost = manifest
        .pointer("/limits/max_cost_usd")
        .and_then(Value::as_f64);
    let max_total_cost = manifest
        .pointer("/limits/max_total_cost_usd")
        .and_then(Value::as_f64);
    let is_deepseek = manifest.pointer("/provider/kind").and_then(Value::as_str)
        == Some(crate::provider::managed_deepseek::DEEPSEEK_PROVIDER_KIND);
    let docs_policy = mutable == ["docs/USER_GUIDE.md"]
        && verifier == Some(crate::rwe::frozen_rwe_bindings::DOCS_GP_VERIFIER_IDENTITY)
        && max_files == Some(1)
        && max_lines == Some(100)
        && ((is_deepseek && max_cost == Some(0.50) && max_total_cost == Some(0.50))
            || (!is_deepseek && max_cost.is_none() && max_total_cost.is_none()));
    let rwe_cell_cost = crate::rwe::operator_corpus::freeze_current_operator_contract_set()
        .ok()
        .and_then(|frozen| {
            frozen
                .schedule
                .body
                .get("cells")
                .and_then(Value::as_array)
                .and_then(|cells| cells.first())
                .and_then(|cell| {
                    crate::rwe::frozen_rwe_bindings::frozen_schedule_cell_max_cost(cell)
                        .ok()
                        .flatten()
                })
        });
    let rwe_policy = {
        let (f, l) =
            crate::rwe::frozen_rwe_bindings::frozen_rwe_max_patch_limits().unwrap_or((0, 0));
        crate::rwe::frozen_rwe_bindings::is_exact_frozen_rwe_allowed_paths(&mutable)
            && verifier == Some(crate::rwe::frozen_rwe_bindings::FROZEN_RWE_VERIFIER_IDENTITY)
            && max_files == Some(f)
            && max_lines == Some(l)
            && ((is_deepseek
                && rwe_cell_cost.is_some()
                && max_cost == rwe_cell_cost
                && max_total_cost == rwe_cell_cost)
                || (!is_deepseek && max_cost.is_none() && max_total_cost.is_none()))
    };
    let valid_models = manifest.get("models") == Some(&expected_models);
    let valid_provider = manifest.get("provider") == Some(&expected_provider)
        && manifest.get("provider_execution_binding") == Some(&expected_binding);
    let policy_matches = manifest
        .pointer("/target/repository")
        .and_then(Value::as_str)
        == Some("Igzela/alters-lab")
        && (docs_policy || rwe_policy)
        && manifest.get("protocol").and_then(Value::as_str) == Some("openai_compatible")
        && valid_models
        && valid_provider
        && {
            // Docs GP uses the classic 3/8k/4k/24k/30s envelope. Frozen RWE cells use
            // the exact schedule cell ceilings (3 requests, 12k/4k/16k tokens, 900s).
            let req = manifest
                .pointer("/limits/max_provider_requests")
                .and_then(Value::as_u64);
            let retries = manifest
                .pointer("/limits/max_retries")
                .and_then(Value::as_u64);
            let input = manifest
                .pointer("/limits/max_input_tokens")
                .and_then(Value::as_u64);
            let output = manifest
                .pointer("/limits/max_output_tokens")
                .and_then(Value::as_u64);
            let cum = manifest
                .pointer("/limits/max_cumulative_tokens")
                .and_then(Value::as_u64);
            let timeout = manifest
                .pointer("/limits/timeout_ms")
                .and_then(Value::as_u64);
            let docs_limits = req == Some(3)
                && retries == Some(0)
                && input == Some(8_000)
                && output == Some(4_000)
                && cum == Some(24_000)
                && timeout == Some(30_000);
            let rwe_limits = req == Some(3)
                && retries == Some(0)
                && input == Some(12_000)
                && output == Some(4_000)
                && cum == Some(16_000)
                && timeout == Some(900_000);
            (docs_policy && docs_limits) || (rwe_policy && rwe_limits)
        }
        && manifest.get("output") == Some(&expected_output);
    if !policy_matches {
        return Err("final manifest is outside the persisted delegation policy".into());
    }
    Ok(manifest_sha256)
}

fn delegated_execution_contract(
    manifest: &Value,
    node_id: &str,
) -> Result<crate::provider::managed_deepseek::PersistedManagedExecutionContract, String> {
    validate_delegated_manifest_policy(manifest)?;
    let provider_binding = crate::rwe::campaign_package::FrozenProviderExecutionBinding::from_json(
        manifest
            .get("provider_execution_binding")
            .ok_or("delegated manifest provider execution binding is missing")?,
    )?;
    let node_ids = manifest
        .pointer("/execution/workflow_node_ids")
        .and_then(Value::as_array)
        .ok_or("delegated manifest workflow_node_ids is missing")?;
    if node_ids.len() != 4 {
        return Err("delegated manifest must bind the exact four-stage route".into());
    }
    let position = node_ids
        .iter()
        .position(|value| value.as_str() == Some(node_id))
        .ok_or("delegated provider node is not manifest-bound")?;
    let model_key = match position {
        0 => "planner",
        1 => "implementer",
        2 => return Err("deterministic verifier cannot receive provider authority".into()),
        3 => "reviewer",
        _ => return Err("delegated provider route position is invalid".into()),
    };
    let requested_model = manifest
        .pointer(&format!("/models/{model_key}"))
        .and_then(Value::as_str)
        .ok_or("delegated manifest model binding is missing")?;
    let price_profile = serde_json::from_value(
        manifest
            .pointer("/provider/price_profile")
            .cloned()
            .ok_or("delegated manifest price profile is missing")?,
    )
    .map_err(|_| "delegated manifest price profile is malformed")?;
    Ok(
        crate::provider::managed_deepseek::PersistedManagedExecutionContract {
            provider_identity: provider_binding.provider_identity,
            provider_kind: manifest
                .pointer("/provider/kind")
                .and_then(Value::as_str)
                .ok_or("delegated manifest provider kind is missing")?
                .into(),
            protocol: crate::provider::managed_deepseek::DeepSeekProtocol::OpenAiCompatible,
            host: manifest
                .pointer("/provider/host")
                .and_then(Value::as_str)
                .ok_or("delegated manifest provider host is missing")?
                .into(),
            base_url: manifest
                .pointer("/provider/base_url")
                .and_then(Value::as_str)
                .ok_or("delegated manifest provider base URL is missing")?
                .into(),
            endpoint_path: manifest
                .pointer("/provider/endpoint_path")
                .and_then(Value::as_str)
                .ok_or("delegated manifest endpoint path is missing")?
                .into(),
            credential_reference: manifest
                .pointer("/provider/credential_reference")
                .and_then(Value::as_str)
                .unwrap_or(crate::provider::managed_deepseek::DEEPSEEK_CREDENTIAL_REFERENCE)
                .into(),
            request_schema_version: manifest
                .pointer("/provider/request_schema_version")
                .and_then(Value::as_str)
                .ok_or("delegated manifest request schema is missing")?
                .into(),
            response_schema_version: manifest
                .pointer("/provider/response_schema_version")
                .and_then(Value::as_str)
                .ok_or("delegated manifest response schema is missing")?
                .into(),
            usage_parser_version: manifest
                .pointer("/provider/usage_parser_version")
                .and_then(Value::as_str)
                .ok_or("delegated manifest usage parser is missing")?
                .into(),
            requested_model: requested_model.into(),
            // The exact frozen cell envelope from the validated manifest (docs GP
            // 8k/4k/24k/30s/0.50; frozen RWE cells 12k/4k/16k/900s/0.20). Never
            // invent or relax the provider ceiling outside the persisted manifest.
            limits: crate::provider::managed_deepseek::ManagedCallLimits {
                max_requests: manifest
                    .pointer("/limits/max_provider_requests")
                    .and_then(Value::as_u64)
                    .unwrap_or(3),
                max_retries: manifest
                    .pointer("/limits/max_retries")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                max_input_tokens: manifest
                    .pointer("/limits/max_input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(8_000),
                max_output_tokens: manifest
                    .pointer("/limits/max_output_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(4_000),
                max_cumulative_tokens: manifest
                    .pointer("/limits/max_cumulative_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(24_000),
                timeout_ms: manifest
                    .pointer("/limits/timeout_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(30_000),
                max_cost_usd: manifest
                    .pointer("/limits/max_cost_usd")
                    .and_then(Value::as_f64),
            },
            price_profile,
        },
    )
}

fn validate_manifest_approval_row(
    manifest: &Value,
    now: &str,
    delegation_sha256: &str,
    status: &str,
    expires_at: &str,
    approver_id: &str,
    authenticated_approver_id: &str,
) -> Result<(), String> {
    if status != "active" || is_at_or_before(expires_at, now)? {
        return Err("delegation is not active".into());
    }
    if manifest.get("delegation_sha256").and_then(Value::as_str) != Some(delegation_sha256)
        || approver_id != authenticated_approver_id
    {
        return Err("delegated manifest approver authority is stale or mismatched".into());
    }
    Ok(())
}

fn delegated_manifest_approval_receipt(
    delegation_id: &str,
    manifest_sha256: &str,
    delegation_sha256: &str,
    approver_id: &str,
    created_at: &str,
    expires_at: &str,
) -> Result<Value, String> {
    let mut receipt = sort_value(&json!({
        "schema_version": DELEGATED_MANIFEST_APPROVAL_SCHEMA_VERSION,
        "approval_id": format!("delegated-manifest-approval-{}", &manifest_sha256[..16]),
        "delegation_id": delegation_id,
        "delegation_sha256": delegation_sha256,
        "manifest_sha256": manifest_sha256,
        "approver_id": approver_id,
        "decision": "approved",
        "one_use": true,
        "created_at": created_at,
        "expires_at": expires_at
    }));
    let receipt_sha256 = sha256_hex(canonical_json(&receipt)?.as_bytes());
    receipt["approval_receipt_sha256"] = json!(receipt_sha256);
    Ok(sort_value(&receipt))
}

fn validate_existing_manifest_approval(
    receipt_sha256: &str,
    receipt_json: &str,
    delegation_id: &str,
    manifest_sha256: &str,
    approver_id: &str,
) -> Result<Value, String> {
    let receipt: Value = serde_json::from_str(receipt_json)
        .map_err(|_| "persisted delegated manifest approval is invalid JSON".to_string())?;
    let mut unhashed = receipt.clone();
    unhashed
        .as_object_mut()
        .ok_or("persisted delegated manifest approval must be an object")?
        .remove("approval_receipt_sha256");
    let computed = sha256_hex(canonical_json(&unhashed)?.as_bytes());
    if computed != receipt_sha256
        || receipt
            .get("approval_receipt_sha256")
            .and_then(Value::as_str)
            != Some(receipt_sha256)
        || receipt.get("delegation_id").and_then(Value::as_str) != Some(delegation_id)
        || receipt.get("manifest_sha256").and_then(Value::as_str) != Some(manifest_sha256)
        || receipt.get("approver_id").and_then(Value::as_str) != Some(approver_id)
        || receipt.get("decision").and_then(Value::as_str) != Some("approved")
    {
        return Err("persisted delegated manifest approval receipt is stale or malformed".into());
    }
    Ok(receipt)
}

fn record_workspace_action_in_journal(
    journal_json: &str,
    binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    receipt: &Value,
) -> Result<(String, String), String> {
    let mut journal: Vec<Value> = serde_json::from_str(journal_json)
        .map_err(|_| "managed workspace action journal is invalid")?;
    let entry = journal
        .iter_mut()
        .find(|entry| {
            entry.get("node_id").and_then(Value::as_str) == Some(binding.node_id.as_str())
        })
        .ok_or("managed workspace action provider journal entry is missing")?;
    if entry.get("status").and_then(Value::as_str) != Some("succeeded") {
        return Err("managed workspace action requires a successful provider journal entry".into());
    }
    if entry.get("workspace_action_failure").is_some() {
        return Err("managed workspace action already has a failure outcome".into());
    }
    let receipt_sha256 = sha256_hex(canonical_json(&sort_value(receipt))?.as_bytes());
    if let Some(existing) = entry.get("workspace_action") {
        if existing.get("receipt_sha256").and_then(Value::as_str) == Some(receipt_sha256.as_str()) {
            return Ok((
                sort_value(&Value::Array(journal)).to_string(),
                receipt_sha256,
            ));
        }
        return Err("managed workspace action receipt conflicts with the persisted receipt".into());
    }
    entry["workspace_action"] = sort_value(&json!({
        "schema_version": "managed_workspace_action_evidence.v1",
        "receipt_sha256": receipt_sha256,
        "receipt": receipt,
    }));
    Ok((
        sort_value(&Value::Array(journal)).to_string(),
        receipt_sha256,
    ))
}

fn record_workspace_action_failure_in_journal(
    journal_json: &str,
    binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    failure_sha256: &str,
) -> Result<(String, String), String> {
    let mut journal: Vec<Value> = serde_json::from_str(journal_json)
        .map_err(|_| "managed workspace action journal is invalid")?;
    let entry = journal
        .iter_mut()
        .find(|entry| {
            entry.get("node_id").and_then(Value::as_str) == Some(binding.node_id.as_str())
        })
        .ok_or("managed workspace action provider journal entry is missing")?;
    if entry.get("status").and_then(Value::as_str) != Some("succeeded") {
        return Err(
            "managed workspace action failure requires a successful provider journal entry".into(),
        );
    }
    if entry.get("workspace_action").is_some() {
        return Err("managed workspace action already has a successful receipt".into());
    }
    let existing = entry.get("workspace_action_failure");
    if let Some(existing) = existing {
        if existing.get("failure_sha256").and_then(Value::as_str) == Some(failure_sha256) {
            return Ok((
                sort_value(&Value::Array(journal)).to_string(),
                failure_sha256.into(),
            ));
        }
        return Err("managed workspace action failure conflicts with the persisted outcome".into());
    }
    let evidence = sort_value(&json!({
        "schema_version": "managed_workspace_action_failure.v1",
        "product_task_id": binding.product_task_id,
        "workflow_id": binding.workflow_id,
        "node_id": binding.node_id,
        "failure_sha256": failure_sha256,
        "replay": "forbidden",
    }));
    entry["workspace_action_failure"] = evidence.clone();
    Ok((
        sort_value(&Value::Array(journal)).to_string(),
        sha256_hex(canonical_json(&evidence)?.as_bytes()),
    ))
}

impl LocalProductStore {
    pub(crate) fn require_delegation_product_task(
        &self,
        delegation_id: &str,
        expected_tenant_id: &str,
        expected_product_task_id: &str,
    ) -> Result<(), String> {
        let row: Option<(String, Option<String>)> = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT tenant_id, product_task_id
                     FROM managed_acceptance_delegations WHERE delegation_id=?1",
                    params![delegation_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT tenant_id, product_task_id
                         FROM managed_acceptance_delegations WHERE delegation_id=$1",
                        &[&delegation_id],
                    )
                    .map(|row| row.map(|row| (row.get(0), row.get(1))))
                    .map_err(|error| error.to_string())
            })?,
        };
        let Some((stored_tenant, stored_product_task_id)) = row else {
            return Err("delegation is not persisted".into());
        };
        if stored_tenant != expected_tenant_id {
            return Err("delegation tenant does not match ProductTask tenant".into());
        }
        if stored_product_task_id.as_deref() != Some(expected_product_task_id) {
            return Err("delegation ProductTask binding is stale or mismatched".into());
        }
        Ok(())
    }

    /// Validate the ProductTask identity carried by a delegated manifest.
    /// Every downstream production authority operation requires the immutable
    /// store-owned binding; a caller cannot manufacture a task identity in a
    /// manifest or use an unbound legacy delegation.
    pub(crate) fn require_manifest_product_task_binding(
        &self,
        delegation_id: &str,
        expected_tenant_id: &str,
        manifest: &Value,
    ) -> Result<String, String> {
        let execution = manifest
            .get("execution")
            .ok_or("delegated manifest execution is missing")?;
        let product_task_id = execution
            .get("product_task_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("delegated manifest ProductTask identity is missing")?;
        let manifest_tenant_id = execution
            .get("tenant_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("delegated manifest tenant identity is missing")?;
        if manifest_tenant_id != expected_tenant_id {
            return Err("delegated manifest tenant does not match authenticated principal".into());
        }

        let binding: Option<(String, Option<String>)> = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT tenant_id, product_task_id
                     FROM managed_acceptance_delegations WHERE delegation_id=?1",
                    params![delegation_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT tenant_id, product_task_id
                         FROM managed_acceptance_delegations WHERE delegation_id=$1",
                        &[&delegation_id],
                    )
                    .map(|row| row.map(|row| (row.get(0), row.get(1))))
                    .map_err(|error| error.to_string())
            })?,
        };
        let Some((stored_tenant_id, stored_product_task_id)) = binding else {
            return Err("delegation is not persisted".into());
        };
        if stored_tenant_id != expected_tenant_id {
            return Err("delegation tenant does not match authenticated principal".into());
        }
        if let Some(stored_product_task_id) = stored_product_task_id {
            if stored_product_task_id != product_task_id {
                return Err("delegation ProductTask binding is stale or mismatched".into());
            }
        } else {
            return Err("delegation ProductTask binding is missing".into());
        }
        Ok(product_task_id.to_string())
    }

    /// Persist the operator-issued delegation. Re-submitting the identical
    /// body is an idempotent replay; any same-id/hash conflict fails closed.
    pub fn persist_delegation(
        &self,
        principal: &AuthenticatedPrincipal,
        delegation: &DelegationContract,
    ) -> Result<Value, String> {
        self.persist_delegation_with_product_task(principal, delegation, None)
    }

    /// Persist a delegation and its ProductTask identity in one store-owned
    /// transaction. The live product route uses this form so a restart cannot
    /// leave an operation with a caller-selected first task binding.
    pub fn persist_delegation_for_product_task(
        &self,
        principal: &AuthenticatedPrincipal,
        task_id: &str,
        delegation: &DelegationContract,
    ) -> Result<Value, String> {
        self.persist_delegation_with_product_task(principal, delegation, Some(task_id))
    }

    fn persist_delegation_with_product_task(
        &self,
        principal: &AuthenticatedPrincipal,
        delegation: &DelegationContract,
        product_task_id: Option<&str>,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_DELEGATED_AUTONOMY)?;
        // Preparing the immutable delegation and reserving spend is not the
        // execution admission effect. Keep attempt admission with the
        // output operator's activate step so the manifest/spend reviewer does
        // not receive execution authority merely to prepare a run.
        if matches!(principal.principal_kind(), PrincipalKind::FixturePrincipal) {
            return Err("delegation requires an authenticated non-fixture operator".into());
        }
        let now = self.now();
        delegation.validate(&now)?;
        let delegation_sha256 = delegation.sha256()?;
        principal.require_scope(SCOPE_DELEGATED_MANIFEST_APPROVE)?;
        principal.require_scope(SCOPE_SPEND_AUTHORIZE)?;
        let manifest_approver_id = principal.principal_id().to_string();
        let artifact_confirmer_id = String::new();
        let body = delegation.body();
        let body_json = body.to_string();
        let existing = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT delegation_sha256, body_json, status, manifest_approver_id,
                            artifact_confirmer_id, product_task_id
                     FROM managed_acceptance_delegations WHERE delegation_id=?1",
                    params![delegation.delegation_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, Option<String>>(5)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .map(|(sha, body_json, status, manifest_approver_id, artifact_confirmer_id, product_task_id)| json!({
                    "schema_version": DELEGATION_SCHEMA_VERSION,
                    "delegation_id": delegation.delegation_id,
                    "delegation_sha256": sha,
                    "body_json": serde_json::from_str::<Value>(&body_json).unwrap_or(Value::Null),
                    "status": status,
                    "manifest_approver_id": manifest_approver_id,
                    "artifact_confirmer_id": artifact_confirmer_id,
                    "product_task_id": product_task_id,
                    "replayed": true
                })))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT delegation_sha256, body_json, status, manifest_approver_id,
                                artifact_confirmer_id, product_task_id
                         FROM managed_acceptance_delegations WHERE delegation_id=$1",
                        &[&delegation.delegation_id],
                    )
                    .map(|row| {
                        row.map(|row| {
                            let body_json: String = row.get(1);
                            json!({
                                "schema_version": DELEGATION_SCHEMA_VERSION,
                                "delegation_id": delegation.delegation_id,
                                "delegation_sha256": row.get::<_, String>(0),
                                "body_json": serde_json::from_str::<Value>(&body_json).unwrap_or(Value::Null),
                                "status": row.get::<_, String>(2),
                                "manifest_approver_id": row.get::<_, String>(3),
                                "artifact_confirmer_id": row.get::<_, String>(4),
                                "product_task_id": row.get::<_, Option<String>>(5),
                                "replayed": true
                            })
                        })
                    })
                    .map_err(|e| e.to_string())
            }),
        };
        if let Some(existing) = existing? {
            if existing.get("delegation_sha256").and_then(Value::as_str)
                != Some(delegation_sha256.as_str())
                || existing.get("body_json") != Some(&body)
                || existing.get("manifest_approver_id").and_then(Value::as_str)
                    != Some(manifest_approver_id.as_str())
                || existing
                    .get("artifact_confirmer_id")
                    .and_then(Value::as_str)
                    != Some(artifact_confirmer_id.as_str())
                || existing.get("product_task_id").and_then(Value::as_str) != product_task_id
            {
                return Err("delegation replay conflicts with persisted immutable body".into());
            }
            return Ok(existing);
        }
        let inserted = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                if let Some(task_id) = product_task_id {
                    let task_tenant: Option<String> = tx
                        .query_row(
                            "SELECT tenant_id FROM product_tasks WHERE task_id=?1",
                            params![task_id],
                            |row| row.get(0),
                        )
                        .optional()
                        .map_err(|e| e.to_string())?;
                    if task_tenant.as_deref() != Some(principal.tenant_id()) {
                        return Err(
                            "ProductTask is missing or does not match delegation principal".into(),
                        );
                    }
                }
                let inserted = tx.execute(
                    "INSERT OR IGNORE INTO managed_acceptance_delegations (
                        delegation_id, tenant_id, principal_kind, principal_id, manifest_approver_id,
                        artifact_confirmer_id, product_task_id,
                        delegation_sha256, body_json, status, executions_allowed, executions_used,
                        max_total_cost_usd, total_cost_usd, created_at, updated_at, expires_at,
                        terminal_at, revoked_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'active',?10,0,?11,0,?12,?12,?13,NULL,NULL)",
                    params![
                        delegation.delegation_id,
                        principal.tenant_id(),
                        principal.principal_kind().as_str(),
                        principal.principal_id(),
                        manifest_approver_id,
                        artifact_confirmer_id,
                        product_task_id,
                        delegation_sha256,
                        body_json,
                        delegation.executions as i64,
                        delegation.max_total_cost_usd,
                        now,
                        delegation.expires_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                if inserted == 0 {
                    let row: (String, String, String, String, Option<String>) = tx
                        .query_row(
                            "SELECT delegation_sha256, body_json, manifest_approver_id,
                                    artifact_confirmer_id, product_task_id
                             FROM managed_acceptance_delegations WHERE delegation_id=?1",
                            params![delegation.delegation_id],
                            |row| {
                                Ok((
                                    row.get(0)?,
                                    row.get(1)?,
                                    row.get(2)?,
                                    row.get(3)?,
                                    row.get(4)?,
                                ))
                            },
                        )
                        .map_err(|e| e.to_string())?;
                    if row.0 != delegation_sha256
                        || row.1 != body_json
                        || row.2 != manifest_approver_id
                        || row.3 != artifact_confirmer_id
                        || row.4.as_deref() != product_task_id
                    {
                        return Err(
                            "delegation replay conflicts with persisted immutable body".into()
                        );
                    }
                }
                tx.commit().map_err(|e| e.to_string())?;
                Ok(inserted == 1)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                if let Some(task_id) = product_task_id {
                    let task_tenant: Option<String> = tx
                        .query_opt(
                            "SELECT tenant_id FROM product_tasks WHERE task_id=$1 FOR SHARE",
                            &[&task_id],
                        )
                        .map(|row| row.map(|row| row.get(0)))
                        .map_err(|e| e.to_string())?;
                    if task_tenant.as_deref() != Some(principal.tenant_id()) {
                        return Err(
                            "ProductTask is missing or does not match delegation principal".into(),
                        );
                    }
                }
                let inserted = tx.execute(
                    "INSERT INTO managed_acceptance_delegations (
                        delegation_id, tenant_id, principal_kind, principal_id, manifest_approver_id,
                        artifact_confirmer_id, product_task_id,
                        delegation_sha256, body_json, status, executions_allowed, executions_used,
                        max_total_cost_usd, total_cost_usd, created_at, updated_at, expires_at,
                        terminal_at, revoked_at
                     ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'active',$10,0,$11,0,$12,$12,$13,NULL,NULL)
                     ON CONFLICT DO NOTHING",
                    &[
                        &delegation.delegation_id,
                        &principal.tenant_id(),
                        &principal.principal_kind().as_str(),
                        &principal.principal_id(),
                        &manifest_approver_id,
                        &artifact_confirmer_id,
                        &product_task_id,
                        &delegation_sha256,
                        &body_json,
                        &(delegation.executions as i64),
                        &delegation.max_total_cost_usd,
                        &now,
                        &delegation.expires_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                if inserted == 0 {
                    let row = tx
                        .query_one(
                            "SELECT delegation_sha256, body_json, manifest_approver_id,
                                    artifact_confirmer_id, product_task_id
                             FROM managed_acceptance_delegations WHERE delegation_id=$1 FOR UPDATE",
                            &[&delegation.delegation_id],
                        )
                        .map_err(|e| e.to_string())?;
                    if row.get::<_, String>(0) != delegation_sha256
                        || row.get::<_, String>(1) != body_json
                        || row.get::<_, String>(2) != manifest_approver_id
                        || row.get::<_, String>(3) != artifact_confirmer_id
                        || row.get::<_, Option<String>>(4).as_deref() != product_task_id
                    {
                        return Err(
                            "delegation replay conflicts with persisted immutable body".into()
                        );
                    }
                }
                tx.commit().map_err(|e| e.to_string())?;
                Ok(inserted == 1)
            }),
        }?;
        Ok(json!({
            "schema_version": DELEGATION_SCHEMA_VERSION,
            "delegation_id": delegation.delegation_id,
            "delegation_sha256": delegation_sha256,
            "status": "active",
            "manifest_approver_id": manifest_approver_id,
            "artifact_confirmer_id": artifact_confirmer_id,
            "product_task_id": product_task_id,
            "execution_granted": false,
            "replayed": !inserted
        }))
    }

    /// Bind an admitted ProductTask to the same immutable delegation that the
    /// prepare route just persisted. The binding is store-owned and monotonic:
    /// it can be filled once, or replayed for the same task and principal, but
    /// it cannot be moved to another task or principal.
    #[cfg(test)]
    pub(crate) fn bind_delegation_to_product_task(
        &self,
        principal: &AuthenticatedPrincipal,
        task_id: &str,
        delegation_id: &str,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_DELEGATED_AUTONOMY)?;
        principal.require_scope(SCOPE_DELEGATED_MANIFEST_APPROVE)?;
        principal.require_scope(SCOPE_SPEND_AUTHORIZE)?;
        if matches!(principal.principal_kind(), PrincipalKind::FixturePrincipal) {
            return Err(
                "delegation ProductTask binding requires an authenticated non-fixture operator"
                    .into(),
            );
        }
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        if task.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
            return Err("ProductTask tenant does not match delegation principal".into());
        }
        let now = self.now();
        let details = json!({
            "task_id": task_id,
            "delegation_id": delegation_id,
            "principal_id": principal.principal_id(),
        });
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let row: (String, String, Option<String>, String, String, String) = tx
                    .query_row(
                        "SELECT tenant_id, status, product_task_id, principal_id,
                                manifest_approver_id, expires_at
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation_id],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                            ))
                        },
                    )
                    .map_err(|error| error.to_string())?;
                if row.0 != principal.tenant_id() {
                    return Err("delegation tenant does not match ProductTask tenant".into());
                }
                if row.1 != "active" {
                    return Err("delegation is not active for ProductTask binding".into());
                }
                if is_at_or_before(&row.5, &now)? {
                    return Err("delegation is expired for ProductTask binding".into());
                }
                if row.3 != principal.principal_id() || row.4 != principal.principal_id() {
                    return Err("delegation principal does not match ProductTask binder".into());
                }
                if let Some(bound_task_id) = row.2.as_deref() {
                    if bound_task_id != task_id {
                        return Err(
                            "delegation ProductTask binding does not match requested task".into(),
                        );
                    }
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(json!({
                        "delegation_id": delegation_id,
                        "product_task_id": task_id,
                        "replayed": true,
                    }));
                }
                let safe_state: i64 = tx
                    .query_row(
                        "SELECT EXISTS(
                            SELECT 1 FROM managed_acceptance_delegations
                             WHERE delegation_id=?1 AND tenant_id=?2 AND status='active'
                               AND product_task_id IS NULL AND executions_used=0
                               AND total_cost_usd=0 AND proposal_sha256 IS NULL
                               AND proposal_json IS NULL AND spend_authorization_id IS NULL
                               AND manifest_approval_sha256 IS NULL
                               AND manifest_approval_json IS NULL AND spend_body_sha256 IS NULL
                               AND spend_status IS NULL AND spend_body_json IS NULL
                               AND manifest_json IS NULL AND attempt_id IS NULL
                               AND attempt_lease_id IS NULL AND attempt_lease_token IS NULL
                               AND attempt_status IS NULL AND attempt_activator_id IS NULL
                               AND artifact_confirmation_sha256 IS NULL
                               AND artifact_confirmation_json IS NULL
                               AND provider_request_journal_json='[]'
                               AND terminal_receipt_json IS NULL AND terminal_at IS NULL
                               AND revoked_at IS NULL AND artifact_confirmer_id=''
                         )",
                        params![delegation_id, principal.tenant_id()],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                if safe_state != 1 {
                    return Err(
                        "delegation ProductTask binding requires untouched pre-admission state"
                            .into(),
                    );
                }
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET product_task_id=?1, updated_at=?2
                         WHERE delegation_id=?3 AND tenant_id=?4 AND status='active'
                           AND product_task_id IS NULL AND executions_used=0 AND total_cost_usd=0
                           AND proposal_sha256 IS NULL AND proposal_json IS NULL
                           AND spend_authorization_id IS NULL AND manifest_approval_sha256 IS NULL
                           AND manifest_approval_json IS NULL AND spend_body_sha256 IS NULL
                           AND spend_status IS NULL AND spend_body_json IS NULL
                           AND manifest_json IS NULL AND attempt_id IS NULL
                           AND attempt_lease_id IS NULL AND attempt_lease_token IS NULL
                           AND attempt_status IS NULL AND attempt_activator_id IS NULL
                           AND artifact_confirmation_sha256 IS NULL
                           AND artifact_confirmation_json IS NULL
                           AND provider_request_journal_json='[]'
                           AND terminal_receipt_json IS NULL AND terminal_at IS NULL
                           AND revoked_at IS NULL AND artifact_confirmer_id=''",
                        params![task_id, now, delegation_id, principal.tenant_id()],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err(
                        "delegation ProductTask binding lost its pre-admission state".into(),
                    );
                }
                append_audit_locked(
                    &tx,
                    &now,
                    principal.principal_id(),
                    "managed_acceptance.delegation_product_task_bound",
                    delegation_id,
                    &details,
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(json!({
                    "delegation_id": delegation_id,
                    "product_task_id": task_id,
                    "replayed": false,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let row = tx
                    .query_one(
                        "SELECT tenant_id, status, product_task_id, principal_id,
                                manifest_approver_id, expires_at
                         FROM managed_acceptance_delegations WHERE delegation_id=$1 FOR UPDATE",
                        &[&delegation_id],
                    )
                    .map_err(|error| error.to_string())?;
                let stored_tenant: String = row.get(0);
                let status: String = row.get(1);
                let stored_task_id: Option<String> = row.get(2);
                let stored_principal_id: String = row.get(3);
                let stored_manifest_approver_id: String = row.get(4);
                let expires_at: String = row.get(5);
                if stored_tenant != principal.tenant_id() {
                    return Err("delegation tenant does not match ProductTask tenant".into());
                }
                if status != "active" {
                    return Err("delegation is not active for ProductTask binding".into());
                }
                if is_at_or_before(&expires_at, &now)? {
                    return Err("delegation is expired for ProductTask binding".into());
                }
                if stored_principal_id != principal.principal_id()
                    || stored_manifest_approver_id != principal.principal_id()
                {
                    return Err("delegation principal does not match ProductTask binder".into());
                }
                if let Some(bound_task_id) = stored_task_id.as_deref() {
                    if bound_task_id != task_id {
                        return Err(
                            "delegation ProductTask binding does not match requested task".into(),
                        );
                    }
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(json!({
                        "delegation_id": delegation_id,
                        "product_task_id": task_id,
                        "replayed": true,
                    }));
                }
                let safe_state: bool = tx
                    .query_one(
                        "SELECT EXISTS(
                            SELECT 1 FROM managed_acceptance_delegations
                             WHERE delegation_id=$1 AND tenant_id=$2 AND status='active'
                               AND product_task_id IS NULL AND executions_used=0
                               AND total_cost_usd=0 AND proposal_sha256 IS NULL
                               AND proposal_json IS NULL AND spend_authorization_id IS NULL
                               AND manifest_approval_sha256 IS NULL
                               AND manifest_approval_json IS NULL AND spend_body_sha256 IS NULL
                               AND spend_status IS NULL AND spend_body_json IS NULL
                               AND manifest_json IS NULL AND attempt_id IS NULL
                               AND attempt_lease_id IS NULL AND attempt_lease_token IS NULL
                               AND attempt_status IS NULL AND attempt_activator_id IS NULL
                               AND artifact_confirmation_sha256 IS NULL
                               AND artifact_confirmation_json IS NULL
                               AND provider_request_journal_json='[]'
                               AND terminal_receipt_json IS NULL AND terminal_at IS NULL
                               AND revoked_at IS NULL AND artifact_confirmer_id=''
                         )",
                        &[&delegation_id, &principal.tenant_id()],
                    )
                    .map(|row| row.get(0))
                    .map_err(|error| error.to_string())?;
                if !safe_state {
                    return Err(
                        "delegation ProductTask binding requires untouched pre-admission state"
                            .into(),
                    );
                }
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET product_task_id=$1, updated_at=$2
                         WHERE delegation_id=$3 AND tenant_id=$4 AND status='active'
                           AND product_task_id IS NULL AND executions_used=0 AND total_cost_usd=0
                           AND proposal_sha256 IS NULL AND proposal_json IS NULL
                           AND spend_authorization_id IS NULL AND manifest_approval_sha256 IS NULL
                           AND manifest_approval_json IS NULL AND spend_body_sha256 IS NULL
                           AND spend_status IS NULL AND spend_body_json IS NULL
                           AND manifest_json IS NULL AND attempt_id IS NULL
                           AND attempt_lease_id IS NULL AND attempt_lease_token IS NULL
                           AND attempt_status IS NULL AND attempt_activator_id IS NULL
                           AND artifact_confirmation_sha256 IS NULL
                           AND artifact_confirmation_json IS NULL
                           AND provider_request_journal_json='[]'
                           AND terminal_receipt_json IS NULL AND terminal_at IS NULL
                           AND revoked_at IS NULL AND artifact_confirmer_id=''",
                        &[&task_id, &now, &delegation_id, &principal.tenant_id()],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err(
                        "delegation ProductTask binding lost its pre-admission state".into(),
                    );
                }
                let details_json = details.to_string();
                tx.execute(
                    "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                     VALUES ($1, $2, 'managed_acceptance.delegation_product_task_bound', $3, $4)",
                    &[
                        &now,
                        &principal.principal_id(),
                        &delegation_id,
                        &details_json,
                    ],
                )
                .map_err(|error| error.to_string())?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(json!({
                    "delegation_id": delegation_id,
                    "product_task_id": task_id,
                    "replayed": false,
                }))
            }),
        }
    }

    /// Bind the separately owner-approved proposal to this delegation exactly
    /// once. A self-consistent replacement proposal is still rejected because
    /// the approved external hash is a fixed authority input.
    pub fn persist_approved_delegated_proposal(
        &self,
        delegation_id: &str,
        proposal: &Value,
        expected_approved_sha256: &str,
    ) -> Result<Value, String> {
        if proposal.get("manifest_sha256").and_then(Value::as_str) != Some(expected_approved_sha256)
            || compute_attempt_manifest_sha256(proposal)? != expected_approved_sha256
        {
            return Err("delegated proposal does not match the owner-approved manifest".into());
        }
        let proposal_json = proposal.to_string();
        let now = self.now();
        let replayed = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let row: (String, Option<String>, Option<String>) = tx
                    .query_row(
                        "SELECT status, proposal_sha256, proposal_json
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .map_err(|error| error.to_string())?;
                if let (Some(stored_sha), Some(stored_json)) = (row.1, row.2) {
                    if stored_sha != expected_approved_sha256 || stored_json != proposal_json {
                        return Err(
                            "delegated proposal conflicts with persisted immutable proposal".into()
                        );
                    }
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(true);
                }
                if row.0 != "active" {
                    return Err("delegation is not active for proposal binding".into());
                }
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET proposal_sha256=?1, proposal_json=?2, updated_at=?3
                         WHERE delegation_id=?4 AND proposal_sha256 IS NULL AND proposal_json IS NULL",
                        params![
                            expected_approved_sha256,
                            proposal_json,
                            now,
                            delegation_id
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("delegated proposal binding lost its one-use authority".into());
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(false)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let row = tx
                    .query_one(
                        "SELECT status, proposal_sha256, proposal_json
                         FROM managed_acceptance_delegations WHERE delegation_id=$1 FOR UPDATE",
                        &[&delegation_id],
                    )
                    .map_err(|error| error.to_string())?;
                let status: String = row.get(0);
                let stored_sha: Option<String> = row.get(1);
                let stored_json: Option<String> = row.get(2);
                if let (Some(stored_sha), Some(stored_json)) = (stored_sha, stored_json) {
                    if stored_sha != expected_approved_sha256 || stored_json != proposal_json {
                        return Err(
                            "delegated proposal conflicts with persisted immutable proposal".into()
                        );
                    }
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(true);
                }
                if status != "active" {
                    return Err("delegation is not active for proposal binding".into());
                }
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET proposal_sha256=$1, proposal_json=$2, updated_at=$3
                         WHERE delegation_id=$4 AND proposal_sha256 IS NULL AND proposal_json IS NULL",
                        &[
                            &expected_approved_sha256,
                            &proposal_json,
                            &now,
                            &delegation_id,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("delegated proposal binding lost its one-use authority".into());
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(false)
            }),
        }?;
        Ok(json!({
            "schema_version": "managed_approved_proposal_receipt.v1",
            "delegation_id": delegation_id,
            "proposal_manifest_sha256": expected_approved_sha256,
            "replayed": replayed,
        }))
    }

    pub(crate) fn require_approved_delegated_proposal(
        &self,
        delegation_id: &str,
        proposal: &Value,
    ) -> Result<(), String> {
        let stored: (String, String) = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT proposal_sha256, proposal_json
                     FROM managed_acceptance_delegations WHERE delegation_id=?1",
                    params![delegation_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT proposal_sha256, proposal_json
                         FROM managed_acceptance_delegations WHERE delegation_id=$1",
                        &[&delegation_id],
                    )
                    .map(|row| (row.get(0), row.get(1)))
                    .map_err(|error| error.to_string())
            }),
        }?;
        let proposal_json = proposal.to_string();
        if proposal.get("manifest_sha256").and_then(Value::as_str) != Some(stored.0.as_str())
            || compute_attempt_manifest_sha256(proposal)? != stored.0
            || stored.1.as_bytes() != proposal_json.as_bytes()
        {
            return Err("delegated proposal is not the immutable approved proposal".into());
        }
        Ok(())
    }

    /// The separated delegated manifest approver checks the exact final
    /// manifest against the immutable persisted delegation and records a
    /// hash-bound approval receipt. It cannot admit an attempt or confirm an
    /// output artifact.
    pub fn approve_delegated_manifest(
        &self,
        principal: &AuthenticatedPrincipal,
        delegation_id: &str,
        manifest: &Value,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_DELEGATED_MANIFEST_APPROVE)?;
        principal.require_scope(SCOPE_SPEND_AUTHORIZE)?;
        if matches!(principal.principal_kind(), PrincipalKind::FixturePrincipal) {
            return Err("fixture principal cannot approve a production delegated manifest".into());
        }
        let manifest_sha256 = validate_delegated_manifest_policy(manifest)?;
        self.require_manifest_product_task_binding(delegation_id, principal.tenant_id(), manifest)?;
        self.require_final_manifest_approved_proposal(delegation_id, manifest)?;
        let now = self.now();
        let receipt = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let row: (String, String, String, String, Option<String>, Option<String>) = tx
                    .query_row(
                        "SELECT delegation_sha256, status, expires_at, manifest_approver_id, manifest_approval_sha256, manifest_approval_json FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
                    )
                    .map_err(|e| e.to_string())?;
                validate_manifest_approval_row(
                    manifest,
                    &now,
                    &row.0,
                    &row.1,
                    &row.2,
                    &row.3,
                    principal.principal_id(),
                )?;
                if let (Some(existing_sha), Some(existing_json)) = (&row.4, &row.5) {
                    let receipt = validate_existing_manifest_approval(
                        existing_sha,
                        existing_json,
                        delegation_id,
                        manifest_sha256,
                        &row.3,
                    )?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(receipt);
                }
                let receipt = delegated_manifest_approval_receipt(
                    delegation_id,
                    manifest_sha256,
                    &row.0,
                    &row.3,
                    &now,
                    &row.2,
                )?;
                let receipt_sha256 = receipt
                    .get("approval_receipt_sha256")
                    .and_then(Value::as_str)
                    .ok_or("delegated approval receipt hash missing")?;
                tx.execute(
                    "UPDATE managed_acceptance_delegations SET manifest_approval_sha256=?1, manifest_approval_json=?2, updated_at=?3 WHERE delegation_id=?4 AND manifest_approval_sha256 IS NULL",
                    params![receipt_sha256, receipt.to_string(), now, delegation_id],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(receipt)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let row = tx
                    .query_one(
                        "SELECT delegation_sha256, status, expires_at, manifest_approver_id, manifest_approval_sha256, manifest_approval_json FROM managed_acceptance_delegations WHERE delegation_id=$1 FOR UPDATE",
                        &[&delegation_id],
                    )
                    .map_err(|e| e.to_string())?;
                let delegation_sha: String = row.get(0);
                let status: String = row.get(1);
                let expires_at: String = row.get(2);
                let approver_id: String = row.get(3);
                validate_manifest_approval_row(
                    manifest,
                    &now,
                    &delegation_sha,
                    &status,
                    &expires_at,
                    &approver_id,
                    principal.principal_id(),
                )?;
                let existing_sha: Option<String> = row.get(4);
                let existing_json: Option<String> = row.get(5);
                if let (Some(existing_sha), Some(existing_json)) =
                    (&existing_sha, &existing_json)
                {
                    let receipt = validate_existing_manifest_approval(
                        existing_sha,
                        existing_json,
                        delegation_id,
                        manifest_sha256,
                        &approver_id,
                    )?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(receipt);
                }
                let receipt = delegated_manifest_approval_receipt(
                    delegation_id,
                    manifest_sha256,
                    &delegation_sha,
                    &approver_id,
                    &now,
                    &expires_at,
                )?;
                let receipt_sha256 = receipt
                    .get("approval_receipt_sha256")
                    .and_then(Value::as_str)
                    .ok_or("delegated approval receipt hash missing")?;
                tx.execute(
                    "UPDATE managed_acceptance_delegations SET manifest_approval_sha256=$1, manifest_approval_json=$2, updated_at=$3 WHERE delegation_id=$4 AND manifest_approval_sha256 IS NULL",
                    &[&receipt_sha256, &receipt.to_string(), &now, &delegation_id],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(receipt)
            }),
        }?;
        Ok(receipt)
    }

    fn require_final_manifest_approved_proposal(
        &self,
        delegation_id: &str,
        manifest: &Value,
    ) -> Result<(), String> {
        let approved_sha: String = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT proposal_sha256 FROM managed_acceptance_delegations
                     WHERE delegation_id=?1",
                    params![delegation_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT proposal_sha256 FROM managed_acceptance_delegations
                         WHERE delegation_id=$1",
                        &[&delegation_id],
                    )
                    .map(|row| row.get(0))
                    .map_err(|error| error.to_string())
            }),
        }?;
        if manifest
            .get("proposal_manifest_sha256")
            .and_then(Value::as_str)
            != Some(approved_sha.as_str())
        {
            return Err("final manifest is not derived from the approved proposal".into());
        }
        Ok(())
    }

    /// Issue the distinct one-use spend receipt, atomically reserving the
    /// delegation's single execution. It consumes the exact separate manifest
    /// approval receipt and never accepts a caller-provided cap.
    pub fn issue_delegated_spend(
        &self,
        principal: &AuthenticatedPrincipal,
        delegation_id: &str,
        approval_receipt_sha256: &str,
        manifest: &Value,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_DELEGATED_MANIFEST_APPROVE)?;
        principal.require_scope(SCOPE_SPEND_AUTHORIZE)?;
        let manifest_sha256 = validate_delegated_manifest_policy(manifest)?;
        self.require_manifest_product_task_binding(delegation_id, principal.tenant_id(), manifest)?;
        let now = self.now();
        let spend_id = format!("mds-{}", Uuid::new_v4());
        let body = sort_value(&json!({
            "schema_version": "managed_delegated_spend_authorization.v1",
            "spend_authorization_id": spend_id,
            "delegation_id": delegation_id,
            "delegation_sha256": manifest.get("delegation_sha256"),
            "manifest_sha256": manifest_sha256,
            "max_cost_usd": manifest.pointer("/limits/max_cost_usd"),
            "one_use": true,
            "approval_receipt_sha256": approval_receipt_sha256,
            "issued_by": principal.principal_id(),
            "created_at": now,
            "expires_at": manifest.pointer("/delegation/expires_at"),
            "manifest": manifest
        }));
        let body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
        let body_json = body.to_string();
        let (issued_spend_id, issued_body_sha256, issued_status) = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let row: DelegatedSpendSqliteRow = tx
                    .query_row(
                        "SELECT delegation_sha256, status, COALESCE(spend_status,''), executions_used, expires_at, manifest_approver_id, manifest_approval_sha256, manifest_approval_json, spend_authorization_id, spend_body_sha256, spend_body_json, manifest_json FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?)),
                    )
                    .map_err(|e| e.to_string())?;
                if row.1 != "active"
                    || row.2 == "revoked"
                    || row.2 == "expired"
                    || is_at_or_before(&row.4, &now)?
                {
                    return Err("delegation is not active".into());
                }
                if manifest.get("delegation_sha256").and_then(Value::as_str) != Some(row.0.as_str()) {
                    return Err("manifest delegation hash does not match persisted delegation".into());
                }
                if row.5 != principal.principal_id()
                    || row.6.as_deref() != Some(approval_receipt_sha256)
                {
                    return Err("delegated spend requires the exact separated manifest approval".into());
                }
                validate_existing_manifest_approval(
                    approval_receipt_sha256,
                    row.7.as_deref().ok_or("delegated manifest approval receipt missing")?,
                    delegation_id,
                    manifest_sha256,
                    &row.5,
                )?;
                if row.3 >= 1 {
                    if row.2 != "active" {
                        return Err("delegation execution budget is already reserved".into());
                    }
                    let existing_spend_id = row
                        .8
                        .as_deref()
                        .ok_or("reserved delegation is missing spend authorization id")?;
                    let existing_body_sha256 = row
                        .9
                        .as_deref()
                        .ok_or("reserved delegation is missing spend body hash")?;
                    let existing_body: Value = serde_json::from_str(
                        row.10
                            .as_deref()
                            .ok_or("reserved delegation is missing spend body")?,
                    )
                    .map_err(|e| format!("persisted delegated spend body is invalid: {e}"))?;
                    let existing_manifest: Value = serde_json::from_str(
                        row.11
                            .as_deref()
                            .ok_or("reserved delegation is missing final manifest")?,
                    )
                    .map_err(|e| format!("persisted delegated final manifest is invalid: {e}"))?;
                    if existing_manifest != *manifest
                        || existing_body.get("manifest") != Some(manifest)
                        || existing_body
                            .get("approval_receipt_sha256")
                            .and_then(Value::as_str)
                            != Some(approval_receipt_sha256)
                        || existing_body
                            .get("manifest_sha256")
                            .and_then(Value::as_str)
                            != Some(manifest_sha256)
                        || existing_body
                            .get("spend_authorization_id")
                            .and_then(Value::as_str)
                            != Some(existing_spend_id)
                        || sha256_hex(canonical_json(&existing_body)?.as_bytes())
                            != existing_body_sha256
                    {
                        return Err(
                            "delegation execution budget is reserved for a conflicting request"
                                .into(),
                        );
                    }
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok((
                        existing_spend_id.to_string(),
                        existing_body_sha256.to_string(),
                        "active".to_string(),
                    ));
                }
                tx.execute(
                    "UPDATE managed_acceptance_delegations SET executions_used=1, spend_authorization_id=?1, spend_body_sha256=?2, spend_status='active', spend_body_json=?3, manifest_json=?4, updated_at=?5 WHERE delegation_id=?6 AND executions_used=0",
                    params![spend_id, body_sha256, body_json, manifest.to_string(), now, delegation_id],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok((spend_id.clone(), body_sha256.clone(), "active".to_string()))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let row = tx
                    .query_one(
                        "SELECT delegation_sha256, status, COALESCE(spend_status,''), executions_used, expires_at, manifest_approver_id, manifest_approval_sha256, manifest_approval_json, spend_authorization_id, spend_body_sha256, spend_body_json, manifest_json FROM managed_acceptance_delegations WHERE delegation_id=$1 FOR UPDATE",
                        &[&delegation_id],
                    )
                    .map_err(|e| e.to_string())?;
                let stored_sha: String = row.get(0);
                let status: String = row.get(1);
                let spend_status: String = row.get(2);
                let executions_used: i64 = row.get(3);
                let expires_at: String = row.get(4);
                let approver_id: String = row.get(5);
                let approval_sha: Option<String> = row.get(6);
                let approval_json: Option<String> = row.get(7);
                let existing_spend_id: Option<String> = row.get(8);
                let existing_body_sha256: Option<String> = row.get(9);
                let existing_body_json: Option<String> = row.get(10);
                let existing_manifest_json: Option<String> = row.get(11);
                if status != "active"
                    || spend_status == "revoked"
                    || spend_status == "expired"
                    || is_at_or_before(&expires_at, &now)?
                {
                    return Err("delegation is not active".into());
                }
                if manifest.get("delegation_sha256").and_then(Value::as_str) != Some(stored_sha.as_str()) {
                    return Err("manifest delegation hash does not match persisted delegation".into());
                }
                if approver_id != principal.principal_id()
                    || approval_sha.as_deref() != Some(approval_receipt_sha256)
                {
                    return Err("delegated spend requires the exact separated manifest approval".into());
                }
                validate_existing_manifest_approval(
                    approval_receipt_sha256,
                    approval_json
                        .as_deref()
                        .ok_or("delegated manifest approval receipt missing")?,
                    delegation_id,
                    manifest_sha256,
                    &approver_id,
                )?;
                if executions_used >= 1 {
                    if spend_status != "active" {
                        return Err("delegation execution budget is already reserved".into());
                    }
                    let existing_spend_id = existing_spend_id
                        .as_deref()
                        .ok_or("reserved delegation is missing spend authorization id")?;
                    let existing_body_sha256 = existing_body_sha256
                        .as_deref()
                        .ok_or("reserved delegation is missing spend body hash")?;
                    let existing_body: Value = serde_json::from_str(
                        existing_body_json
                            .as_deref()
                            .ok_or("reserved delegation is missing spend body")?,
                    )
                    .map_err(|e| format!("persisted delegated spend body is invalid: {e}"))?;
                    let existing_manifest: Value = serde_json::from_str(
                        existing_manifest_json
                            .as_deref()
                            .ok_or("reserved delegation is missing final manifest")?,
                    )
                    .map_err(|e| format!("persisted delegated final manifest is invalid: {e}"))?;
                    if existing_manifest != *manifest
                        || existing_body.get("manifest") != Some(manifest)
                        || existing_body
                            .get("approval_receipt_sha256")
                            .and_then(Value::as_str)
                            != Some(approval_receipt_sha256)
                        || existing_body
                            .get("manifest_sha256")
                            .and_then(Value::as_str)
                            != Some(manifest_sha256)
                        || existing_body
                            .get("spend_authorization_id")
                            .and_then(Value::as_str)
                            != Some(existing_spend_id)
                        || sha256_hex(canonical_json(&existing_body)?.as_bytes())
                            != existing_body_sha256
                    {
                        return Err(
                            "delegation execution budget is reserved for a conflicting request"
                                .into(),
                        );
                    }
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok((
                        existing_spend_id.to_string(),
                        existing_body_sha256.to_string(),
                        "active".to_string(),
                    ));
                }
                tx.execute(
                    "UPDATE managed_acceptance_delegations SET executions_used=1, spend_authorization_id=$1, spend_body_sha256=$2, spend_status='active', spend_body_json=$3, manifest_json=$4, updated_at=$5 WHERE delegation_id=$6 AND executions_used=0",
                    &[&spend_id, &body_sha256, &body_json, &manifest.to_string(), &now, &delegation_id],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok((spend_id.clone(), body_sha256.clone(), "active".to_string()))
            }),
        }?;
        Ok(json!({
            "schema_version": "managed_delegated_spend_authorization.v1",
            "spend_authorization_id": issued_spend_id,
            "delegation_id": delegation_id,
            "manifest_sha256": manifest_sha256,
            "spend_body_sha256": issued_body_sha256,
            "status": issued_status,
            "one_use": true
        }))
    }

    /// Consume the one-use delegated attempt lease. A duplicate exact request
    /// returns the same lease; a different attempt or manifest is rejected.
    pub fn admit_delegated_attempt(
        &self,
        principal: &AuthenticatedPrincipal,
        delegation_id: &str,
        attempt_id: &str,
        manifest: &Value,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_DELEGATED_EXECUTE)?;
        principal.require_scope(SCOPE_ATTEMPT_ADMIT)?;
        if matches!(principal.principal_kind(), PrincipalKind::FixturePrincipal) {
            return Err("fixture principal cannot activate a production delegated attempt".into());
        }
        let manifest_sha256 = manifest
            .get("manifest_sha256")
            .and_then(Value::as_str)
            .ok_or("final manifest hash is required")?;
        if manifest_sha256 != compute_attempt_manifest_sha256(manifest)? {
            return Err("final manifest hash mismatch".into());
        }
        self.require_manifest_product_task_binding(delegation_id, principal.tenant_id(), manifest)?;
        let manifest_json = manifest.to_string();
        let now = self.now();
        let lease_token = format!("lease-{}", Uuid::new_v4());
        let lease_id = crate::provider::managed_deepseek::managed_attempt_lease_id(&lease_token);
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let row: DelegatedAttemptSqliteRow = tx
                    .query_row(
                        "SELECT delegation_sha256, COALESCE(spend_status,''), attempt_id, attempt_lease_id, attempt_lease_token, expires_at, manifest_approver_id, status, manifest_json, attempt_activator_id FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?)),
                    )
                    .map_err(|e| e.to_string())?;
                if row.7 != "active"
                    || row.6 == principal.principal_id()
                    || is_at_or_before(&row.5, &now)?
                    || row.8.as_deref() != Some(manifest_json.as_str())
                    || manifest.get("delegation_sha256").and_then(Value::as_str)
                        != Some(row.0.as_str())
                {
                    return Err("delegated attempt authority is stale or mismatched".into());
                }
                if let Some(existing_attempt) = row.2 {
                    if existing_attempt == attempt_id
                        && row.9.as_deref() == Some(principal.principal_id())
                    {
                        return Ok(json!({
                            "schema_version": "managed_delegated_attempt_lease.v1",
                            "delegation_id": delegation_id,
                            "manifest_sha256": manifest_sha256,
                            "attempt_id": existing_attempt,
                            "attempt_lease_id": row.3,
                            "attempt_lease_token": row.4,
                            "status": "admitted",
                            "replayed": true
                        }));
                    }
                    return Err("delegated attempt already leased by another attempt".into());
                }
                if row.1 != "active" {
                    return Err("delegated spend is not active".into());
                }
                tx.execute(
                    "UPDATE managed_acceptance_delegations SET attempt_id=?1, attempt_lease_id=?2, attempt_lease_token=?3, attempt_status='admitted', spend_status='consumed', attempt_activator_id=?4, updated_at=?5 WHERE delegation_id=?6 AND attempt_id IS NULL AND spend_status='active'",
                    params![attempt_id, lease_id, lease_token, principal.principal_id(), now, delegation_id],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(json!({
                    "schema_version": "managed_delegated_attempt_lease.v1",
                    "delegation_id": delegation_id,
                    "manifest_sha256": manifest_sha256,
                    "attempt_id": attempt_id,
                    "attempt_lease_id": lease_id,
                    "attempt_lease_token": lease_token,
                    "status": "admitted",
                    "replayed": false
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let row = tx
                    .query_one(
                        "SELECT delegation_sha256, COALESCE(spend_status,''), attempt_id, attempt_lease_id, attempt_lease_token, expires_at, manifest_approver_id, status, manifest_json, attempt_activator_id FROM managed_acceptance_delegations WHERE delegation_id=$1 FOR UPDATE",
                        &[&delegation_id],
                    )
                    .map_err(|e| e.to_string())?;
                let stored_sha: String = row.get(0);
                let spend_status: String = row.get(1);
                let existing_attempt: Option<String> = row.get(2);
                let existing_lease_id: Option<String> = row.get(3);
                let existing_token: Option<String> = row.get(4);
                let expires_at: String = row.get(5);
                let approver_id: String = row.get(6);
                let status: String = row.get(7);
                let persisted_manifest: Option<String> = row.get(8);
                let existing_activator_id: Option<String> = row.get(9);
                if status != "active"
                    || approver_id == principal.principal_id()
                    || is_at_or_before(&expires_at, &now)?
                    || persisted_manifest.as_deref() != Some(manifest_json.as_str())
                    || manifest.get("delegation_sha256").and_then(Value::as_str)
                        != Some(stored_sha.as_str())
                {
                    return Err("delegated attempt authority is stale or mismatched".into());
                }
                if let Some(existing_attempt) = existing_attempt {
                    if existing_attempt == attempt_id
                        && existing_activator_id.as_deref() == Some(principal.principal_id())
                    {
                        tx.commit().map_err(|e| e.to_string())?;
                        return Ok(json!({"schema_version":"managed_delegated_attempt_lease.v1","delegation_id":delegation_id,"manifest_sha256":manifest_sha256,"attempt_id":existing_attempt,"attempt_lease_id":existing_lease_id,"attempt_lease_token":existing_token,"status":"admitted","replayed":true}));
                    }
                    return Err("delegated attempt already leased by another attempt".into());
                }
                if spend_status != "active" {
                    return Err("delegated spend is not active".into());
                }
                tx.execute(
                    "UPDATE managed_acceptance_delegations SET attempt_id=$1, attempt_lease_id=$2, attempt_lease_token=$3, attempt_status='admitted', spend_status='consumed', attempt_activator_id=$4, updated_at=$5 WHERE delegation_id=$6 AND attempt_id IS NULL AND spend_status='active'",
                    &[&attempt_id, &lease_id, &lease_token, &principal.principal_id(), &now, &delegation_id],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(json!({"schema_version":"managed_delegated_attempt_lease.v1","delegation_id":delegation_id,"manifest_sha256":manifest_sha256,"attempt_id":attempt_id,"attempt_lease_id":lease_id,"attempt_lease_token":lease_token,"status":"admitted","replayed":false}))
            }),
        }
    }

    /// Persist the separated artifact/output confirmer receipt. This authority
    /// can authorize one Draft-PR output only; it cannot execute a provider,
    /// admit spend, merge, release, or deploy.
    #[allow(clippy::too_many_arguments)]
    pub fn persist_delegated_artifact_confirmation(
        &self,
        principal: &AuthenticatedPrincipal,
        delegation_id: &str,
        manifest: &Value,
        artifact: &Value,
        verification: &Value,
        review: &Value,
        provider_execution: &Value,
        target_main_sha: &str,
        realized_cost_usd: f64,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_DELEGATED_ARTIFACT_CONFIRM)?;
        if matches!(principal.principal_kind(), PrincipalKind::FixturePrincipal) {
            return Err("fixture principal cannot confirm a production delegated artifact".into());
        }
        let _manifest_sha256 = validate_delegated_manifest_policy(manifest)?;
        self.require_manifest_product_task_binding(delegation_id, principal.tenant_id(), manifest)?;
        let manifest_json = manifest.to_string();
        let now = self.now();
        let confirm = |delegation_sha: &str,
                       body_json: &str,
                       status: &str,
                       expires_at: &str,
                       confirmer_id: &str,
                       activator_id: Option<&str>,
                       approver_id: &str,
                       persisted_manifest: Option<&str>,
                       attempt_status: Option<&str>|
         -> Result<Value, String> {
            if status != "active"
                || is_at_or_before(expires_at, &now)?
                || persisted_manifest != Some(manifest_json.as_str())
                || attempt_status != Some("admitted")
                || manifest.get("delegation_sha256").and_then(Value::as_str) != Some(delegation_sha)
                || activator_id.is_none()
                || activator_id == Some(principal.principal_id())
                || approver_id == principal.principal_id()
                || !confirmer_id.is_empty() && confirmer_id != principal.principal_id()
            {
                return Err("delegated artifact confirmer authority is stale or mismatched".into());
            }
            let delegation: DelegationContract = serde_json::from_str(body_json)
                .map_err(|_| "persisted delegation body is invalid")?;
            delegation.validate(&now)?;
            let mut confirmation = confirm_delegated_artifact_output(
                &delegation,
                manifest,
                artifact,
                verification,
                review,
                provider_execution,
                target_main_sha,
                realized_cost_usd,
            )?;
            let object = confirmation
                .as_object_mut()
                .ok_or("delegated artifact confirmation must be an object")?;
            object.insert("delegation_id".into(), json!(delegation_id));
            object.insert("delegation_sha256".into(), json!(delegation_sha));
            object.insert("confirmer_id".into(), json!(principal.principal_id()));
            object.insert("confirmed_at".into(), json!(now));
            object.insert("one_use".into(), json!(true));
            let confirmation_sha256 =
                sha256_hex(canonical_json(&sort_value(&confirmation))?.as_bytes());
            confirmation["artifact_confirmation_sha256"] = json!(confirmation_sha256);
            Ok(sort_value(&confirmation))
        };

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let row: DelegatedArtifactConfirmationSqliteRow = tx
                    .query_row(
                        "SELECT delegation_sha256, body_json, status, expires_at, artifact_confirmer_id, manifest_json, attempt_status, artifact_confirmation_sha256, artifact_confirmation_json, attempt_activator_id, manifest_approver_id FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?)),
                    )
                    .map_err(|e| e.to_string())?;
                let confirmation = confirm(
                    &row.0,
                    &row.1,
                    &row.2,
                    &row.3,
                    &row.4,
                    row.9.as_deref(),
                    &row.10,
                    row.5.as_deref(),
                    row.6.as_deref(),
                )?;
                let confirmation_sha256 = confirmation
                    .get("artifact_confirmation_sha256")
                    .and_then(Value::as_str)
                    .ok_or("delegated artifact confirmation hash missing")?;
                if let (Some(existing_sha), Some(existing_json)) = (row.7, row.8) {
                    let replay = replay_delegated_artifact_confirmation(
                        &existing_sha,
                        &existing_json,
                        &confirmation,
                    )?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(replay);
                }
                tx.execute(
                    "UPDATE managed_acceptance_delegations SET artifact_confirmation_sha256=?1, artifact_confirmation_json=?2, artifact_confirmer_id=?3, updated_at=?4 WHERE delegation_id=?5 AND artifact_confirmation_sha256 IS NULL",
                    params![confirmation_sha256, confirmation.to_string(), principal.principal_id(), now, delegation_id],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(confirmation)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let row = tx
                    .query_one(
                        "SELECT delegation_sha256, body_json, status, expires_at, artifact_confirmer_id, manifest_json, attempt_status, artifact_confirmation_sha256, artifact_confirmation_json, attempt_activator_id, manifest_approver_id FROM managed_acceptance_delegations WHERE delegation_id=$1 FOR UPDATE",
                        &[&delegation_id],
                    )
                    .map_err(|e| e.to_string())?;
                let delegation_sha: String = row.get(0);
                let body_json: String = row.get(1);
                let status: String = row.get(2);
                let expires_at: String = row.get(3);
                let confirmer_id: String = row.get(4);
                let persisted_manifest: Option<String> = row.get(5);
                let attempt_status: Option<String> = row.get(6);
                let activator_id: Option<String> = row.get(9);
                let approver_id: String = row.get(10);
                let confirmation = confirm(
                    &delegation_sha,
                    &body_json,
                    &status,
                    &expires_at,
                    &confirmer_id,
                    activator_id.as_deref(),
                    &approver_id,
                    persisted_manifest.as_deref(),
                    attempt_status.as_deref(),
                )?;
                let confirmation_sha256 = confirmation
                    .get("artifact_confirmation_sha256")
                    .and_then(Value::as_str)
                    .ok_or("delegated artifact confirmation hash missing")?;
                let existing_sha: Option<String> = row.get(7);
                let existing_json: Option<String> = row.get(8);
                if let (Some(existing_sha), Some(existing_json)) =
                    (existing_sha, existing_json)
                {
                    let replay = replay_delegated_artifact_confirmation(
                        &existing_sha,
                        &existing_json,
                        &confirmation,
                    )?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(replay);
                }
                tx.execute(
                    "UPDATE managed_acceptance_delegations SET artifact_confirmation_sha256=$1, artifact_confirmation_json=$2, artifact_confirmer_id=$3, updated_at=$4 WHERE delegation_id=$5 AND artifact_confirmation_sha256 IS NULL",
                    &[&confirmation_sha256, &confirmation.to_string(), &principal.principal_id(), &now, &delegation_id],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(confirmation)
            }),
        }
    }

    /// Persist terminal evidence and close the delegated spend/lease. Unknown
    /// outcomes remain terminal and never become retryable.
    pub fn complete_delegated_attempt(
        &self,
        delegation_id: &str,
        attempt_id: &str,
        lease_token: &str,
        status: &str,
        receipt: &Value,
        realized_cost_usd: f64,
    ) -> Result<Value, String> {
        validate_attempt_terminal_status(status)?;
        if !realized_cost_usd.is_finite() || realized_cost_usd < 0.0 {
            return Err("realized delegated cost is invalid".into());
        }
        let mut receipt = sort_value(receipt);
        let receipt_object = receipt
            .as_object_mut()
            .ok_or("delegated terminal receipt must be an object")?;
        if receipt_object.contains_key("terminal_class")
            || receipt_object.contains_key("spend_authorization_state")
            || receipt_object.contains_key("attempt_lease_state")
            || receipt_object.contains_key("delegation_state")
            || receipt_object.contains_key("realized_cost_usd")
        {
            return Err("delegated terminal receipt contains store-owned state fields".into());
        }
        receipt_object.insert("terminal_class".into(), json!(status));
        receipt_object.insert("spend_authorization_state".into(), json!("expired"));
        receipt_object.insert("attempt_lease_state".into(), json!("closed"));
        receipt_object.insert("delegation_state".into(), json!("expired"));
        receipt_object.insert("realized_cost_usd".into(), json!(realized_cost_usd));
        let receipt = sort_value(&receipt);
        let receipt_sha256 = sha256_hex(canonical_json(&receipt)?.as_bytes());
        let receipt_json = receipt.to_string();
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let row: (
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    String,
                    f64,
                    f64,
                ) = tx
                    .query_row(
                        "SELECT attempt_id, attempt_lease_token, terminal_receipt_json, status,
                                max_total_cost_usd, total_cost_usd
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation_id],
                        |r| {
                            Ok((
                                r.get(0)?,
                                r.get(1)?,
                                r.get(2)?,
                                r.get(3)?,
                                r.get(4)?,
                                r.get(5)?,
                            ))
                        },
                    )
                    .map_err(|e| e.to_string())?;
                if row.0.as_deref() != Some(attempt_id) || row.1.as_deref() != Some(lease_token) {
                    return Err("delegated attempt lease ownership mismatch".into());
                }
                if let Some(existing) = row.2 {
                    if existing == receipt_json
                        && row.3 == "expired"
                        && (row.5 - realized_cost_usd).abs() <= 1e-12
                    {
                        return Ok(json!({"status":"closed","terminal_class":status,"spend_authorization_state":"expired","attempt_lease_state":"closed","delegation_state":"expired","receipt_sha256":receipt_sha256,"replayed":true}));
                    }
                    return Err("late or conflicting delegated terminal write".into());
                }
                if realized_cost_usd > row.4 {
                    return Err("delegated cumulative cost ceiling exceeded".into());
                }
                tx.execute(
                    "UPDATE managed_acceptance_delegations SET status=CASE WHEN status='revoked' THEN 'revoked' ELSE 'expired' END, total_cost_usd=?1, spend_status=CASE WHEN status='revoked' THEN 'revoked' ELSE 'expired' END, attempt_status='closed', terminal_receipt_json=?2, terminal_at=?3, updated_at=?3 WHERE delegation_id=?4 AND attempt_id=?5 AND attempt_lease_token=?6",
                    params![realized_cost_usd, receipt_json, now, delegation_id, attempt_id, lease_token],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(json!({"status":"closed","terminal_class":status,"spend_authorization_state":"expired","attempt_lease_state":"closed","delegation_state":"expired","receipt_sha256":receipt_sha256,"replayed":false}))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let row = tx.query_one("SELECT attempt_id, attempt_lease_token, terminal_receipt_json, status, max_total_cost_usd, total_cost_usd FROM managed_acceptance_delegations WHERE delegation_id=$1 FOR UPDATE", &[&delegation_id]).map_err(|e| e.to_string())?;
                let stored_attempt: Option<String> = row.get(0);
                let stored_token: Option<String> = row.get(1);
                let existing: Option<String> = row.get(2);
                let stored_status: String = row.get(3);
                let max_cost: f64 = row.get(4);
                let stored_cost: f64 = row.get(5);
                if stored_attempt.as_deref() != Some(attempt_id) || stored_token.as_deref() != Some(lease_token) {
                    return Err("delegated attempt lease ownership mismatch".into());
                }
                if let Some(existing) = existing {
                    if existing == receipt_json
                        && stored_status == "expired"
                        && (stored_cost - realized_cost_usd).abs() <= 1e-12
                    {
                        tx.commit().map_err(|e| e.to_string())?;
                        return Ok(json!({"status":"closed","terminal_class":status,"spend_authorization_state":"expired","attempt_lease_state":"closed","delegation_state":"expired","receipt_sha256":receipt_sha256,"replayed":true}));
                    }
                    return Err("late or conflicting delegated terminal write".into());
                }
                if realized_cost_usd > max_cost { return Err("delegated cumulative cost ceiling exceeded".into()); }
                tx.execute("UPDATE managed_acceptance_delegations SET status=CASE WHEN status='revoked' THEN 'revoked' ELSE 'expired' END, total_cost_usd=$1, spend_status=CASE WHEN status='revoked' THEN 'revoked' ELSE 'expired' END, attempt_status='closed', terminal_receipt_json=$2, terminal_at=$3, updated_at=$3 WHERE delegation_id=$4 AND attempt_id=$5 AND attempt_lease_token=$6", &[&realized_cost_usd, &receipt_json, &now, &delegation_id, &attempt_id, &lease_token]).map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(json!({"status":"closed","terminal_class":status,"spend_authorization_state":"expired","attempt_lease_state":"closed","delegation_state":"expired","receipt_sha256":receipt_sha256,"replayed":false}))
            }),
        }
    }

    /// Close a successful delegated ProductTask using only store-owned
    /// evidence and the current persisted lease. The caller cannot supply a
    /// cost, lease token, output receipt, or terminal class.
    pub fn complete_delegated_product_task_terminal(
        &self,
        delegation_id: &str,
        attempt_id: &str,
        product_task_id: &str,
        actor: &str,
    ) -> Result<Value, String> {
        let row: (String, String, String, String) = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT attempt_lease_token, artifact_confirmation_json, tenant_id, manifest_json
                     FROM managed_acceptance_delegations
                     WHERE delegation_id=?1 AND attempt_id=?2",
                    params![delegation_id, attempt_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT attempt_lease_token, artifact_confirmation_json, tenant_id, manifest_json
                         FROM managed_acceptance_delegations
                         WHERE delegation_id=$1 AND attempt_id=$2",
                        &[&delegation_id, &attempt_id],
                    )
                    .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)))
                    .map_err(|error| error.to_string())
            }),
        }?;
        let (lease_token, confirmation_json, tenant_id, manifest_json) = row;
        let confirmation: Value = serde_json::from_str(&confirmation_json)
            .map_err(|_| "persisted delegated artifact confirmation is invalid")?;
        let manifest: Value = serde_json::from_str(&manifest_json)
            .map_err(|_| "persisted delegated manifest is invalid")?;
        validate_delegated_manifest_policy(&manifest)?;
        self.require_delegation_product_task(delegation_id, &tenant_id, product_task_id)?;
        let task = self
            .get_product_task(product_task_id)?
            .ok_or("delegated terminal ProductTask is missing")?;
        if task.get("status").and_then(Value::as_str) != Some("completed")
            || task.get("tenant_id").and_then(Value::as_str) != Some(tenant_id.as_str())
            || task
                .pointer("/workspace_binding/source_revision")
                .and_then(Value::as_str)
                != confirmation.get("target_main_sha").and_then(Value::as_str)
            || manifest
                .pointer("/execution/product_task_id")
                .and_then(Value::as_str)
                != Some(product_task_id)
            || manifest
                .pointer("/execution/attempt_id")
                .and_then(Value::as_str)
                != Some(attempt_id)
        {
            return Err("delegated terminal ProductTask binding is stale or mismatched".into());
        }
        let terminal_evidence = self.get_product_task_terminal_evidence(product_task_id)?;
        let evidence_id = terminal_evidence
            .get("evidence_id")
            .and_then(Value::as_str)
            .ok_or("delegated ProductTask terminal evidence identity is missing")?;
        let run_id = terminal_evidence
            .get("run_id")
            .and_then(Value::as_str)
            .ok_or("delegated ProductTask terminal run identity is missing")?;
        let approval_id = terminal_evidence
            .pointer("/approval/approval_id")
            .and_then(Value::as_str)
            .ok_or("delegated ProductTask terminal approval is missing")?;
        let approval = self
            .workflow_run_approvals(run_id, 10_000)?
            .into_iter()
            .find(|approval| {
                approval.get("approval_id").and_then(Value::as_str) == Some(approval_id)
            })
            .ok_or("delegated ProductTask terminal approval disappeared")?;
        let confirmation_sha256 = confirmation
            .get("artifact_confirmation_sha256")
            .and_then(Value::as_str)
            .ok_or("delegated artifact confirmation hash is missing")?;
        let output_intent = terminal_evidence
            .pointer("/output/intent")
            .and_then(Value::as_str)
            .ok_or("delegated terminal output intent is missing")?;
        // Output modes are terminal-specific and mutually exclusive: a
        // completed Draft PR for draft_pr output, or the durable artifact-only
        // receipt re-read from the persisted artifact owner with no network
        // effect. Mixed or unknown modes fail closed.
        let (draft_pr, artifact_receipt) = match output_intent {
            "draft_pr" => {
                if terminal_evidence.pointer("/output/receipt_id").is_some() {
                    return Err("delegated terminal output evidence mixes output modes".into());
                }
                let draft_pr = terminal_evidence
                    .pointer("/output/draft_pr")
                    .filter(|value| value.is_object())
                    .ok_or("delegated terminal evidence requires a completed Draft PR")?;
                (Some(draft_pr), None)
            }
            "artifact_only" => {
                // Null placeholders are part of the committed evidence shape;
                // only materialized network-effect values are forbidden.
                let carries_network_effect = |pointer: &str| -> bool {
                    terminal_evidence
                        .pointer(pointer)
                        .is_some_and(|value| !value.is_null())
                };
                if carries_network_effect("/output/draft_pr")
                    || carries_network_effect("/output/branch")
                    || carries_network_effect("/output/pushed_commit")
                {
                    return Err(
                        "artifact-only delegated terminal must not carry network output effects"
                            .into(),
                    );
                }
                let receipt_id = terminal_evidence
                    .pointer("/output/receipt_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or(
                        "delegated terminal requires the durable artifact-only receipt identity",
                    )?;
                let result_sha256 = terminal_evidence
                    .pointer("/output/result_sha256")
                    .and_then(Value::as_str)
                    .filter(|value| {
                        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
                    .ok_or("delegated terminal artifact-only receipt hash is missing")?;
                (None, Some((receipt_id, result_sha256)))
            }
            other => {
                return Err(format!(
                    "delegated terminal output intent is unsupported: {other}"
                ))
            }
        };
        if approval
            .get("artifact_confirmation_sha256")
            .and_then(Value::as_str)
            != Some(confirmation_sha256)
            || confirmation
                .pointer("/provider_execution/provider_request_count")
                .and_then(Value::as_u64)
                != Some(3)
            || confirmation
                .pointer("/output/draft_pr_only")
                .and_then(Value::as_bool)
                != Some(true)
            || draft_pr.is_some_and(|draft_pr| {
                draft_pr.get("draft").and_then(Value::as_bool) != Some(true)
                    || draft_pr
                        .get("head_branch")
                        .and_then(Value::as_str)
                        .is_none_or(|branch| !branch.starts_with("acp/"))
            })
        {
            return Err("delegated terminal output evidence is stale or outside policy".into());
        }
        let persisted_artifact_receipt = if let Some((receipt_id, result_sha256)) = artifact_receipt
        {
            let source_revision = task
                .pointer("/workspace_binding/source_revision")
                .and_then(Value::as_str)
                .ok_or("delegated terminal source revision is missing")?;
            let workspace_record_id = task
                .get("workspace_record_id")
                .and_then(Value::as_str)
                .ok_or("delegated terminal workspace identity is missing")?;
            let artifact = self.current_product_task_artifact(
                product_task_id,
                run_id,
                workspace_record_id,
                source_revision,
            )?;
            let receipt = artifact
                .get("product_output_receipt")
                .filter(|value| value.is_object())
                .ok_or("durable artifact-only output receipt is missing")?;
            if receipt.get("schema_version").and_then(Value::as_str)
                != Some("product_output_receipt.v1")
                || receipt.get("receipt_id").and_then(Value::as_str) != Some(receipt_id)
                || receipt.get("state").and_then(Value::as_str) != Some("completed")
                || receipt.get("output_intent").and_then(Value::as_str) != Some("artifact_only")
                || receipt.get("output_sha256").and_then(Value::as_str) != Some(result_sha256)
                || receipt.get("approval_id").and_then(Value::as_str) != Some(approval_id)
                || receipt.pointer("/output/mode").and_then(Value::as_str) != Some("artifact_only")
                || receipt
                    .pointer("/output/target_mutation")
                    .and_then(Value::as_bool)
                    != Some(false)
            {
                return Err(
                    "durable artifact-only output receipt does not match the terminal evidence"
                        .into(),
                );
            }
            Some(json!({
                "receipt_id": receipt_id,
                "output_result_sha256": result_sha256,
                "target_mutation": false,
            }))
        } else {
            None
        };
        let workspace_id = task
            .get("workspace_record_id")
            .and_then(Value::as_str)
            .ok_or("delegated terminal workspace identity is missing")?;
        let cleanup = self.cleanup_workspace(workspace_id, actor)?;
        if cleanup.get("status").and_then(Value::as_str) != Some("cleaned") {
            return Err("delegated terminal workspace cleanup is incomplete".into());
        }
        let realized_cost_usd = confirmation
            .get("realized_cost_usd")
            .and_then(Value::as_f64)
            .ok_or("delegated artifact confirmation realized cost is missing")?;
        let mut receipt = json!({
            "schema_version": "managed_delegated_terminal_evidence.v1",
            "product_task_id": product_task_id,
            "workflow_run_id": run_id,
            "manifest_sha256": manifest.get("manifest_sha256"),
            "artifact_confirmation_sha256": confirmation_sha256,
            "product_terminal_evidence_id": evidence_id,
            "provider_requests": 3,
            "output_intent": output_intent,
            "cleanup_status": cleanup.get("status"),
            "target_main_sha": confirmation.get("target_main_sha"),
        });
        match (draft_pr, artifact_receipt, persisted_artifact_receipt) {
            (Some(draft_pr), None, None) => {
                receipt["draft_pr"] = draft_pr.clone();
            }
            (None, Some((receipt_id, result_sha256)), Some(persisted)) => {
                receipt["artifact_output_receipt"] = persisted;
                receipt["artifact_output_receipt_id"] = json!(receipt_id);
                receipt["output_result_sha256"] = json!(result_sha256);
            }
            _ => return Err("delegated terminal output mode is ambiguous".into()),
        }
        let terminal = self.complete_delegated_attempt(
            delegation_id,
            attempt_id,
            &lease_token,
            "succeeded",
            &receipt,
            realized_cost_usd,
        )?;
        Ok(json!({
            "terminal": terminal,
            "cleanup": cleanup,
            "product_terminal_evidence": terminal_evidence,
            "artifact_confirmation": confirmation,
        }))
    }

    /// HTTP/store boundary for delegated terminalization. The authenticated
    /// output principal supplies the tenant and required terminal scopes; the
    /// request body supplies only immutable identities that are rechecked by
    /// the existing terminal owner.
    pub fn complete_delegated_product_task_terminal_for_principal(
        &self,
        principal: &AuthenticatedPrincipal,
        delegation_id: &str,
        attempt_id: &str,
        product_task_id: &str,
        actor: &str,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_ATTEMPT_ADMIT)?;
        principal.require_scope(SCOPE_DELEGATED_EXECUTE)?;
        let task = self
            .get_product_task(product_task_id)?
            .ok_or("delegated terminal ProductTask is missing")?;
        if task.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
            return Err("delegated terminal ProductTask tenant does not match principal".into());
        }
        self.require_delegation_product_task(
            delegation_id,
            principal.tenant_id(),
            product_task_id,
        )?;
        self.complete_delegated_product_task_terminal(
            delegation_id,
            attempt_id,
            product_task_id,
            actor,
        )
    }

    /// Close a delegated non-success ProductTask from store-owned task,
    /// provider-journal, workspace, and lease evidence. This is called by the
    /// existing ProductTask finalizer/recovery path and cannot be used to turn
    /// a failed task into a successful output.
    pub(crate) fn close_delegated_failure_if_bound(
        &self,
        product_task_id: &str,
        actor: &str,
    ) -> Result<Option<Value>, String> {
        let task = self
            .get_product_task(product_task_id)?
            .ok_or("delegated failure ProductTask is missing")?;
        let task_status = task
            .get("status")
            .and_then(Value::as_str)
            .ok_or("delegated failure ProductTask status is missing")?;
        if !matches!(
            task_status,
            "failed" | "killed" | "blocked" | "budget_exhausted" | "outcome_unknown"
        ) {
            return Ok(None);
        }
        let rows: Vec<(String, String, String, String, String)> = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                let mut statement = connection
                    .prepare(
                        "SELECT delegation_id, attempt_id, attempt_lease_token, manifest_json,
                                provider_request_journal_json
                         FROM managed_acceptance_delegations
                         WHERE status IN ('active','revoked') AND attempt_status='admitted'",
                    )
                    .map_err(|error| error.to_string())?;
                let rows = statement
                    .query_map([], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    })
                    .map_err(|error| error.to_string())?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?;
                Ok(rows)
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query(
                        "SELECT delegation_id, attempt_id, attempt_lease_token, manifest_json,
                                provider_request_journal_json
                         FROM managed_acceptance_delegations
                         WHERE status IN ('active','revoked') AND attempt_status='admitted'",
                        &[],
                    )
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4)))
                            .collect()
                    })
                    .map_err(|error| error.to_string())
            })?,
        };
        let mut matched = rows
            .into_iter()
            .filter_map(|row| {
                let manifest = serde_json::from_str::<Value>(&row.3).ok()?;
                (manifest
                    .pointer("/execution/product_task_id")
                    .and_then(Value::as_str)
                    == Some(product_task_id))
                .then_some((row, manifest))
            })
            .collect::<Vec<_>>();
        if matched.is_empty() {
            return Ok(None);
        }
        if matched.len() != 1 {
            return Err("multiple delegated attempts bind the same failed ProductTask".into());
        }
        let ((delegation_id, attempt_id, lease_token, _manifest_json, journal_json), manifest) =
            matched.pop().expect("one matched delegated attempt");
        validate_delegated_manifest_policy(&manifest)?;
        let task_tenant_id = task
            .get("tenant_id")
            .and_then(Value::as_str)
            .ok_or("delegated failure ProductTask tenant is missing")?;
        self.require_delegation_product_task(&delegation_id, task_tenant_id, product_task_id)?;
        if manifest
            .pointer("/execution/attempt_id")
            .and_then(Value::as_str)
            != Some(attempt_id.as_str())
        {
            return Err("delegated failure manifest attempt binding is stale".into());
        }
        let journal: Vec<Value> = serde_json::from_str(&journal_json)
            .map_err(|_| "delegated failure provider journal is invalid")?;
        let mut realized_or_reserved_cost_usd = 0.0_f64;
        let mut uncertain_provider_effect = false;
        let mut provider_states = Vec::with_capacity(journal.len());
        for entry in &journal {
            let status = entry
                .get("status")
                .and_then(Value::as_str)
                .ok_or("delegated failure provider journal status is missing")?;
            if !matches!(
                status,
                "sending"
                    | "succeeded"
                    | "failed_before_send"
                    | "failed_known_outcome"
                    | "outcome_unknown"
            ) {
                return Err("delegated failure provider journal status is invalid".into());
            }
            uncertain_provider_effect |= matches!(status, "sending" | "outcome_unknown");
            let cost = entry
                .get("effective_cost_usd")
                .and_then(Value::as_f64)
                .ok_or("delegated failure provider journal cost is missing")?;
            if !cost.is_finite() || cost < 0.0 {
                return Err("delegated failure provider journal cost is invalid".into());
            }
            realized_or_reserved_cost_usd += cost;
            provider_states.push(json!({
                "ordinal": entry.get("ordinal"),
                "node_id": entry.get("node_id"),
                "status": status,
                "request_sha256": entry.get("request_sha256"),
                "effective_cost_usd": cost,
            }));
        }
        let terminal_class = if task_status == "outcome_unknown" || uncertain_provider_effect {
            "outcome_unknown"
        } else if task_status == "killed"
            || task
                .get("error_detail")
                .and_then(Value::as_str)
                .is_some_and(|detail| detail == "cancelled")
        {
            "cancelled"
        } else {
            "failed"
        };
        let workspace_id = task
            .get("workspace_record_id")
            .and_then(Value::as_str)
            .ok_or("delegated failure workspace identity is missing")?;
        let cleanup = self.cleanup_workspace(workspace_id, actor)?;
        if cleanup.get("status").and_then(Value::as_str) != Some("cleaned") {
            return Err("delegated failure workspace cleanup is incomplete".into());
        }
        let provider_journal_sha256 =
            sha256_hex(canonical_json(&sort_value(&Value::Array(journal)))?.as_bytes());
        let receipt = json!({
            "schema_version": "managed_delegated_failure_terminal_evidence.v1",
            "product_task_id": product_task_id,
            "workflow_run_id": task.get("run_id"),
            "manifest_sha256": manifest.get("manifest_sha256"),
            "provider_request_count": provider_states.len(),
            "provider_request_journal_sha256": provider_journal_sha256,
            "provider_request_states": provider_states,
            "cleanup_status": cleanup.get("status"),
            "product_task_status": task_status,
            "target_main_sha": manifest.pointer("/target/main_sha"),
            "cost_evidence": if uncertain_provider_effect {
                "conservative_reservation"
            } else {
                "reconciled"
            },
        });
        let terminal = self.complete_delegated_attempt(
            &delegation_id,
            &attempt_id,
            &lease_token,
            terminal_class,
            &receipt,
            realized_or_reserved_cost_usd,
        )?;
        Ok(Some(json!({
            "terminal": terminal,
            "cleanup": cleanup,
            "failure_terminal_evidence": receipt,
        })))
    }

    fn close_revoked_delegation_attempt(
        &self,
        delegation_id: &str,
        actor: &str,
    ) -> Result<Option<Value>, String> {
        let row: Option<(String, String, String)> = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                connection
                    .query_row(
                        "SELECT attempt_id, manifest_json, provider_request_journal_json
                         FROM managed_acceptance_delegations
                         WHERE delegation_id=?1 AND status='revoked'
                           AND attempt_status='admitted'",
                        params![delegation_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()
                    .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT attempt_id, manifest_json, provider_request_journal_json
                         FROM managed_acceptance_delegations
                         WHERE delegation_id=$1 AND status='revoked'
                           AND attempt_status='admitted'",
                        &[&delegation_id],
                    )
                    .map(|row| row.map(|row| (row.get(0), row.get(1), row.get(2))))
                    .map_err(|error| error.to_string())
            })?,
        };
        let Some((attempt_id, manifest_json, journal_json)) = row else {
            return Ok(None);
        };
        let manifest: Value = serde_json::from_str(&manifest_json)
            .map_err(|_| "revoked delegation final manifest is invalid")?;
        let product_task_id = manifest
            .pointer("/execution/product_task_id")
            .and_then(Value::as_str)
            .ok_or("revoked delegation ProductTask binding is missing")?;
        let task = self
            .get_product_task(product_task_id)?
            .ok_or("revoked delegation ProductTask is missing")?;
        let workspace_id = task
            .get("workspace_record_id")
            .and_then(Value::as_str)
            .ok_or("revoked delegation workspace identity is missing")?;
        let cleanup = self.cleanup_workspace(workspace_id, actor)?;
        if cleanup.get("status").and_then(Value::as_str) != Some("cleaned") {
            return Err("revoked delegation workspace cleanup is incomplete".into());
        }
        let journal: Vec<Value> = serde_json::from_str(&journal_json)
            .map_err(|_| "revoked delegation provider journal is invalid")?;
        let receipt = sort_value(&json!({
            "schema_version": "managed_delegated_revocation_terminal_evidence.v1",
            "delegation_id": delegation_id,
            "attempt_id": attempt_id,
            "product_task_id": product_task_id,
            "manifest_sha256": manifest.get("manifest_sha256"),
            "provider_request_journal_sha256":
                sha256_hex(canonical_json(&sort_value(&Value::Array(journal)))?.as_bytes()),
            "cleanup": cleanup,
            "rollback_evidence": {
                "workspace_status": "cleaned",
                "target_main_write": false
            },
            "terminal_class": "revoked"
        }));
        let now = self.now();
        let receipt_json = receipt.to_string();
        let changed = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                connection
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET attempt_status='closed', spend_status='revoked',
                             terminal_receipt_json=?1, terminal_at=?2, updated_at=?2
                         WHERE delegation_id=?3 AND status='revoked'
                           AND attempt_id=?4 AND attempt_status='admitted'",
                        params![receipt_json, now, delegation_id, attempt_id],
                    )
                    .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET attempt_status='closed', spend_status='revoked',
                             terminal_receipt_json=$1, terminal_at=$2, updated_at=$2
                         WHERE delegation_id=$3 AND status='revoked'
                           AND attempt_id=$4 AND attempt_status='admitted'",
                        &[&receipt_json, &now, &delegation_id, &attempt_id],
                    )
                    .map(|changed| changed as usize)
                    .map_err(|error| error.to_string())
            })?,
        };
        if changed != 1 {
            return Err("revoked delegation terminal closure lost its lease state".into());
        }
        Ok(Some(json!({"terminal": receipt, "cleanup": cleanup})))
    }

    pub fn revoke_delegation(
        &self,
        principal: &AuthenticatedPrincipal,
        delegation_id: &str,
    ) -> Result<(), String> {
        principal.require_scope(SCOPE_REVOKE)?;
        principal.require_scope(SCOPE_DELEGATED_AUTONOMY)?;
        if matches!(principal.principal_kind(), PrincipalKind::FixturePrincipal) {
            return Err("fixture principal cannot revoke production delegation".into());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let row: Option<(String, String, String)> = tx
                    .query_row(
                        "SELECT tenant_id, principal_id, status
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                let (tenant_id, principal_id, status) =
                    row.ok_or("delegation to revoke does not exist")?;
                if tenant_id != principal.tenant_id() || principal_id != principal.principal_id() {
                    return Err("delegation revocation ownership mismatch".into());
                }
                if status == "revoked" {
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(());
                }
                if status != "active" {
                    return Err("delegation is not revocable in its current state".into());
                }
                let changed = tx.execute("UPDATE managed_acceptance_delegations SET status='revoked', spend_status='revoked', revoked_at=?1, updated_at=?1 WHERE delegation_id=?2 AND tenant_id=?3 AND principal_id=?4 AND status='active'", params![now, delegation_id, principal.tenant_id(), principal.principal_id()]).map_err(|e| e.to_string())?;
                if changed != 1 {
                    return Err("delegation revocation lost its current authority".into());
                }
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let row = tx
                    .query_opt(
                        "SELECT tenant_id, principal_id, status
                         FROM managed_acceptance_delegations WHERE delegation_id=$1 FOR UPDATE",
                        &[&delegation_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or("delegation to revoke does not exist")?;
                let tenant_id: String = row.get(0);
                let principal_id: String = row.get(1);
                let status: String = row.get(2);
                if tenant_id != principal.tenant_id() || principal_id != principal.principal_id() {
                    return Err("delegation revocation ownership mismatch".into());
                }
                if status == "revoked" {
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(());
                }
                if status != "active" {
                    return Err("delegation is not revocable in its current state".into());
                }
                let changed = tx.execute("UPDATE managed_acceptance_delegations SET status='revoked', spend_status='revoked', revoked_at=$1, updated_at=$1 WHERE delegation_id=$2 AND tenant_id=$3 AND principal_id=$4 AND status='active'", &[&now, &delegation_id, &principal.tenant_id(), &principal.principal_id()]).map_err(|e| e.to_string())?;
                if changed != 1 {
                    return Err("delegation revocation lost its current authority".into());
                }
                tx.commit().map_err(|error| error.to_string())
            }),
        }?;
        self.close_revoked_delegation_attempt(delegation_id, principal.principal_id())?;
        Ok(())
    }

    /// Rebind an active delegation that never reached spend or attempt
    /// admission after a restart/preparation interruption. The delegation
    /// id, body, and creation identity remain immutable; only the canonical
    /// reissued reviewer principal and ProductTask binding may be filled in.
    /// No admitted, spent, confirmed, outcome-unknown, revoked, or terminal
    /// operation can enter this recovery path.
    pub fn rebind_unadmitted_delegation_for_bootstrap(
        &self,
        bootstrap: &AuthenticatedPrincipal,
        task_id: &str,
        delegation_id: &str,
        reviewer: &AuthenticatedPrincipal,
    ) -> Result<Value, String> {
        if bootstrap.principal_kind() != &PrincipalKind::OperatorApiKey
            || bootstrap.principal_id() != LOCAL_BOOTSTRAP_API_KEY_ID
            || !bootstrap.has_scope(SCOPE_IDENTITY_DELEGATE)
        {
            return Err("canonical bootstrap identity-delegation authority is required".into());
        }
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0);
        let canonical_bootstrap = self.authenticate_bootstrap_identity_delegation_principal(
            bootstrap.tenant_id(),
            Some(now_unix),
        )?;
        if canonical_bootstrap.principal_id() != bootstrap.principal_id()
            || canonical_bootstrap.scopes() != bootstrap.scopes()
        {
            return Err("bootstrap authority context is stale or mismatched".into());
        }
        let canonical_reviewer = self.authenticate_managed_acceptance_principal_for_tenant(
            reviewer.tenant_id(),
            reviewer.principal_id(),
            Some(now_unix),
        )?;
        if canonical_reviewer.principal_kind() != &PrincipalKind::OperatorApiKey
            || canonical_reviewer.tenant_id() != canonical_bootstrap.tenant_id()
            || is_forbidden_principal_id(canonical_reviewer.principal_id())
            || is_forbidden_role(canonical_reviewer.role())
        {
            return Err("reissued reviewer principal is not an authenticated operator".into());
        }
        if canonical_reviewer.role() != "reviewer" {
            return Err("reissued identity must use the reviewer profile".into());
        }
        validate_managed_acceptance_role_scopes(
            canonical_reviewer.role(),
            canonical_reviewer.scopes(),
        )?;
        canonical_reviewer.require_scope(SCOPE_DELEGATED_AUTONOMY)?;
        canonical_reviewer.require_scope(SCOPE_DELEGATED_MANIFEST_APPROVE)?;
        canonical_reviewer.require_scope(SCOPE_SPEND_AUTHORIZE)?;
        let bootstrap_scopes_json = serde_json::to_string(canonical_bootstrap.scopes())
            .map_err(|error| error.to_string())?;
        let reviewer_scopes_json = serde_json::to_string(canonical_reviewer.scopes())
            .map_err(|error| error.to_string())?;
        let tenant_id = canonical_bootstrap.tenant_id();
        let reviewer_key_id = canonical_reviewer.principal_id();
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        if task.get("tenant_id").and_then(Value::as_str) != Some(tenant_id) {
            return Err("product task tenant does not match bootstrap authority".into());
        }
        let task_version = task
            .get("version")
            .and_then(Value::as_i64)
            .ok_or("product task version is missing")?;
        if reviewer_key_id.trim().is_empty() {
            return Err("reissued reviewer principal is required".into());
        }
        let now = self.now();
        let details = json!({
            "authority_owner": "managed_acceptance_bootstrap",
            "pre_admission_only": true,
            "task_id": task_id,
            "task_version": task_version,
            "reissued_reviewer_principal": reviewer_key_id,
        });
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let current_task_version: i64 = tx
                    .query_row(
                        "SELECT version FROM product_tasks WHERE task_id=?1",
                        params![task_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                if current_task_version != task_version {
                    return Err("ProductTask version moved during bootstrap recovery".into());
                }
                let row: (String, String, Option<String>, String, String, String) = tx
                    .query_row(
                        "SELECT tenant_id, status, product_task_id, principal_id,
                                manifest_approver_id, expires_at
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation_id],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                            ))
                        },
                    )
                    .map_err(|error| error.to_string())?;
                if row.0 != tenant_id {
                    return Err("delegation tenant does not match ProductTask tenant".into());
                }
                if row.1 != "active" {
                    return Err(
                        "bootstrap delegation recovery only permits active pre-admission state"
                            .into(),
                    );
                }
                if is_at_or_before(&row.5, &now)? {
                    return Err("bootstrap delegation recovery refuses an expired delegation".into());
                }
                if row.2.as_deref() != Some(task_id) {
                    return Err(
                        "bootstrap recovery requires an existing immutable ProductTask binding"
                            .into(),
                    );
                }
                let safe_state: i64 = tx
                    .query_row(
                        "SELECT EXISTS(
                            SELECT 1 FROM managed_acceptance_delegations
                             WHERE delegation_id=?1 AND tenant_id=?2 AND status='active'
                               AND product_task_id=?3
                               AND executions_used=0 AND total_cost_usd=0
                               AND proposal_sha256 IS NULL AND proposal_json IS NULL
                               AND spend_authorization_id IS NULL
                               AND manifest_approval_sha256 IS NULL AND manifest_approval_json IS NULL
                               AND spend_body_sha256 IS NULL AND spend_status IS NULL
                               AND spend_body_json IS NULL AND manifest_json IS NULL
                               AND attempt_id IS NULL AND attempt_lease_id IS NULL
                               AND attempt_lease_token IS NULL AND attempt_status IS NULL
                               AND attempt_activator_id IS NULL
                               AND artifact_confirmation_sha256 IS NULL
                               AND artifact_confirmation_json IS NULL
                               AND provider_request_journal_json='[]'
                               AND terminal_receipt_json IS NULL AND terminal_at IS NULL
                               AND revoked_at IS NULL AND artifact_confirmer_id=''
                         )",
                        params![delegation_id, tenant_id, task_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                if safe_state != 1 {
                    return Err(
                        "bootstrap delegation recovery requires an untouched pre-admission operation"
                            .into(),
                    );
                }
                if row.4 != reviewer_key_id && row.4 != row.3 {
                    return Err(
                        "bootstrap recovery cannot replace an already rebound reviewer".into(),
                    );
                }
                let replayed = row.4 == reviewer_key_id;
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET manifest_approver_id=?1,
                             updated_at=CASE WHEN manifest_approver_id=?1 THEN updated_at ELSE ?2 END
                         WHERE delegation_id=?3 AND tenant_id=?4 AND status='active'
                           AND product_task_id=?5
                           AND executions_used=0 AND total_cost_usd=0
                           AND proposal_sha256 IS NULL AND proposal_json IS NULL
                           AND spend_authorization_id IS NULL
                           AND manifest_approval_sha256 IS NULL AND manifest_approval_json IS NULL
                           AND spend_body_sha256 IS NULL AND spend_status IS NULL
                           AND spend_body_json IS NULL AND manifest_json IS NULL
                           AND attempt_id IS NULL AND attempt_lease_id IS NULL
                           AND attempt_lease_token IS NULL AND attempt_status IS NULL
                           AND attempt_activator_id IS NULL
                           AND artifact_confirmation_sha256 IS NULL
                           AND artifact_confirmation_json IS NULL
                           AND provider_request_journal_json='[]'
                           AND terminal_receipt_json IS NULL AND terminal_at IS NULL
                           AND revoked_at IS NULL AND artifact_confirmer_id=''
                           AND (manifest_approver_id=?1 OR manifest_approver_id=principal_id)
                           AND EXISTS (
                               SELECT 1 FROM api_key_metadata
                               WHERE key_id=?6 AND tenant_id=?4 AND role='reviewer'
                                 AND scopes_json=?7 AND revoked_at IS NULL
                                 AND (expires_at IS NULL OR CAST(expires_at AS REAL) >= ?9)
                           )
                           AND EXISTS (
                               SELECT 1 FROM api_key_metadata
                               WHERE key_id=?8 AND tenant_id=?4 AND role IN ('admin','bootstrap')
                                 AND scopes_json=?10 AND revoked_at IS NULL
                                 AND (expires_at IS NULL OR CAST(expires_at AS REAL) >= ?9)
                           )",
                        params![
                            reviewer_key_id,
                            now,
                            delegation_id,
                            tenant_id,
                            task_id,
                            reviewer_key_id,
                            reviewer_scopes_json,
                            LOCAL_BOOTSTRAP_API_KEY_ID,
                            now_unix,
                            bootstrap_scopes_json,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("delegation recovery lost its current authenticated pre-admission state".into());
                }
                if !replayed {
                    append_audit_locked(
                        &tx,
                        &now,
                        canonical_bootstrap.principal_id(),
                        "managed_acceptance.delegation_bootstrap_rebound",
                        delegation_id,
                        &details,
                    )?;
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(json!({
                    "status": "active",
                    "delegation_state": "active",
                    "operation_identity": delegation_id,
                    "product_task_id": task_id,
                    "reviewer_principal_id": reviewer_key_id,
                    "original_principal_id": row.3,
                    "replayed": replayed,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let current_task_version: i64 = tx
                    .query_one(
                        "SELECT version FROM product_tasks WHERE task_id=$1 FOR UPDATE",
                        &[&task_id],
                    )
                    .map(|row| row.get(0))
                    .map_err(|error| error.to_string())?;
                if current_task_version != task_version {
                    return Err("ProductTask version moved during bootstrap recovery".into());
                }
                let row = tx
                    .query_one(
                        "SELECT tenant_id, status, product_task_id, principal_id,
                                manifest_approver_id, expires_at
                         FROM managed_acceptance_delegations WHERE delegation_id=$1 FOR UPDATE",
                        &[&delegation_id],
                    )
                    .map_err(|error| error.to_string())?;
                let stored_tenant: String = row.get(0);
                let status: String = row.get(1);
                let stored_task_id: Option<String> = row.get(2);
                let stored_principal_id: String = row.get(3);
                let stored_manifest_approver_id: String = row.get(4);
                let expires_at: String = row.get(5);
                if stored_tenant != tenant_id {
                    return Err("delegation tenant does not match ProductTask tenant".into());
                }
                if status != "active" {
                    return Err(
                        "bootstrap delegation recovery only permits active pre-admission state"
                            .into(),
                    );
                }
                if is_at_or_before(&expires_at, &now)? {
                    return Err("bootstrap delegation recovery refuses an expired delegation".into());
                }
                if stored_task_id.as_deref() != Some(task_id) {
                    return Err(
                        "bootstrap recovery requires an existing immutable ProductTask binding"
                            .into(),
                    );
                }
                let safe_state: bool = tx
                    .query_one(
                        "SELECT EXISTS(
                            SELECT 1 FROM managed_acceptance_delegations
                             WHERE delegation_id=$1 AND tenant_id=$2 AND status='active'
                               AND product_task_id=$3
                               AND executions_used=0 AND total_cost_usd=0
                               AND proposal_sha256 IS NULL AND proposal_json IS NULL
                               AND spend_authorization_id IS NULL
                               AND manifest_approval_sha256 IS NULL AND manifest_approval_json IS NULL
                               AND spend_body_sha256 IS NULL AND spend_status IS NULL
                               AND spend_body_json IS NULL AND manifest_json IS NULL
                               AND attempt_id IS NULL AND attempt_lease_id IS NULL
                               AND attempt_lease_token IS NULL AND attempt_status IS NULL
                               AND attempt_activator_id IS NULL
                               AND artifact_confirmation_sha256 IS NULL
                               AND artifact_confirmation_json IS NULL
                               AND provider_request_journal_json='[]'
                               AND terminal_receipt_json IS NULL AND terminal_at IS NULL
                               AND revoked_at IS NULL AND artifact_confirmer_id=''
                         )",
                        &[&delegation_id, &tenant_id, &task_id],
                    )
                    .map(|row| row.get(0))
                    .map_err(|error| error.to_string())?;
                if !safe_state {
                    return Err(
                        "bootstrap delegation recovery requires an untouched pre-admission operation"
                            .into(),
                    );
                }
                if stored_manifest_approver_id != reviewer_key_id
                    && stored_manifest_approver_id != stored_principal_id
                {
                    return Err(
                        "bootstrap recovery cannot replace an already rebound reviewer".into(),
                    );
                }
                let replayed = stored_manifest_approver_id == reviewer_key_id;
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET manifest_approver_id=$1,
                             updated_at=CASE WHEN manifest_approver_id=$1 THEN updated_at ELSE $2 END
                         WHERE delegation_id=$3 AND tenant_id=$4 AND status='active'
                           AND product_task_id=$5
                           AND executions_used=0 AND total_cost_usd=0
                           AND proposal_sha256 IS NULL AND proposal_json IS NULL
                           AND spend_authorization_id IS NULL
                           AND manifest_approval_sha256 IS NULL AND manifest_approval_json IS NULL
                           AND spend_body_sha256 IS NULL AND spend_status IS NULL
                           AND spend_body_json IS NULL AND manifest_json IS NULL
                           AND attempt_id IS NULL AND attempt_lease_id IS NULL
                           AND attempt_lease_token IS NULL AND attempt_status IS NULL
                           AND attempt_activator_id IS NULL
                           AND artifact_confirmation_sha256 IS NULL
                           AND artifact_confirmation_json IS NULL
                           AND provider_request_journal_json='[]'
                           AND terminal_receipt_json IS NULL AND terminal_at IS NULL
                           AND revoked_at IS NULL AND artifact_confirmer_id=''
                           AND (manifest_approver_id=$1 OR manifest_approver_id=principal_id)
                           AND EXISTS (
                               SELECT 1 FROM api_key_metadata
                               WHERE key_id=$6 AND tenant_id=$4 AND role='reviewer'
                                 AND scopes_json=$7 AND revoked_at IS NULL
                                 AND (expires_at IS NULL OR CAST(expires_at AS DOUBLE PRECISION) >= $9)
                           )
                           AND EXISTS (
                               SELECT 1 FROM api_key_metadata
                               WHERE key_id=$8 AND tenant_id=$4 AND role IN ('admin','bootstrap')
                                 AND scopes_json=$10 AND revoked_at IS NULL
                                 AND (expires_at IS NULL OR CAST(expires_at AS DOUBLE PRECISION) >= $9)
                           )",
                        &[
                            &reviewer_key_id,
                            &now,
                            &delegation_id,
                            &tenant_id,
                            &task_id,
                            &reviewer_key_id,
                            &reviewer_scopes_json,
                            &LOCAL_BOOTSTRAP_API_KEY_ID,
                            &now_unix,
                            &bootstrap_scopes_json,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("delegation recovery lost its current authenticated pre-admission state".into());
                }
                if !replayed {
                    let details_json = details.to_string();
                    tx.execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, 'managed_acceptance.delegation_bootstrap_rebound', $3, $4)",
                        &[
                            &now,
                            &canonical_bootstrap.principal_id(),
                            &delegation_id,
                            &details_json,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(json!({
                    "status": "active",
                    "delegation_state": "active",
                    "operation_identity": delegation_id,
                    "product_task_id": task_id,
                    "reviewer_principal_id": reviewer_key_id,
                    "original_principal_id": stored_principal_id,
                    "replayed": replayed,
                }))
            }),
        }
    }

    pub fn delegated_authority_state(&self, delegation_id: &str) -> Result<Value, String> {
        let row: (
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                connection
                    .query_row(
                        "SELECT status, spend_status, attempt_status,
                                    terminal_receipt_json, terminal_at
                             FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation_id],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                            ))
                        },
                    )
                    .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT status, spend_status, attempt_status,
                                    terminal_receipt_json, terminal_at
                             FROM managed_acceptance_delegations WHERE delegation_id=$1",
                        &[&delegation_id],
                    )
                    .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4)))
                    .map_err(|error| error.to_string())
            })?,
        };
        Ok(json!({
            "delegation_id": delegation_id,
            "delegation_state": row.0,
            "spend_authorization_state": row.1,
            "attempt_lease_state": row.2,
            "terminal_evidence": row.3
                .as_deref()
                .and_then(|receipt| serde_json::from_str::<Value>(receipt).ok())
                .map(redact_lease_fields),
            "terminal_at": row.4
        }))
    }
}

/// Kind of authenticated principal. Fixture is test-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrincipalKind {
    OperatorApiKey,
    FixturePrincipal,
}

impl PrincipalKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OperatorApiKey => "operator_api_key",
            Self::FixturePrincipal => "fixture_principal",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "operator_api_key" => Ok(Self::OperatorApiKey),
            "fixture_principal" => Ok(Self::FixturePrincipal),
            other => Err(format!("unsupported principal_kind {other}")),
        }
    }
}

/// Authenticated principal. Fields are private; construct only via store authentication
/// or explicit fixture constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPrincipal {
    tenant_id: String,
    principal_id: String,
    principal_kind: PrincipalKind,
    scopes: Vec<String>,
    user_id: String,
    role: String,
}

impl AuthenticatedPrincipal {
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }
    pub fn principal_id(&self) -> &str {
        &self.principal_id
    }
    pub fn principal_kind(&self) -> &PrincipalKind {
        &self.principal_kind
    }
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }
    pub fn user_id(&self) -> &str {
        &self.user_id
    }
    pub fn role(&self) -> &str {
        &self.role
    }

    /// Internal fixture principal for provider-free dry-run support.
    pub(crate) fn fixture_for_dry_run(tenant_id: &str, fixture_id: &str) -> Result<Self, String> {
        let fixture_id = fixture_id.trim();
        if !fixture_id.starts_with("fixture-principal-") {
            return Err("fixture principal id must use fixture-principal- prefix".into());
        }
        Ok(Self {
            tenant_id: tenant_id.trim().to_string(),
            principal_id: fixture_id.to_string(),
            principal_kind: PrincipalKind::FixturePrincipal,
            scopes: ALL_MANAGED_ACCEPTANCE_SCOPES
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            user_id: "fixture-user".into(),
            role: "fixture".into(),
        })
    }

    /// Test-feature-only fixture constructor for external PostgreSQL tests.
    #[cfg(any(test, feature = "pg-tests"))]
    #[doc(hidden)]
    pub fn fixture_for_tests(tenant_id: &str, fixture_id: &str) -> Result<Self, String> {
        Self::fixture_for_dry_run(tenant_id, fixture_id)
    }

    pub fn may_authorize_production_live_start(&self) -> bool {
        matches!(self.principal_kind, PrincipalKind::OperatorApiKey)
            && !is_forbidden_principal_id(&self.principal_id)
            && !is_forbidden_role(&self.role)
            && self.has_scope(SCOPE_SPEND_AUTHORIZE)
            && self.has_scope(SCOPE_ATTEMPT_ADMIT)
    }

    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }

    pub(crate) fn require_scope(&self, scope: &str) -> Result<(), String> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err(format!("principal missing required scope {scope}"))
        }
    }
}

/// Caller-submitted risk acknowledgement. Phrase is typed by the operator; required phrase
/// and scope/expiry are loaded from the immutable persisted decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskAcknowledgementRequest {
    pub decision_id: String,
    pub expected_decision_body_sha256: String,
    pub expected_residual_finding_sha256: String,
    pub submitted_phrase: String,
    pub explicit_go: bool,
}

/// Typed cost authority (no boolean "declared").
#[derive(Debug, Clone, PartialEq)]
pub enum CostAuthority {
    ProviderReported {
        max_cost: f64,
        currency: String,
    },
    LocalEstimate {
        max_cost: f64,
        currency: String,
        provider: String,
        model: String,
        pricing_table_version: String,
        estimate_semantics: String,
    },
    CostUnavailable,
}

impl CostAuthority {
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::ProviderReported { .. } => "provider_reported",
            Self::LocalEstimate { .. } => "local_estimate",
            Self::CostUnavailable => "cost_unavailable",
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::ProviderReported { max_cost, currency } => json!({
                "kind": "provider_reported",
                "max_cost": max_cost,
                "currency": currency,
                "monetary_ceiling_enforced": true,
            }),
            Self::LocalEstimate {
                max_cost,
                currency,
                provider,
                model,
                pricing_table_version,
                estimate_semantics,
            } => json!({
                "kind": "local_estimate",
                "max_cost": max_cost,
                "currency": currency,
                "provider": provider,
                "model": model,
                "pricing_table_version": pricing_table_version,
                "estimate_semantics": estimate_semantics,
                "monetary_ceiling_enforced": true,
                "estimate_only": true,
            }),
            Self::CostUnavailable => json!({
                "kind": "cost_unavailable",
                "monetary_ceiling_enforced": false,
                "note": "rely on request/token/time caps; no monetary ceiling claimed",
            }),
        }
    }

    pub fn from_json(v: &Value) -> Result<Self, String> {
        let kind = v
            .get("kind")
            .and_then(Value::as_str)
            .ok_or("cost_authority.kind required")?;
        match kind {
            "cost_unavailable" => Ok(Self::CostUnavailable),
            "provider_reported" => {
                let max_cost = v
                    .get("max_cost")
                    .and_then(Value::as_f64)
                    .ok_or("provider_reported requires max_cost")?;
                if max_cost <= 0.0 {
                    return Err("provider_reported max_cost must be positive".into());
                }
                Ok(Self::ProviderReported {
                    max_cost,
                    currency: v
                        .get("currency")
                        .and_then(Value::as_str)
                        .unwrap_or("USD")
                        .to_string(),
                })
            }
            "local_estimate" => {
                let max_cost = v
                    .get("max_cost")
                    .and_then(Value::as_f64)
                    .ok_or("local_estimate requires max_cost")?;
                if max_cost <= 0.0 {
                    return Err("local_estimate max_cost must be positive".into());
                }
                Ok(Self::LocalEstimate {
                    max_cost,
                    currency: v
                        .get("currency")
                        .and_then(Value::as_str)
                        .unwrap_or("USD")
                        .to_string(),
                    provider: required_str(v, "provider")?,
                    model: required_str(v, "model")?,
                    pricing_table_version: required_str(v, "pricing_table_version")?,
                    estimate_semantics: required_str(v, "estimate_semantics")?,
                })
            }
            other => Err(format!("unsupported cost_authority.kind {other}")),
        }
    }
}

/// Request to issue a one-use spend authorization (execution authority).
/// Every budget/identity field is bound into the spend receipt and compared to the
/// persisted decision trial envelope; expansion or mismatch fails closed.
#[derive(Debug, Clone, PartialEq)]
pub struct SpendAuthorizationRequest {
    pub risk_authorization_id: String,
    pub product_task_id: String,
    pub workflow_id: Option<String>,
    pub workflow_node_id: Option<String>,
    pub execution_id: String,
    pub attempt_id: String,
    pub binary_path: String,
    pub binary_version: String,
    pub binary_sha256: String,
    /// Required for a newly issued runtime-profile-backed attempt. `None`
    /// remains readable only for historical Codex receipts.
    pub runtime_profile_sha256: Option<String>,
    pub capability_probe_sha256: Option<String>,
    pub provider_kind: String,
    pub provider_host: String,
    pub provider_base_url: String,
    pub admitted_endpoint_paths: Vec<String>,
    pub model: String,
    pub target_repo: String,
    pub target_main_sha: String,
    pub output_branch_prefix: String,
    pub draft_pr_only: bool,
    pub max_provider_requests: u64,
    pub max_retries: u64,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_total_tokens: u64,
    pub max_wall_time_ms: u64,
    pub cost_authority: CostAuthority,
    pub cancellation_identity: String,
    pub rollback_identity: String,
}

/// Immutable, runtime-observed inputs presented at the sole managed-Codex
/// production spawn boundary.  These are not node declarations: the store
/// reloads the node, ProductTask, workspace, spend, decision and risk owners
/// and compares each fact before issuing a lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedCodexLaunchFacts {
    pub run_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub workspace_path: PathBuf,
    pub executable_path: PathBuf,
    pub executable_version: String,
    pub executable_sha256: String,
    pub runtime_profile_sha256: Option<String>,
    pub capability_probe_sha256: Option<String>,
    pub model: String,
}

/// Store-issued, one-use lease for a managed Codex process.  The lease token is
/// deliberately not serialized into generic replay responses; it stays inside
/// the executor/store ownership seam and is required for every terminal write.
#[derive(Debug, Clone)]
pub struct ManagedCodexSpawnLease {
    facts: ManagedCodexLaunchFacts,
    product_task_id: String,
    product_task_version: u64,
    tenant_id: String,
    principal_id: String,
    principal_kind: PrincipalKind,
    spend_authorization_id: String,
    attempt_id: String,
    execution_id: String,
    lease_token: String,
    spend_body: Value,
}

impl ManagedCodexSpawnLease {
    pub fn facts(&self) -> &ManagedCodexLaunchFacts {
        &self.facts
    }

    pub fn product_task_id(&self) -> &str {
        &self.product_task_id
    }

    pub fn product_task_version(&self) -> u64 {
        self.product_task_version
    }

    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    pub fn principal_id(&self) -> &str {
        &self.principal_id
    }

    pub fn principal_kind(&self) -> &PrincipalKind {
        &self.principal_kind
    }

    pub fn spend_authorization_id(&self) -> &str {
        &self.spend_authorization_id
    }

    pub fn attempt_id(&self) -> &str {
        &self.attempt_id
    }

    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }

    pub fn spend_body(&self) -> &Value {
        &self.spend_body
    }

    /// The only production constructor for a gateway-start capability.  The
    /// token itself stays private to this store/executor ownership seam.
    pub(crate) fn gateway_start_permit(
        &self,
    ) -> crate::cli::codex_budget_authority::CodexGatewayStartPermit {
        crate::cli::codex_budget_authority::CodexGatewayStartPermit::managed_store_lease(
            &self.execution_id,
            &self.lease_token,
        )
    }
}

fn is_forbidden_principal_id(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    [
        "agent",
        "bot",
        "automation",
        "ci",
        "github-actions",
        "system",
        "self",
        "none",
        "fixture",
        "test",
        "local-dev",
        "anonymous",
    ]
    .iter()
    .any(|f| {
        lower == *f || lower.starts_with(&format!("{f}-")) || lower.starts_with(&format!("{f}_"))
    })
}

fn is_forbidden_role(role: &str) -> bool {
    let lower = role.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "agent" | "bot" | "automation" | "ci" | "system" | "anonymous" | "fixture"
    )
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn canonical_json(value: &Value) -> Result<String, String> {
    Ok(sort_value(value).to_string())
}

fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                if let Some(v) = map.get(&k) {
                    out.insert(k, sort_value(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}

fn required_str(v: &Value, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{key} required"))
}

/// Parse a canonical RFC3339 timestamp and normalize to UTC.
fn parse_rfc3339_utc(field: &str, value: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(value.trim())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|_| format!("{field} must be canonical RFC3339/UTC"))
}

/// True when `expires_at` is at or before `now` using parsed UTC instants (not lexical strings).
fn is_at_or_before(expires_at: &str, now: &str) -> Result<bool, String> {
    let exp = parse_rfc3339_utc("expires_at", expires_at)?;
    let n = parse_rfc3339_utc("now", now)?;
    Ok(exp <= n)
}

/// Strictly after using parsed UTC instants.
fn is_strictly_after(expires_at: &str, now: &str) -> Result<bool, String> {
    let exp = parse_rfc3339_utc("expires_at", expires_at)?;
    let n = parse_rfc3339_utc("now", now)?;
    Ok(exp > n)
}

/// Canonical attempt-manifest hash: sort + hash body without the self-referential sha field.
pub fn compute_attempt_manifest_sha256(manifest: &Value) -> Result<String, String> {
    let mut body = sort_value(manifest);
    if let Value::Object(ref mut map) = body {
        map.remove("manifest_sha256");
    }
    Ok(sha256_hex(canonical_json(&body)?.as_bytes()))
}

/// Build the complete authority-bound attempt manifest from a spend body.
pub fn build_attempt_authority_manifest(spend_body: &Value) -> Result<Value, String> {
    let require = |key: &str| -> Result<Value, String> {
        spend_body
            .get(key)
            .cloned()
            .ok_or_else(|| format!("spend body missing {key} for attempt manifest"))
    };
    let manifest = sort_value(&json!({
        "schema_version": "managed_acceptance_attempt_manifest.v1",
        "product_task_id": require("product_task_id")?,
        "workflow_id": spend_body.get("workflow_id").cloned().unwrap_or(Value::Null),
        "workflow_node_id": spend_body.get("workflow_node_id").cloned().unwrap_or(Value::Null),
        "execution_id": require("execution_id")?,
        "attempt_id": require("attempt_id")?,
        "binary_path": require("binary_path")?,
        "binary_version": require("binary_version")?,
        "binary_sha256": require("binary_sha256")?,
        "runtime_profile_sha256": spend_body.get("runtime_profile_sha256").cloned().unwrap_or(Value::Null),
        "capability_probe_sha256": spend_body.get("capability_probe_sha256").cloned().unwrap_or(Value::Null),
        "provider_kind": require("provider_kind")?,
        "provider_host": require("provider_host")?,
        "provider_base_url": require("provider_base_url")?,
        "admitted_endpoint_paths": require("admitted_endpoint_paths")?,
        "model": require("model")?,
        "target_repo": require("target_repo")?,
        "target_main_sha": require("target_main_sha")?,
        "output_branch_prefix": require("output_branch_prefix")?,
        "draft_pr_only": require("draft_pr_only")?,
        "max_provider_requests": require("max_provider_requests")?,
        "max_retries": require("max_retries")?,
        "max_input_tokens": require("max_input_tokens")?,
        "max_output_tokens": require("max_output_tokens")?,
        "max_total_tokens": require("max_total_tokens")?,
        "max_wall_time_ms": require("max_wall_time_ms")?,
        "cost_authority": require("cost_authority")?,
        "cancellation_identity": require("cancellation_identity")?,
        "rollback_identity": require("rollback_identity")?,
        "decision_body_sha256": spend_body.get("decision_body_sha256").cloned().unwrap_or(Value::Null),
        "spend_authorization_id": spend_body.get("spend_authorization_id").cloned().unwrap_or(Value::Null),
    }));
    let sha = compute_attempt_manifest_sha256(&manifest)?;
    let mut out = manifest;
    if let Value::Object(ref mut map) = out {
        map.insert("manifest_sha256".into(), json!(sha));
    }
    Ok(out)
}

/// Record an immutable decision status transition as a hash-linked receipt.
/// The decision body authority hash stays stable; status is not mutated under that hash.
fn decision_status_transition_receipt(
    decision_id: &str,
    tenant_id: &str,
    decision_body_sha256: &str,
    residual_finding_sha256: &str,
    sequence: i64,
    previous_transition_sequence: Option<i64>,
    previous_transition_sha256: Option<&str>,
    from_status: &str,
    to_status: &str,
    principal: &AuthenticatedPrincipal,
    at: &str,
    reason: &str,
) -> Result<Value, String> {
    let body = sort_value(&json!({
        "schema_version": "managed_acceptance_decision_status_transition.v2",
        "decision_id": decision_id,
        "tenant_id": tenant_id,
        "decision_body_sha256": decision_body_sha256,
        "residual_finding_sha256": residual_finding_sha256,
        "sequence": sequence,
        "previous_transition_sequence": previous_transition_sequence,
        "previous_transition_sha256": previous_transition_sha256,
        "from_status": from_status,
        "to_status": to_status,
        "actor_principal_kind": principal.principal_kind.as_str(),
        "actor_principal_id": principal.principal_id,
        "at": at,
        "reason": reason,
    }));
    let transition_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
    let mut receipt = body;
    if let Value::Object(ref mut map) = receipt {
        map.insert(
            "transition_receipt_id".into(),
            json!(format!("matr-{}", &transition_sha256[..24])),
        );
        map.insert("transition_sha256".into(), json!(transition_sha256));
        map.insert("receipt_sha256".into(), json!(transition_sha256));
    }
    Ok(receipt)
}

/// Validate the durable decision-status chain before it is used as authority.
/// A receipt is not evidence merely because it has a hash-shaped field: its
/// self-hash, immutable decision binding, predecessor link, and current
/// decision state must all agree.
fn validate_managed_acceptance_decision_transition_chain(
    decision: &Value,
    receipts: &[Value],
) -> Result<(), String> {
    let decision_id = required_str(decision, "decision_id")?;
    let tenant_id = required_str(decision, "tenant_id")?;
    let decision_body_sha256 = required_str(decision, "decision_body_sha256")?;
    let residual_finding_sha256 = required_str(decision, "residual_finding_sha256")?;
    let current_status = required_str(decision, "status")?;
    if receipts.is_empty() {
        return Err("managed Codex decision transition receipt set is empty".to_string());
    }

    struct TransitionRecord {
        sequence: i64,
        from_status: String,
        to_status: String,
        previous_transition_sequence: Option<i64>,
        previous_transition_sha256: Option<String>,
    }

    let mut records = std::collections::BTreeMap::<String, TransitionRecord>::new();
    for receipt in receipts {
        if receipt.get("schema_version").and_then(Value::as_str)
            != Some("managed_acceptance_decision_status_transition.v2")
        {
            return Err("managed Codex decision transition receipt schema is invalid".to_string());
        }
        for (field, expected) in [
            ("decision_id", decision_id.as_str()),
            ("tenant_id", tenant_id.as_str()),
            ("decision_body_sha256", decision_body_sha256.as_str()),
            ("residual_finding_sha256", residual_finding_sha256.as_str()),
        ] {
            if receipt.get(field).and_then(Value::as_str) != Some(expected) {
                return Err(format!(
                    "managed Codex decision transition receipt {field} mismatches its owner"
                ));
            }
        }
        let transition_sha256 = required_str(receipt, "transition_sha256")?;
        if receipt.get("receipt_sha256").and_then(Value::as_str) != Some(transition_sha256.as_str())
        {
            return Err("managed Codex transition receipt hash alias is invalid".to_string());
        }
        let sequence = receipt
            .get("sequence")
            .and_then(Value::as_i64)
            .filter(|value| *value >= 1)
            .ok_or("managed Codex transition sequence is invalid")?;
        if transition_sha256.len() != 64
            || !transition_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("managed Codex decision transition receipt hash is invalid".to_string());
        }
        let receipt_id = required_str(receipt, "transition_receipt_id")?;
        if receipt_id != format!("matr-{}", &transition_sha256[..24]) {
            return Err(
                "managed Codex decision transition receipt identifier is stale".to_string(),
            );
        }
        for field in [
            "from_status",
            "to_status",
            "actor_principal_kind",
            "actor_principal_id",
            "at",
            "reason",
        ] {
            required_str(receipt, field)?;
        }
        let mut hash_body = receipt.clone();
        let body = hash_body.as_object_mut().ok_or_else(|| {
            "managed Codex decision transition receipt must be an object".to_string()
        })?;
        body.remove("transition_receipt_id");
        body.remove("transition_sha256");
        body.remove("receipt_sha256");
        if sha256_hex(canonical_json(&hash_body)?.as_bytes()) != transition_sha256 {
            return Err(
                "managed Codex decision transition receipt hash does not match content".to_string(),
            );
        }
        let from_status = required_str(receipt, "from_status")?;
        let to_status = required_str(receipt, "to_status")?;
        let previous_transition_sha256 = match receipt.get("previous_transition_sha256") {
            Some(Value::Null) | None => None,
            Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
            _ => {
                return Err("managed Codex decision transition predecessor is malformed".to_string())
            }
        };
        let previous_transition_sequence = match receipt.get("previous_transition_sequence") {
            Some(Value::Null) | None => None,
            Some(value) => Some(
                value
                    .as_i64()
                    .filter(|sequence| *sequence >= 1)
                    .ok_or("managed Codex transition predecessor sequence is invalid")?,
            ),
        };
        if sequence == 1
            && (previous_transition_sequence.is_some() || previous_transition_sha256.is_some())
        {
            return Err("genesis transition cannot have a predecessor".to_string());
        }
        if sequence > 1
            && (previous_transition_sequence.is_none() || previous_transition_sha256.is_none())
        {
            return Err("non-genesis transition requires one predecessor".to_string());
        }
        if !matches!(
            (from_status.as_str(), to_status.as_str()),
            ("draft_pending_operator", "operator_accepted") | ("operator_accepted", "revoked")
        ) {
            return Err("managed Codex decision transition edge is invalid".to_string());
        }
        if records
            .insert(
                transition_sha256,
                TransitionRecord {
                    sequence,
                    from_status,
                    to_status,
                    previous_transition_sequence,
                    previous_transition_sha256,
                },
            )
            .is_some()
        {
            return Err("managed Codex decision transition receipt hash is duplicated".to_string());
        }
    }

    let genesis = records
        .iter()
        .filter_map(|(hash, record)| {
            record
                .previous_transition_sha256
                .is_none()
                .then_some(hash.clone())
        })
        .collect::<Vec<_>>();
    let [genesis] = genesis.as_slice() else {
        return Err("managed Codex decision transition chain must have one genesis".to_string());
    };
    let mut children = std::collections::BTreeMap::<String, Vec<String>>::new();
    for (hash, record) in &records {
        let Some(previous) = record.previous_transition_sha256.as_ref() else {
            continue;
        };
        if !records.contains_key(previous) {
            return Err("managed Codex decision transition predecessor is missing".to_string());
        }
        let next = children.entry(previous.clone()).or_default();
        next.push(hash.clone());
        if next.len() > 1 {
            return Err("managed Codex decision transition chain is forked".to_string());
        }
    }

    let mut visited = std::collections::BTreeSet::new();
    let mut current_hash = genesis.clone();
    let mut expected_from_status = "draft_pending_operator".to_string();
    let mut expected_sequence = 1_i64;
    let latest_to_status = loop {
        let record = records
            .get(&current_hash)
            .ok_or_else(|| "managed Codex decision transition chain is malformed".to_string())?;
        if record.from_status != expected_from_status {
            return Err("managed Codex decision transition predecessor link is stale".to_string());
        }
        if record.sequence != expected_sequence {
            return Err("managed Codex decision transition sequence is not contiguous".to_string());
        }
        if !visited.insert(current_hash.clone()) {
            return Err("managed Codex decision transition chain contains a cycle".to_string());
        }
        let latest = record.to_status.clone();
        match children.get(&current_hash) {
            None => break latest,
            Some(next) if next.len() == 1 => {
                let next_record = records.get(&next[0]).ok_or_else(|| {
                    "managed Codex decision transition chain is malformed".to_string()
                })?;
                if next_record.previous_transition_sequence != Some(record.sequence) {
                    return Err(
                        "managed Codex decision transition predecessor sequence is stale"
                            .to_string(),
                    );
                }
                current_hash = next[0].clone();
                expected_from_status = latest;
                expected_sequence += 1;
            }
            Some(_) => return Err("managed Codex decision transition chain is forked".to_string()),
        }
    };
    if visited.len() != records.len() {
        return Err("managed Codex decision transition chain is disconnected".to_string());
    }
    if latest_to_status != current_status {
        return Err(
            "managed Codex current decision status is not backed by the latest transition receipt"
                .to_string(),
        );
    }
    Ok(())
}

fn persist_transition_sqlite(
    tx: &rusqlite::Transaction<'_>,
    decision: &Value,
    from_status: &str,
    to_status: &str,
    principal: &AuthenticatedPrincipal,
    now: &str,
    reason: &str,
) -> Result<Value, String> {
    let decision_id = required_str(decision, "decision_id")?;
    let tenant_id = required_str(decision, "tenant_id")?;
    let decision_sha = required_str(decision, "decision_body_sha256")?;
    let residual_sha = required_str(decision, "residual_finding_sha256")?;
    let previous = tx
        .query_row(
            "SELECT sequence, transition_sha256 FROM managed_acceptance_decision_transition_receipts
             WHERE decision_id=?1 AND to_status=?2
             ORDER BY sequence DESC LIMIT 1",
            params![decision_id, from_status],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (sequence, previous_transition_sequence, previous_transition_sha256) = match previous {
        Some((previous_sequence, previous_hash)) => (
            previous_sequence + 1,
            Some(previous_sequence),
            Some(previous_hash),
        ),
        None => (1, None, None),
    };
    let receipt = decision_status_transition_receipt(
        &decision_id,
        &tenant_id,
        &decision_sha,
        &residual_sha,
        sequence,
        previous_transition_sequence,
        previous_transition_sha256.as_deref(),
        from_status,
        to_status,
        principal,
        now,
        reason,
    )?;
    let receipt_id = required_str(&receipt, "transition_receipt_id")?;
    let transition_sha = required_str(&receipt, "transition_sha256")?;
    tx.execute(
        "INSERT INTO managed_acceptance_decision_transition_receipts (
            transition_receipt_id, decision_id, tenant_id, decision_body_sha256,
            sequence, previous_transition_sequence, previous_transition_sha256,
            transition_sha256, from_status, to_status,
            actor_principal_kind, actor_principal_id, receipt_json, created_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
         ON CONFLICT(decision_id, transition_sha256) DO NOTHING",
        params![
            receipt_id,
            decision_id,
            tenant_id,
            decision_sha,
            sequence,
            previous_transition_sequence,
            previous_transition_sha256,
            transition_sha,
            from_status,
            to_status,
            principal.principal_kind.as_str(),
            principal.principal_id,
            receipt.to_string(),
            now,
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(receipt)
}

#[cfg(feature = "pg")]
fn persist_transition_pg(
    tx: &mut postgres::Transaction<'_>,
    decision: &Value,
    from_status: &str,
    to_status: &str,
    principal: &AuthenticatedPrincipal,
    now: &str,
    reason: &str,
) -> Result<Value, String> {
    let decision_id = required_str(decision, "decision_id")?;
    let tenant_id = required_str(decision, "tenant_id")?;
    let decision_sha = required_str(decision, "decision_body_sha256")?;
    let residual_sha = required_str(decision, "residual_finding_sha256")?;
    let previous = tx
        .query_opt(
            "SELECT sequence, transition_sha256 FROM managed_acceptance_decision_transition_receipts
             WHERE decision_id=$1 AND to_status=$2
             ORDER BY sequence DESC LIMIT 1 FOR UPDATE",
            &[&decision_id, &from_status],
        )
        .map_err(|error| error.to_string())?
        .map(|row| (row.get::<_, i64>(0), row.get::<_, String>(1)));
    let (sequence, previous_transition_sequence, previous_transition_sha256) = match previous {
        Some((previous_sequence, previous_hash)) => (
            previous_sequence + 1,
            Some(previous_sequence),
            Some(previous_hash),
        ),
        None => (1, None, None),
    };
    let receipt = decision_status_transition_receipt(
        &decision_id,
        &tenant_id,
        &decision_sha,
        &residual_sha,
        sequence,
        previous_transition_sequence,
        previous_transition_sha256.as_deref(),
        from_status,
        to_status,
        principal,
        now,
        reason,
    )?;
    let receipt_id = required_str(&receipt, "transition_receipt_id")?;
    let transition_sha = required_str(&receipt, "transition_sha256")?;
    tx.execute(
        "INSERT INTO managed_acceptance_decision_transition_receipts (
            transition_receipt_id, decision_id, tenant_id, decision_body_sha256,
            sequence, previous_transition_sequence, previous_transition_sha256,
            transition_sha256, from_status, to_status,
            actor_principal_kind, actor_principal_id, receipt_json, created_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
         ON CONFLICT (decision_id, transition_sha256) DO NOTHING",
        &[
            &receipt_id,
            &decision_id,
            &tenant_id,
            &decision_sha,
            &sequence,
            &previous_transition_sequence,
            &previous_transition_sha256,
            &transition_sha,
            &from_status,
            &to_status,
            &principal.principal_kind.as_str(),
            &principal.principal_id,
            &receipt.to_string(),
            &now,
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(receipt)
}

impl LocalProductStore {
    /// Re-authenticate the immutable environment bootstrap owner from the
    /// store-owned key metadata. This capability is intentionally narrower
    /// than normal managed-acceptance authentication: identity delegation is
    /// sufficient, while child principals must be authenticated separately
    /// with their own least-privilege scopes.
    pub fn authenticate_bootstrap_identity_delegation_principal(
        &self,
        tenant_id: &str,
        now_unix: Option<f64>,
    ) -> Result<AuthenticatedPrincipal, String> {
        let tenant_id = tenant_id.trim();
        if tenant_id != LOCAL_BOOTSTRAP_TENANT_ID {
            return Err("canonical bootstrap tenant does not match local authority".into());
        }
        let meta = self
            .get_api_key_metadata_for_tenant(LOCAL_BOOTSTRAP_API_KEY_ID, tenant_id)?
            .ok_or("canonical bootstrap key metadata is missing")?;
        if meta.get("tenant_id").and_then(Value::as_str) != Some(tenant_id) {
            return Err("canonical bootstrap key tenant binding is missing or invalid".into());
        }
        if meta.get("revoked_at").and_then(Value::as_str).is_some() {
            return Err("canonical bootstrap key is revoked".into());
        }
        if let Some(exp) = meta.get("expires_at").and_then(Value::as_f64) {
            let now = now_unix.unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_secs_f64())
                    .unwrap_or(0.0)
            });
            if now > exp {
                return Err("canonical bootstrap key is expired".into());
            }
        }
        let user_id = meta
            .get("user_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("canonical bootstrap user_id is missing")?;
        let role = meta
            .get("role")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "admin" | "bootstrap"))
            .ok_or("canonical bootstrap role is invalid")?;
        let scopes: Vec<String> = meta
            .get("scopes")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        validate_bootstrap_identity_delegation_scopes(&scopes)?;
        let _ = self.touch_api_key_last_used_for_tenant(LOCAL_BOOTSTRAP_API_KEY_ID, tenant_id);
        Ok(AuthenticatedPrincipal {
            tenant_id: tenant_id.to_string(),
            principal_id: LOCAL_BOOTSTRAP_API_KEY_ID.to_string(),
            principal_kind: PrincipalKind::OperatorApiKey,
            scopes,
            user_id: user_id.to_string(),
            role: role.to_string(),
        })
    }

    /// Derive a production principal from verified store-owned API key metadata.
    /// Rejects missing, revoked, expired, inactive, forbidden, or under-scoped keys.
    pub fn authenticate_managed_acceptance_principal(
        &self,
        tenant_id: &str,
        key_id: &str,
        now_unix: Option<f64>,
    ) -> Result<AuthenticatedPrincipal, String> {
        self.authenticate_managed_acceptance_principal_inner(tenant_id, key_id, now_unix, true)
    }

    /// Read-only managed-acceptance authentication for inspection/preflight
    /// callers. It validates the canonical metadata and scopes without
    /// touching `api_key_metadata.last_used_at`.
    pub fn authenticate_managed_acceptance_principal_read_only(
        &self,
        tenant_id: &str,
        key_id: &str,
        now_unix: Option<f64>,
    ) -> Result<AuthenticatedPrincipal, String> {
        self.authenticate_managed_acceptance_principal_inner(tenant_id, key_id, now_unix, false)
    }

    fn authenticate_managed_acceptance_principal_inner(
        &self,
        tenant_id: &str,
        key_id: &str,
        now_unix: Option<f64>,
        touch_last_used: bool,
    ) -> Result<AuthenticatedPrincipal, String> {
        let tenant_id = tenant_id.trim();
        let key_id = key_id.trim();
        if tenant_id.is_empty() || key_id.is_empty() {
            return Err("tenant_id and key_id required".into());
        }
        if is_forbidden_principal_id(key_id) {
            return Err(format!(
                "key_id {key_id:?} is not admitted for operator authority"
            ));
        }
        let meta = self
            .get_api_key_metadata_for_tenant(key_id, tenant_id)?
            .ok_or_else(|| {
                format!("api key {key_id} not found or tenant binding is missing or invalid")
            })?;
        if meta.get("tenant_id").and_then(Value::as_str) != Some(tenant_id) {
            return Err(
                "api key tenant binding is missing or does not match requested tenant".into(),
            );
        }
        if meta.get("revoked_at").and_then(Value::as_str).is_some() {
            return Err("api key revoked".into());
        }
        if let Some(exp) = meta.get("expires_at").and_then(Value::as_f64) {
            let now = now_unix.unwrap_or_else(|| {
                // RFC3339 clock: approximate from wall clock seconds when not provided.
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0)
            });
            if now > exp {
                return Err("api key expired".into());
            }
        }
        let user_id = meta
            .get("user_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let role = meta
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if user_id.is_empty() || is_forbidden_principal_id(&user_id) {
            return Err("api key user_id not admitted".into());
        }
        if role.is_empty() || is_forbidden_role(&role) {
            return Err("api key role not admitted for operator authority".into());
        }
        let scopes: Vec<String> = meta
            .get("scopes")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let principal = AuthenticatedPrincipal {
            tenant_id: tenant_id.to_string(),
            principal_id: key_id.to_string(),
            principal_kind: PrincipalKind::OperatorApiKey,
            scopes,
            user_id,
            role,
        };
        // Must hold at least risk-ack scope to be usable as a managed-acceptance principal.
        principal.require_scope(SCOPE_RISK_ACKNOWLEDGE)?;
        if touch_last_used {
            let _ = self.touch_api_key_last_used_for_tenant(key_id, tenant_id);
        }
        Ok(principal)
    }

    /// Authenticate a canonical managed identity whose store record is
    /// immutably bound to the requested tenant. The generic production
    /// authentication path is strict as well; this named variant documents
    /// the tenant-bound contract at identity-reissuance call sites.
    pub fn authenticate_managed_acceptance_principal_for_tenant(
        &self,
        tenant_id: &str,
        key_id: &str,
        now_unix: Option<f64>,
    ) -> Result<AuthenticatedPrincipal, String> {
        let principal =
            self.authenticate_managed_acceptance_principal(tenant_id, key_id, now_unix)?;
        let metadata = self
            .get_api_key_metadata_for_tenant(key_id, tenant_id)?
            .ok_or_else(|| format!("api key {key_id} not found"))?;
        if metadata.get("tenant_id").and_then(Value::as_str) != Some(tenant_id.trim()) {
            return Err("canonical managed identity tenant binding is missing or invalid".into());
        }
        Ok(principal)
    }

    /// Persist a draft decision body. Body must already carry full canonical fields including
    /// acknowledgement.required_phrase and trial envelope.
    ///
    /// Finite `expires_at` is mandatory. The stored `decision_body_sha256` hashes the immutable
    /// authority envelope (body, residual, expiry, invalidation). Mutable status is tracked via
    /// separate hash-linked transition receipts — never rewritten under the body hash.
    pub fn upsert_managed_acceptance_decision(
        &self,
        tenant_id: &str,
        decision_body: &Value,
        residual_finding_sha256: &str,
        status: &str,
        principal: Option<&AuthenticatedPrincipal>,
        expires_at: Option<&str>,
    ) -> Result<Value, String> {
        validate_decision_status(status)?;
        if status != "draft_pending_operator" {
            return Err(
                "managed acceptance decisions must be created in draft_pending_operator; transitions are receipt-owned"
                    .to_string(),
            );
        }
        if residual_finding_sha256.len() != 64
            || !residual_finding_sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit())
        {
            return Err("residual_finding_sha256 must be 64 hex chars".into());
        }
        let expires_at = require_finite_expiry(expires_at)?;
        let body = sort_value(decision_body);
        if body.get("status").and_then(Value::as_str) != Some("draft_pending_operator") {
            return Err(
                "managed acceptance decision body status must be draft_pending_operator; it is not transition evidence"
                    .to_string(),
            );
        }
        let now = self.now();
        // Require acknowledgement phrase embedded for store-derived accept.
        let _ = body
            .pointer("/acknowledgement/required_phrase")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or("decision body must embed acknowledgement.required_phrase")?;
        if body.get("effect_envelope").is_some() {
            validate_effect_envelope_body(&body, tenant_id, principal, &now)?;
        } else {
            let trial = body
                .get("trial_envelope")
                .ok_or("decision body must embed trial_envelope")?;
            validate_trial_envelope_shape(trial)?;
        }
        let invalidation_state = body
            .get("invalidation_state")
            .and_then(Value::as_str)
            .unwrap_or("none");
        if !matches!(
            invalidation_state,
            "none" | "invalidated" | "revoked" | "expired"
        ) {
            return Err(format!("invalid invalidation_state {invalidation_state}"));
        }
        if matches!(status, "invalidated" | "revoked" | "expired") && invalidation_state == "none" {
            return Err(
                "terminal invalidation status requires matching invalidation_state in body".into(),
            );
        }
        let decision_body_sha256 = canonical_decision_authority_hash(
            &body,
            residual_finding_sha256,
            &expires_at,
            invalidation_state,
        )?;
        let decision_id = body
            .get("decision_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("mad-{}", Uuid::new_v4()));
        if !is_strictly_after(&expires_at, &now)? {
            return Err("decision expires_at must be strictly after store clock now".into());
        }
        if let Some(effect) = body.get("effect_envelope") {
            let effect_expires_at = effect
                .get("expires_at")
                .and_then(Value::as_str)
                .ok_or("effect envelope expires_at is missing")?;
            if require_finite_expiry(Some(effect_expires_at))? != expires_at {
                return Err("effect envelope expiry does not match decision expiry".into());
            }
        }
        let principal_kind = principal
            .map(|p| p.principal_kind.as_str().to_string())
            .unwrap_or_else(|| "operator_api_key".into());
        let principal_id = principal.map(|p| p.principal_id.clone());
        if let Some(p) = principal {
            if p.tenant_id != tenant_id {
                return Err("principal tenant_id mismatch".into());
            }
        }

        let row = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                if let Some((existing_sha, _existing_status)) = tx
                    .query_row(
                        "SELECT decision_body_sha256, status FROM managed_acceptance_decisions WHERE decision_id=?1",
                        params![decision_id],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                {
                    if existing_sha != decision_body_sha256 {
                        return Err(
                            "managed acceptance decision body conflict for decision_id".into()
                        );
                    }
                    let row = load_decision_sqlite(&tx, &decision_id)?;
                    return Ok(row);
                }
                tx.execute(
                    "INSERT INTO managed_acceptance_decisions (
                        decision_id, tenant_id, decision_body_sha256, residual_finding_sha256,
                        status, principal_kind, principal_id, body_json, created_at, updated_at, expires_at, revoked_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,?10,NULL)",
                    params![
                        decision_id,
                        tenant_id,
                        decision_body_sha256,
                        residual_finding_sha256,
                        status,
                        principal_kind,
                        principal_id,
                        body.to_string(),
                        now,
                        expires_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    principal_id.as_deref().unwrap_or("system"),
                    "managed_acceptance.decision_upsert",
                    &decision_id,
                    &json!({
                        "decision_body_sha256": decision_body_sha256,
                        "status": status,
                        "tenant_id": tenant_id,
                        "expires_at": expires_at,
                    }),
                )?;
                let row = load_decision_sqlite(&tx, &decision_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!("mad:{tenant_id}:{decision_id}")],
                )
                .map_err(|e| e.to_string())?;
                if let Some(row) = tx
                    .query_opt(
                        "SELECT decision_body_sha256, status FROM managed_acceptance_decisions WHERE decision_id=$1 FOR UPDATE",
                        &[&decision_id],
                    )
                    .map_err(|e| e.to_string())?
                {
                    let existing_sha: String = row.get(0);
                    if existing_sha != decision_body_sha256 {
                        return Err(
                            "managed acceptance decision body conflict for decision_id".into()
                        );
                    }
                    return load_decision_pg(&mut tx, &decision_id);
                }
                tx.execute(
                    "INSERT INTO managed_acceptance_decisions (
                        decision_id, tenant_id, decision_body_sha256, residual_finding_sha256,
                        status, principal_kind, principal_id, body_json, created_at, updated_at, expires_at, revoked_at
                     ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,$10,NULL)",
                    &[
                        &decision_id,
                        &tenant_id,
                        &decision_body_sha256,
                        &residual_finding_sha256,
                        &status,
                        &principal_kind,
                        &principal_id,
                        &body.to_string(),
                        &now,
                        &expires_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_effect_audit_pg(
                    &mut tx,
                    &now,
                    principal_id.as_deref().unwrap_or("system"),
                    "managed_acceptance.decision_upsert",
                    &decision_id,
                    &json!({
                        "decision_body_sha256": decision_body_sha256,
                        "status": status,
                        "tenant_id": tenant_id,
                        "expires_at": expires_at,
                    }),
                )?;
                let row = load_decision_pg(&mut tx, &decision_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }?;
        Ok(row)
    }

    /// Persist an immutable parent effect envelope as a managed-acceptance
    /// decision draft. The caller must be the owner recorded in the envelope;
    /// acceptance remains a separate, explicit operation below.
    pub fn persist_effect_envelope(
        &self,
        principal: &AuthenticatedPrincipal,
        envelope: &EffectEnvelopeContract,
        residual_finding_sha256: &str,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_RISK_ACKNOWLEDGE)?;
        principal.require_scope(SCOPE_SPEND_AUTHORIZE)?;
        if matches!(principal.principal_kind(), PrincipalKind::FixturePrincipal) {
            return Err("fixture principal cannot persist a production effect envelope".into());
        }
        if envelope.owner_principal_id != principal.principal_id {
            return Err("effect envelope owner principal mismatch".into());
        }
        let now = self.now();
        envelope.validate(&now)?;
        let body = sort_value(&json!({
            "schema_version": EFFECT_ENVELOPE_SCHEMA_VERSION,
            "decision_id": envelope.decision_id,
            "status": "draft_pending_operator",
            "invalidation_state": "none",
            "acknowledgement": {
                "required_phrase": EFFECT_ENVELOPE_APPROVAL_PHRASE
            },
            "effect_envelope": envelope.body()
        }));
        self.upsert_managed_acceptance_decision(
            principal.tenant_id.as_str(),
            &body,
            residual_finding_sha256,
            "draft_pending_operator",
            Some(principal),
            Some(envelope.expires_at.as_str()),
        )
    }

    /// Owner approval for a parent effect envelope. This only creates the
    /// existing risk acknowledgement receipt; it grants no child or execution
    /// authority until a separate child derivation call succeeds.
    pub fn approve_effect_envelope(
        &self,
        principal: &AuthenticatedPrincipal,
        envelope: &EffectEnvelopeContract,
        residual_finding_sha256: &str,
    ) -> Result<Value, String> {
        let decision =
            self.persist_effect_envelope(principal, envelope, residual_finding_sha256)?;
        let decision_body_sha256 = decision
            .get("decision_body_sha256")
            .and_then(Value::as_str)
            .ok_or("effect envelope decision hash is missing")?;
        self.accept_managed_acceptance_decision(
            principal,
            &RiskAcknowledgementRequest {
                decision_id: envelope.decision_id.clone(),
                expected_decision_body_sha256: decision_body_sha256.to_string(),
                expected_residual_finding_sha256: residual_finding_sha256.to_string(),
                submitted_phrase: EFFECT_ENVELOPE_APPROVAL_PHRASE.to_string(),
                explicit_go: true,
            },
        )
    }

    /// Store-derived risk acceptance. Scope and expiry come from the persisted decision only.
    pub fn accept_managed_acceptance_decision(
        &self,
        principal: &AuthenticatedPrincipal,
        request: &RiskAcknowledgementRequest,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_RISK_ACKNOWLEDGE)?;
        if !request.explicit_go {
            return Err("explicit_go required".into());
        }
        if matches!(principal.principal_kind, PrincipalKind::FixturePrincipal) {
            // fixture dry-run only
        } else if !matches!(principal.principal_kind, PrincipalKind::OperatorApiKey) {
            return Err("principal cannot authorize managed acceptance".into());
        }
        if is_forbidden_principal_id(&principal.principal_id)
            && !matches!(principal.principal_kind, PrincipalKind::FixturePrincipal)
        {
            return Err("forbidden principal".into());
        }
        let now = self.now();

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let decision = load_decision_sqlite(&tx, &request.decision_id)?;
                let row = accept_on_sqlite(&tx, principal, request, &decision, &now)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!("maa:{}", request.decision_id)],
                )
                .map_err(|e| e.to_string())?;
                let decision = load_decision_pg(&mut tx, &request.decision_id)?;
                let row = accept_on_pg(&mut tx, principal, request, &decision, &now)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    /// Issue one-use spend authorization distinct from risk acknowledgement.
    pub fn issue_managed_acceptance_spend_authorization(
        &self,
        principal: &AuthenticatedPrincipal,
        request: &SpendAuthorizationRequest,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_SPEND_AUTHORIZE)?;
        let now = self.now();
        let fixture_only = matches!(principal.principal_kind, PrincipalKind::FixturePrincipal);
        if !fixture_only && !principal.may_authorize_production_live_start() {
            return Err("principal cannot issue production spend authorization".into());
        }

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let row = issue_spend_sqlite(&tx, principal, request, fixture_only, &now)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!(
                        "mas:{}:{}",
                        principal.tenant_id, request.risk_authorization_id
                    )],
                )
                .map_err(|e| e.to_string())?;
                let row = issue_spend_pg(&mut tx, principal, request, fixture_only, &now)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    /// Derive one bounded, one-use child authorization from an owner-approved
    /// parent effect envelope. This is a provider-free authority operation:
    /// it records a child receipt but never executes the effect it describes.
    pub fn derive_effect_child_authorization(
        &self,
        principal: &AuthenticatedPrincipal,
        parent_authorization_id: &str,
        request: &EffectChildAuthorizationRequest,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_DELEGATED_EXECUTE)?;
        principal.require_scope(SCOPE_ATTEMPT_ADMIT)?;
        if matches!(principal.principal_kind(), PrincipalKind::FixturePrincipal) {
            return Err("fixture principal cannot derive a production effect child".into());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let parent = load_authorization_sqlite(&tx, parent_authorization_id)?;
                let decision_id = required_str(&parent, "decision_id")?;
                let decision = load_decision_sqlite(&tx, &decision_id)?;
                let envelope = effect_parent_from_authority(&parent, &decision, principal, &now)?;
                let expires_at = request.validate_against(&envelope, &now)?;
                let body = effect_child_body(
                    principal,
                    &parent,
                    &decision,
                    &envelope,
                    request,
                    &expires_at,
                    &now,
                )?;
                let logical_authorization_sha256 = stable_spend_authorization_identity(&body)?;
                let mut body = body;
                body["logical_authorization_sha256"] =
                    json!(logical_authorization_sha256.clone());
                let body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());

                let mut statement = tx
                    .prepare(
                        "SELECT spend_authorization_id, status, body_json
                         FROM managed_acceptance_spend_authorizations
                         WHERE risk_authorization_id=?1 AND decision_id=?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = statement
                    .query_map(params![parent_authorization_id, decision_id], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    })
                    .map_err(|e| e.to_string())?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?;
                drop(statement);
                let mut child_count = 0_u64;
                let mut reserved_cost = 0.0_f64;
                for (existing_id, status, encoded) in rows {
                    let existing: Value = serde_json::from_str(&encoded)
                        .map_err(|e| format!("persisted effect child body is invalid: {e}"))?;
                    if existing.get("schema_version").and_then(Value::as_str)
                        != Some(EFFECT_CHILD_AUTHORIZATION_SCHEMA_VERSION)
                    {
                        continue;
                    }
                    if existing.get("parent_envelope_id").and_then(Value::as_str)
                        != Some(envelope.envelope_id.as_str())
                    {
                        continue;
                    }
                    child_count += 1;
                    let existing_cost = existing
                        .get("max_cost_usd")
                        .and_then(Value::as_f64)
                        .ok_or("persisted effect child max_cost_usd is missing")?;
                    if !existing_cost.is_finite() || existing_cost < 0.0 {
                        return Err("persisted effect child cost is invalid".into());
                    }
                    reserved_cost += existing_cost;
                    if existing.get("child_authorization_id").and_then(Value::as_str)
                        == Some(request.child_authorization_id.as_str())
                    {
                        if effect_child_replay_identity(&existing)?
                            != effect_child_replay_identity(&body)?
                        {
                            return Err("effect child id is bound to a conflicting request".into());
                        }
                        if status != "active" {
                            return Err("effect child is already consumed or terminal".into());
                        }
                        let spend = load_spend_sqlite(&tx, &existing_id)?;
                        let receipt = effect_child_receipt(&spend, &existing, true)?;
                        tx.commit().map_err(|e| e.to_string())?;
                        return Ok(receipt);
                    }
                }
                let unknown: Option<String> = tx
                    .query_row(
                        "SELECT a.attempt_id
                         FROM managed_acceptance_attempts a
                         JOIN managed_acceptance_spend_authorizations s
                           ON s.spend_authorization_id=a.spend_authorization_id
                         WHERE s.risk_authorization_id=?1 AND a.status='outcome_unknown'
                         LIMIT 1",
                        params![parent_authorization_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                if let Some(attempt_id) = unknown {
                    return Err(format!(
                        "effect parent has outcome_unknown attempt {attempt_id}; child derivation is not retryable"
                    ));
                }
                if child_count >= envelope.max_child_authorizations {
                    return Err("effect parent child-authorization limit exhausted".into());
                }
                if reserved_cost + request.max_cost_usd
                    > envelope.max_total_cost_usd + f64::EPSILON
                {
                    return Err("effect parent total budget would be exceeded".into());
                }
                let risk_authorization_sha256 = required_str(&parent, "authorization_sha256")?;
                let decision_body_sha256 = required_str(&decision, "decision_body_sha256")?;
                let residual_finding_sha256 = required_str(&decision, "residual_finding_sha256")?;
                tx.execute(
                    "INSERT INTO managed_acceptance_spend_authorizations (
                        spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
                        principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
                        logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                        fixture_only, status, body_json, created_at, updated_at, expires_at,
                        consumed_at, consumed_by_attempt_id, revoked_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,0,'active',?12,?13,?13,?14,NULL,NULL,NULL)",
                    params![
                        request.child_authorization_id,
                        decision_id,
                        parent_authorization_id,
                        principal.tenant_id,
                        principal.principal_kind.as_str(),
                        principal.principal_id,
                        body_sha256,
                        risk_authorization_sha256,
                        logical_authorization_sha256,
                        decision_body_sha256,
                        residual_finding_sha256,
                        body.to_string(),
                        now,
                        expires_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    &principal.principal_id,
                    "managed_acceptance.effect_child_derived",
                    &request.child_authorization_id,
                    &json!({
                        "parent_authorization_id": parent_authorization_id,
                        "parent_envelope_id": envelope.envelope_id,
                        "spend_body_sha256": body_sha256,
                        "logical_authorization_sha256": logical_authorization_sha256,
                        "effect_kind": request.effect_kind,
                        "target_repository": request.target_repository,
                        "target_main_sha": request.target_main_sha,
                        "max_cost_usd": request.max_cost_usd,
                        "expires_at": expires_at,
                        "outcome_unknown_retry": false,
                    }),
                )?;
                let spend = load_spend_sqlite(&tx, &request.child_authorization_id)?;
                let receipt = effect_child_receipt(&spend, &body, false)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(receipt)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.query_one(
                    "SELECT authorization_id FROM managed_acceptance_authorizations
                     WHERE authorization_id=$1 FOR UPDATE",
                    &[&parent_authorization_id],
                )
                .map_err(|e| e.to_string())?;
                let parent = load_authorization_pg(&mut tx, parent_authorization_id)?;
                let decision_id = required_str(&parent, "decision_id")?;
                let decision = load_decision_pg(&mut tx, &decision_id)?;
                let envelope = effect_parent_from_authority(&parent, &decision, principal, &now)?;
                let expires_at = request.validate_against(&envelope, &now)?;
                let body = effect_child_body(
                    principal,
                    &parent,
                    &decision,
                    &envelope,
                    request,
                    &expires_at,
                    &now,
                )?;
                let logical_authorization_sha256 = stable_spend_authorization_identity(&body)?;
                let mut body = body;
                body["logical_authorization_sha256"] =
                    json!(logical_authorization_sha256.clone());
                let body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
                let rows = tx
                    .query(
                        "SELECT spend_authorization_id, status, body_json
                         FROM managed_acceptance_spend_authorizations
                         WHERE risk_authorization_id=$1 AND decision_id=$2
                         FOR UPDATE",
                        &[&parent_authorization_id, &decision_id],
                    )
                    .map_err(|e| e.to_string())?;
                let mut child_count = 0_u64;
                let mut reserved_cost = 0.0_f64;
                for row in rows {
                    let existing_id: String = row.get(0);
                    let status: String = row.get(1);
                    let existing: Value = serde_json::from_str(&row.get::<_, String>(2))
                        .map_err(|e| format!("persisted effect child body is invalid: {e}"))?;
                    if existing.get("schema_version").and_then(Value::as_str)
                        != Some(EFFECT_CHILD_AUTHORIZATION_SCHEMA_VERSION)
                        || existing.get("parent_envelope_id").and_then(Value::as_str)
                            != Some(envelope.envelope_id.as_str())
                    {
                        continue;
                    }
                    child_count += 1;
                    let existing_cost = existing
                        .get("max_cost_usd")
                        .and_then(Value::as_f64)
                        .ok_or("persisted effect child max_cost_usd is missing")?;
                    if !existing_cost.is_finite() || existing_cost < 0.0 {
                        return Err("persisted effect child cost is invalid".into());
                    }
                    reserved_cost += existing_cost;
                    if existing.get("child_authorization_id").and_then(Value::as_str)
                        == Some(request.child_authorization_id.as_str())
                    {
                        if effect_child_replay_identity(&existing)?
                            != effect_child_replay_identity(&body)?
                        {
                            return Err("effect child id is bound to a conflicting request".into());
                        }
                        if status != "active" {
                            return Err("effect child is already consumed or terminal".into());
                        }
                        let spend = load_spend_pg(&mut tx, &existing_id)?;
                        let receipt = effect_child_receipt(&spend, &existing, true)?;
                        tx.commit().map_err(|e| e.to_string())?;
                        return Ok(receipt);
                    }
                }
                let unknown: Option<String> = tx
                    .query_opt(
                        "SELECT a.attempt_id
                         FROM managed_acceptance_attempts a
                         JOIN managed_acceptance_spend_authorizations s
                           ON s.spend_authorization_id=a.spend_authorization_id
                         WHERE s.risk_authorization_id=$1 AND a.status='outcome_unknown'
                         LIMIT 1",
                        &[&parent_authorization_id],
                    )
                    .map_err(|e| e.to_string())?
                    .map(|row| row.get(0));
                if let Some(attempt_id) = unknown {
                    return Err(format!(
                        "effect parent has outcome_unknown attempt {attempt_id}; child derivation is not retryable"
                    ));
                }
                if child_count >= envelope.max_child_authorizations {
                    return Err("effect parent child-authorization limit exhausted".into());
                }
                if reserved_cost + request.max_cost_usd
                    > envelope.max_total_cost_usd + f64::EPSILON
                {
                    return Err("effect parent total budget would be exceeded".into());
                }
                let risk_authorization_sha256 = required_str(&parent, "authorization_sha256")?;
                let decision_body_sha256 = required_str(&decision, "decision_body_sha256")?;
                let residual_finding_sha256 = required_str(&decision, "residual_finding_sha256")?;
                let zero: i32 = 0;
                let active = "active";
                tx.execute(
                    "INSERT INTO managed_acceptance_spend_authorizations (
                        spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
                        principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
                        logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                        fixture_only, status, body_json, created_at, updated_at, expires_at,
                        consumed_at, consumed_by_attempt_id, revoked_at
                     ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$15,$16,NULL,NULL,NULL)",
                    &[
                        &request.child_authorization_id,
                        &decision_id,
                        &parent_authorization_id,
                        &principal.tenant_id,
                        &principal.principal_kind.as_str(),
                        &principal.principal_id,
                        &body_sha256,
                        &risk_authorization_sha256,
                        &logical_authorization_sha256,
                        &decision_body_sha256,
                        &residual_finding_sha256,
                        &zero,
                        &active,
                        &body.to_string(),
                        &now,
                        &expires_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_effect_audit_pg(
                    &mut tx,
                    &now,
                    &principal.principal_id,
                    "managed_acceptance.effect_child_derived",
                    &request.child_authorization_id,
                    &json!({
                        "parent_authorization_id": parent_authorization_id,
                        "parent_envelope_id": envelope.envelope_id,
                        "spend_body_sha256": body_sha256,
                        "logical_authorization_sha256": logical_authorization_sha256,
                        "effect_kind": request.effect_kind,
                        "target_repository": request.target_repository,
                        "target_main_sha": request.target_main_sha,
                        "max_cost_usd": request.max_cost_usd,
                        "expires_at": expires_at,
                        "outcome_unknown_retry": false,
                    }),
                )?;
                let spend = load_spend_pg(&mut tx, &request.child_authorization_id)?;
                let receipt = effect_child_receipt(&spend, &body, false)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(receipt)
            }),
        }
    }

    /// Persist a terminal provider-free outcome for one derived effect child.
    /// This is an observation/settlement boundary only: it never invokes a
    /// provider or performs the external effect.  An unknown outcome closes
    /// the child permanently and therefore prevents all later derivation for
    /// the parent envelope.
    pub fn settle_effect_child_authorization(
        &self,
        principal: &AuthenticatedPrincipal,
        child_authorization_id: &str,
        attempt_id: &str,
        status: &str,
        evidence: &Value,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_DELEGATED_EXECUTE)?;
        principal.require_scope(SCOPE_ATTEMPT_ADMIT)?;
        if matches!(principal.principal_kind(), PrincipalKind::FixturePrincipal) {
            return Err("fixture principal cannot settle a production effect child".into());
        }
        if !matches!(status, "succeeded" | "failed" | EFFECT_OUTCOME_UNKNOWN) {
            return Err(
                "effect child outcome must be succeeded, failed, or outcome_unknown".into(),
            );
        }
        let evidence = validate_effect_outcome_evidence(evidence)?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let spend = load_spend_sqlite(&tx, child_authorization_id)?;
                let risk_authorization_id = required_str(&spend, "risk_authorization_id")?;
                let decision_id = required_str(&spend, "decision_id")?;
                let parent = load_authorization_sqlite(&tx, &risk_authorization_id)?;
                let decision = load_decision_sqlite(&tx, &decision_id)?;
                let (child, _envelope) = validate_effect_child_for_settlement(
                    &spend,
                    &parent,
                    &decision,
                    principal,
                )?;
                if evidence
                    .pointer("/evidence/child_authorization_id")
                    .and_then(Value::as_str)
                    != Some(child_authorization_id)
                {
                    return Err("effect outcome evidence child binding is invalid".into());
                }
                let receipt = effect_outcome_receipt(
                    &spend,
                    &child,
                    attempt_id,
                    status,
                    &evidence,
                )?;
                let receipt_sha256 = sha256_hex(canonical_json(&receipt)?.as_bytes());
                let terminal_class = effect_terminal_class(status);
                let attempt_body = effect_attempt_body(
                    &spend,
                    &child,
                    attempt_id,
                    status,
                    terminal_class,
                    &receipt_sha256,
                )?;
                let attempt_body_sha256 = sha256_hex(canonical_json(&attempt_body)?.as_bytes());

                if let Some((existing_status, existing_class, existing_receipt_sha, existing_spend)) =
                    tx.query_row(
                        "SELECT status, terminal_class, receipt_sha256, spend_authorization_id
                         FROM managed_acceptance_attempts
                         WHERE tenant_id=?1 AND attempt_id=?2",
                        params![principal.tenant_id, attempt_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, Option<String>>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, Option<String>>(3)?,
                            ))
                        },
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                {
                    if existing_spend.as_deref() != Some(child_authorization_id) {
                        return Err("attempt_id is already bound to another effect child".into());
                    }
                    if existing_status == status
                        && existing_class.as_deref() == Some(terminal_class)
                        && existing_receipt_sha.as_deref() == Some(receipt_sha256.as_str())
                    {
                        if spend.get("status").and_then(Value::as_str) != Some("consumed")
                            || spend.get("consumed_by_attempt_id").and_then(Value::as_str)
                                != Some(attempt_id)
                        {
                            return Err("effect child attempt replay has inconsistent spend state".into());
                        }
                        let mut row = load_attempt_sqlite(&tx, attempt_id)?;
                        if let Value::Object(object) = &mut row {
                            object.insert("idempotent_replay".into(), json!(true));
                        }
                        tx.commit().map_err(|e| e.to_string())?;
                        return Ok(row);
                    }
                    return Err("late or conflicting effect child settlement".into());
                }
                if spend.get("status").and_then(Value::as_str) != Some("active") {
                    return Err("effect child is not active and cannot be settled".into());
                }
                let updated = tx
                    .execute(
                        "UPDATE managed_acceptance_spend_authorizations
                         SET status='consumed', consumed_at=?1, consumed_by_attempt_id=?2, updated_at=?1
                         WHERE spend_authorization_id=?3 AND status='active'",
                        params![now, attempt_id, child_authorization_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("effect child was consumed or revoked concurrently".into());
                }
                let decision_id = required_str(&spend, "decision_id")?;
                let risk_authorization_id = required_str(&spend, "risk_authorization_id")?;
                let manifest_sha256 = required_str(&spend, "spend_body_sha256")?;
                let operation_id = required_str(&child, "operation_id")?;
                tx.execute(
                    "INSERT INTO managed_acceptance_attempts (
                        attempt_id, tenant_id, product_task_id, workflow_node_id, execution_id,
                        decision_id, authorization_id, spend_authorization_id, manifest_sha256,
                        attempt_body_sha256, status, terminal_class, body_json, receipt_json,
                        receipt_sha256, lease_token, created_at, updated_at
                     ) VALUES (?1,?2,NULL,NULL,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,NULL,?14,?14)",
                    params![
                        attempt_id,
                        principal.tenant_id,
                        operation_id,
                        decision_id,
                        risk_authorization_id,
                        child_authorization_id,
                        manifest_sha256,
                        attempt_body_sha256,
                        status,
                        terminal_class,
                        attempt_body.to_string(),
                        receipt.to_string(),
                        receipt_sha256,
                        now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    &principal.principal_id,
                    "managed_acceptance.effect_child_settled",
                    child_authorization_id,
                    &json!({
                        "attempt_id": attempt_id,
                        "status": status,
                        "terminal_class": terminal_class,
                        "receipt_sha256": receipt_sha256,
                        "outcome_unknown_retry": false,
                    }),
                )?;
                let row = load_attempt_sqlite(&tx, attempt_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let spend = load_spend_pg(&mut tx, child_authorization_id)?;
                let risk_authorization_id = required_str(&spend, "risk_authorization_id")?;
                let decision_id = required_str(&spend, "decision_id")?;
                let parent = load_authorization_pg(&mut tx, &risk_authorization_id)?;
                let decision = load_decision_pg(&mut tx, &decision_id)?;
                let (child, _envelope) = validate_effect_child_for_settlement(
                    &spend,
                    &parent,
                    &decision,
                    principal,
                )?;
                if evidence
                    .pointer("/evidence/child_authorization_id")
                    .and_then(Value::as_str)
                    != Some(child_authorization_id)
                {
                    return Err("effect outcome evidence child binding is invalid".into());
                }
                let receipt = effect_outcome_receipt(
                    &spend,
                    &child,
                    attempt_id,
                    status,
                    &evidence,
                )?;
                let receipt_sha256 = sha256_hex(canonical_json(&receipt)?.as_bytes());
                let terminal_class = effect_terminal_class(status);
                let attempt_body = effect_attempt_body(
                    &spend,
                    &child,
                    attempt_id,
                    status,
                    terminal_class,
                    &receipt_sha256,
                )?;
                let attempt_body_sha256 = sha256_hex(canonical_json(&attempt_body)?.as_bytes());
                if let Some(row) = tx
                    .query_opt(
                        "SELECT status, terminal_class, receipt_sha256, spend_authorization_id
                         FROM managed_acceptance_attempts
                         WHERE tenant_id=$1 AND attempt_id=$2 FOR UPDATE",
                        &[&principal.tenant_id, &attempt_id],
                    )
                    .map_err(|e| e.to_string())?
                {
                    let existing_status: String = row.get(0);
                    let existing_class: Option<String> = row.get(1);
                    let existing_receipt_sha: Option<String> = row.get(2);
                    let existing_spend: Option<String> = row.get(3);
                    if existing_spend.as_deref() != Some(child_authorization_id) {
                        return Err("attempt_id is already bound to another effect child".into());
                    }
                    if existing_status == status
                        && existing_class.as_deref() == Some(terminal_class)
                        && existing_receipt_sha.as_deref() == Some(receipt_sha256.as_str())
                    {
                        if spend.get("status").and_then(Value::as_str) != Some("consumed")
                            || spend.get("consumed_by_attempt_id").and_then(Value::as_str)
                                != Some(attempt_id)
                        {
                            return Err("effect child attempt replay has inconsistent spend state".into());
                        }
                        let mut row = load_attempt_pg(&mut tx, attempt_id)?;
                        if let Value::Object(object) = &mut row {
                            object.insert("idempotent_replay".into(), json!(true));
                        }
                        tx.commit().map_err(|e| e.to_string())?;
                        return Ok(row);
                    }
                    return Err("late or conflicting effect child settlement".into());
                }
                if spend.get("status").and_then(Value::as_str) != Some("active") {
                    return Err("effect child is not active and cannot be settled".into());
                }
                let updated = tx
                    .execute(
                        "UPDATE managed_acceptance_spend_authorizations
                         SET status='consumed', consumed_at=$1, consumed_by_attempt_id=$2, updated_at=$1
                         WHERE spend_authorization_id=$3 AND status='active'",
                        &[&now, &attempt_id, &child_authorization_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("effect child was consumed or revoked concurrently".into());
                }
                let decision_id = required_str(&spend, "decision_id")?;
                let risk_authorization_id = required_str(&spend, "risk_authorization_id")?;
                let manifest_sha256 = required_str(&spend, "spend_body_sha256")?;
                let operation_id = required_str(&child, "operation_id")?;
                let status_s = status.to_string();
                let terminal_class_s = terminal_class.to_string();
                let body_s = attempt_body.to_string();
                let receipt_s = receipt.to_string();
                tx.execute(
                    "INSERT INTO managed_acceptance_attempts (
                        attempt_id, tenant_id, product_task_id, workflow_node_id, execution_id,
                        decision_id, authorization_id, spend_authorization_id, manifest_sha256,
                        attempt_body_sha256, status, terminal_class, body_json, receipt_json,
                        receipt_sha256, lease_token, created_at, updated_at
                     ) VALUES ($1,$2,NULL,NULL,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,NULL,$14,$14)",
                    &[
                        &attempt_id,
                        &principal.tenant_id,
                        &operation_id,
                        &decision_id,
                        &risk_authorization_id,
                        &child_authorization_id,
                        &manifest_sha256,
                        &attempt_body_sha256,
                        &status_s,
                        &terminal_class_s,
                        &body_s,
                        &receipt_s,
                        &receipt_sha256,
                        &now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_effect_audit_pg(
                    &mut tx,
                    &now,
                    &principal.principal_id,
                    "managed_acceptance.effect_child_settled",
                    child_authorization_id,
                    &json!({
                        "attempt_id": attempt_id,
                        "status": status,
                        "terminal_class": terminal_class,
                        "receipt_sha256": receipt_sha256,
                        "outcome_unknown_retry": false,
                    }),
                )?;
                let row = load_attempt_pg(&mut tx, attempt_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    pub fn get_managed_acceptance_decision(
        &self,
        decision_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT decision_id FROM managed_acceptance_decisions WHERE decision_id=?1",
                    params![decision_id],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .map(|_| load_decision_sqlite(conn, decision_id))
                .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                if client
                    .query_opt(
                        "SELECT decision_id FROM managed_acceptance_decisions WHERE decision_id=$1",
                        &[&decision_id],
                    )
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    Ok(Some(load_decision_pg(client, decision_id)?))
                } else {
                    Ok(None)
                }
            }),
        }
    }

    /// Query durable, hash-linked lifecycle transitions for one immutable decision.
    /// This is receipt evidence only; it grants no authority.
    pub fn list_managed_acceptance_decision_transition_receipts(
        &self,
        decision_id: &str,
    ) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut statement = conn
                    .prepare(
                        "SELECT receipt_json FROM managed_acceptance_decision_transition_receipts
                         WHERE decision_id=?1
                         ORDER BY sequence ASC, transition_receipt_id ASC",
                    )
                    .map_err(|error| error.to_string())?;
                let rows = statement
                    .query_map(params![decision_id], |row| row.get::<_, String>(0))
                    .map_err(|error| error.to_string())?;
                let receipts = rows
                    .map(|row| {
                        let encoded = row.map_err(|error| error.to_string())?;
                        serde_json::from_str(&encoded).map_err(|_| {
                            "managed acceptance transition receipt is invalid JSON".to_string()
                        })
                    })
                    .collect::<Result<Vec<Value>, String>>()?;
                if receipts.is_empty() {
                    return Ok(receipts);
                }
                let decision = load_decision_sqlite(conn, decision_id)?;
                validate_managed_acceptance_decision_transition_chain(&decision, &receipts)?;
                Ok(receipts)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let receipts = client
                    .query(
                        "SELECT receipt_json FROM managed_acceptance_decision_transition_receipts
                         WHERE decision_id=$1
                         ORDER BY sequence ASC, transition_receipt_id ASC",
                        &[&decision_id],
                    )
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .map(|row| {
                        let encoded: String = row.get(0);
                        serde_json::from_str(&encoded).map_err(|_| {
                            "managed acceptance transition receipt is invalid JSON".to_string()
                        })
                    })
                    .collect::<Result<Vec<Value>, String>>()?;
                if receipts.is_empty() {
                    return Ok(receipts);
                }
                let decision = load_decision_pg(client, decision_id)?;
                validate_managed_acceptance_decision_transition_chain(&decision, &receipts)?;
                Ok(receipts)
            }),
        }
    }

    pub fn get_active_managed_acceptance_authorization(
        &self,
        authorization_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let row = conn
                    .query_row(
                        "SELECT status, expires_at FROM managed_acceptance_authorizations WHERE authorization_id=?1",
                        params![authorization_id],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                let Some((status, expires_at)) = row else {
                    return Ok(None);
                };
                if status != "active" {
                    return Ok(None);
                }
                let now = self.now();
                if is_at_or_before(&expires_at, &now)? {
                    return Ok(None);
                }
                Ok(Some(load_authorization_sqlite(conn, authorization_id)?))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let row = client
                    .query_opt(
                        "SELECT status, expires_at FROM managed_acceptance_authorizations WHERE authorization_id=$1",
                        &[&authorization_id],
                    )
                    .map_err(|e| e.to_string())?;
                let Some(row) = row else {
                    return Ok(None);
                };
                let status: String = row.get(0);
                let expires_at: String = row.get(1);
                if status != "active" || is_at_or_before(&expires_at, &self.now())? {
                    return Ok(None);
                }
                Ok(Some(load_authorization_pg(client, authorization_id)?))
            }),
        }
    }

    pub fn get_managed_acceptance_spend_authorization(
        &self,
        spend_authorization_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT spend_authorization_id FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=?1",
                    params![spend_authorization_id],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .map(|_| load_spend_sqlite(conn, spend_authorization_id))
                .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                if client
                    .query_opt(
                        "SELECT spend_authorization_id FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=$1",
                        &[&spend_authorization_id],
                    )
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    Ok(Some(load_spend_pg(client, spend_authorization_id)?))
                } else {
                    Ok(None)
                }
            }),
        }
    }

    pub fn revoke_managed_acceptance_authorization(
        &self,
        principal: &AuthenticatedPrincipal,
        authorization_id: &str,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_REVOKE)?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let auth = load_authorization_sqlite(&tx, authorization_id)?;
                if auth.get("principal_id").and_then(Value::as_str)
                    != Some(principal.principal_id.as_str())
                    && !principal.has_scope("team:admin")
                {
                    return Err("principal cannot revoke this authorization".into());
                }
                tx.execute(
                    "UPDATE managed_acceptance_authorizations SET status='revoked', revoked_at=?1, updated_at=?1 WHERE authorization_id=?2",
                    params![now, authorization_id],
                )
                .map_err(|e| e.to_string())?;
                // Also revoke unconsumed spend under this risk auth.
                tx.execute(
                    "UPDATE managed_acceptance_spend_authorizations SET status='revoked', revoked_at=?1, updated_at=?1 WHERE risk_authorization_id=?2 AND status='active'",
                    params![now, authorization_id],
                )
                .map_err(|e| e.to_string())?;
                let decision_id = auth
                    .get("decision_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let decision_before = load_decision_sqlite(&tx, decision_id)?;
                let from_status = decision_before
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                tx.execute(
                    "UPDATE managed_acceptance_decisions SET status='revoked', revoked_at=?1, updated_at=?1 WHERE decision_id=?2",
                    params![now, decision_id],
                )
                .map_err(|e| e.to_string())?;
                // Decision body hash stays immutable; record transition receipt separately.
                let transition = persist_transition_sqlite(
                    &tx,
                    &decision_before,
                    &from_status,
                    "revoked",
                    principal,
                    &now,
                    "risk_authorization_revoked",
                )?;
                append_audit_locked(
                    &tx,
                    &now,
                    &principal.principal_id,
                    "managed_acceptance.risk_auth_revoked",
                    authorization_id,
                    &json!({
                        "decision_id": decision_id,
                        "status_transition": transition,
                    }),
                )?;
                let row = load_authorization_sqlite(&tx, authorization_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let auth = load_authorization_pg(&mut tx, authorization_id)?;
                // Parity with SQLite: only owner principal or team:admin may revoke.
                if auth.get("principal_id").and_then(Value::as_str)
                    != Some(principal.principal_id.as_str())
                    && !principal.has_scope("team:admin")
                {
                    return Err("principal cannot revoke this authorization".into());
                }
                tx.execute(
                    "UPDATE managed_acceptance_authorizations SET status='revoked', revoked_at=$1, updated_at=$1 WHERE authorization_id=$2",
                    &[&now, &authorization_id],
                )
                .map_err(|e| e.to_string())?;
                tx.execute(
                    "UPDATE managed_acceptance_spend_authorizations SET status='revoked', revoked_at=$1, updated_at=$1 WHERE risk_authorization_id=$2 AND status='active'",
                    &[&now, &authorization_id],
                )
                .map_err(|e| e.to_string())?;
                let decision_id = auth
                    .get("decision_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let decision_before = load_decision_pg(&mut tx, &decision_id)?;
                let from_status = decision_before
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                tx.execute(
                    "UPDATE managed_acceptance_decisions SET status='revoked', revoked_at=$1, updated_at=$1 WHERE decision_id=$2",
                    &[&now, &decision_id],
                )
                .map_err(|e| e.to_string())?;
                let transition = persist_transition_pg(
                    &mut tx,
                    &decision_before,
                    &from_status,
                    "revoked",
                    principal,
                    &now,
                    "risk_authorization_revoked",
                )?;
                append_effect_audit_pg(
                    &mut tx,
                    &now,
                    &principal.principal_id,
                    "managed_acceptance.risk_auth_revoked",
                    authorization_id,
                    &json!({
                        "decision_id": decision_id,
                        "status_transition": transition,
                    }),
                )?;
                let row = load_authorization_pg(&mut tx, authorization_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    /// Exactly-once attempt admission. Consumes one-use spend authorization atomically.
    /// Risk acknowledgement alone never admits production attempts.
    pub fn admit_managed_acceptance_attempt(
        &self,
        principal: &AuthenticatedPrincipal,
        attempt_id: &str,
        attempt_body: &Value,
        spend_authorization_id: &str,
    ) -> Result<Value, String> {
        if matches!(principal.principal_kind(), PrincipalKind::FixturePrincipal) {
            return Err("fixture principal cannot use production admission API".into());
        }
        principal.require_scope(SCOPE_ATTEMPT_ADMIT)?;
        let row = self.admit_managed_acceptance_attempt_internal(
            principal,
            attempt_id,
            attempt_body,
            spend_authorization_id,
            false,
        )?;
        if let Value::Object(mut object) = row {
            object.remove("lease_token");
            return Ok(Value::Object(object));
        }
        Ok(row)
    }

    /// Crate-internal provider-free test seam. The lease is intentionally
    /// available only through this named fixture API; production/general reads
    /// are redacted and the managed executor receives it only as an opaque lease.
    #[doc(hidden)]
    pub(crate) fn admit_managed_acceptance_attempt_for_test(
        &self,
        principal: &AuthenticatedPrincipal,
        attempt_id: &str,
        attempt_body: &Value,
        spend_authorization_id: &str,
        allow_fixture_dry_run: bool,
    ) -> Result<Value, String> {
        self.admit_managed_acceptance_attempt_internal(
            principal,
            attempt_id,
            attempt_body,
            spend_authorization_id,
            allow_fixture_dry_run,
        )
    }

    /// PostgreSQL integration support is exposed only under its test feature.
    #[cfg(feature = "pg-tests")]
    #[doc(hidden)]
    pub fn admit_managed_acceptance_attempt_for_pg_tests(
        &self,
        principal: &AuthenticatedPrincipal,
        attempt_id: &str,
        attempt_body: &Value,
        spend_authorization_id: &str,
        allow_fixture_dry_run: bool,
    ) -> Result<Value, String> {
        self.admit_managed_acceptance_attempt_for_test(
            principal,
            attempt_id,
            attempt_body,
            spend_authorization_id,
            allow_fixture_dry_run,
        )
    }

    fn admit_managed_acceptance_attempt_internal(
        &self,
        principal: &AuthenticatedPrincipal,
        attempt_id: &str,
        attempt_body: &Value,
        spend_authorization_id: &str,
        allow_fixture_dry_run: bool,
    ) -> Result<Value, String> {
        let body = sort_value(attempt_body);
        let attempt_body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
        let now = self.now();

        let row = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let row = admit_sqlite(
                    &tx,
                    principal,
                    attempt_id,
                    &body,
                    &attempt_body_sha256,
                    spend_authorization_id,
                    allow_fixture_dry_run,
                    &now,
                )?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!("mat:{}:{}", principal.tenant_id, attempt_id)],
                )
                .map_err(|e| e.to_string())?;
                let row = admit_pg(
                    &mut tx,
                    principal,
                    attempt_id,
                    &body,
                    &attempt_body_sha256,
                    spend_authorization_id,
                    allow_fixture_dry_run,
                    &now,
                )?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }?;
        self.attach_attempt_lease_token(row, attempt_id)
    }

    fn attach_attempt_lease_token(
        &self,
        mut row: Value,
        attempt_id: &str,
    ) -> Result<Value, String> {
        let token = self.current_attempt_lease_token(attempt_id)?;
        if let Value::Object(ref mut object) = row {
            object.insert("lease_token".to_string(), json!(token));
        }
        Ok(row)
    }

    fn current_attempt_lease_token(&self, attempt_id: &str) -> Result<String, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT lease_token FROM managed_acceptance_attempts WHERE attempt_id=?1",
                    params![attempt_id],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|e| e.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT lease_token FROM managed_acceptance_attempts WHERE attempt_id=$1",
                        &[&attempt_id],
                    )
                    .map(|row| row.get::<_, String>(0))
                    .map_err(|e| e.to_string())
            }),
        }
    }

    /// Terminalize attempt; requires current lease_token. Exact terminal replay allowed.
    pub fn complete_managed_acceptance_attempt(
        &self,
        attempt_id: &str,
        lease_token: &str,
        status: &str,
        terminal_class: &str,
        receipt: &Value,
    ) -> Result<Value, String> {
        validate_attempt_terminal_status(status)?;
        let now = self.now();
        let receipt_sorted = sort_value(receipt);
        let receipt_sha256 = sha256_hex(canonical_json(&receipt_sorted)?.as_bytes());
        let receipt_s = receipt_sorted.to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let (current, current_lease, current_receipt_sha, current_class): (
                    String,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                ) = tx
                    .query_row(
                        "SELECT status, lease_token, receipt_sha256, terminal_class FROM managed_acceptance_attempts WHERE attempt_id=?1",
                        params![attempt_id],
                        |r| {
                            Ok((
                                r.get(0)?,
                                r.get(1)?,
                                r.get(2)?,
                                r.get(3)?,
                            ))
                        },
                    )
                    .map_err(|e| e.to_string())?;
                if current_lease.as_deref() != Some(lease_token) {
                    return Err("lease_token mismatch; ownership lost or cancelled".into());
                }
                if matches!(
                    current.as_str(),
                    "succeeded" | "failed" | "cancelled" | "outcome_unknown" | "replayed"
                ) {
                    if current == status
                        && current_class.as_deref() == Some(terminal_class)
                        && current_receipt_sha.as_deref() == Some(receipt_sha256.as_str())
                    {
                        return load_attempt_sqlite(&tx, attempt_id);
                    }
                    return Err("late terminal write after attempt already terminal".into());
                }
                tx.execute(
                    "UPDATE managed_acceptance_attempts SET status=?1, terminal_class=?2, receipt_json=?3, receipt_sha256=?4, updated_at=?5 WHERE attempt_id=?6",
                    params![status, terminal_class, receipt_s, receipt_sha256, now, attempt_id],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    "lease-owner",
                    "managed_acceptance.attempt_terminal",
                    attempt_id,
                    &json!({
                        "status": status,
                        "terminal_class": terminal_class,
                        "receipt_sha256": receipt_sha256,
                    }),
                )?;
                let row = load_attempt_sqlite(&tx, attempt_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let row = tx
                    .query_one(
                        "SELECT status, lease_token, receipt_sha256, terminal_class FROM managed_acceptance_attempts WHERE attempt_id=$1 FOR UPDATE",
                        &[&attempt_id],
                    )
                    .map_err(|e| e.to_string())?;
                let current: String = row.get(0);
                let current_lease: Option<String> = row.get(1);
                let current_receipt_sha: Option<String> = row.get(2);
                let current_class: Option<String> = row.get(3);
                if current_lease.as_deref() != Some(lease_token) {
                    return Err("lease_token mismatch; ownership lost or cancelled".into());
                }
                if matches!(
                    current.as_str(),
                    "succeeded" | "failed" | "cancelled" | "outcome_unknown" | "replayed"
                ) {
                    if current == status
                        && current_class.as_deref() == Some(terminal_class)
                        && current_receipt_sha.as_deref() == Some(receipt_sha256.as_str())
                    {
                        return load_attempt_pg(&mut tx, attempt_id);
                    }
                    return Err("late terminal write after attempt already terminal".into());
                }
                tx.execute(
                    "UPDATE managed_acceptance_attempts SET status=$1, terminal_class=$2, receipt_json=$3, receipt_sha256=$4, updated_at=$5 WHERE attempt_id=$6",
                    &[
                        &status,
                        &terminal_class,
                        &receipt_s,
                        &receipt_sha256,
                        &now,
                        &attempt_id,
                    ],
                )
                .map_err(|e| e.to_string())?;
                let row = load_attempt_pg(&mut tx, attempt_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    pub fn get_managed_acceptance_attempt(
        &self,
        attempt_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT attempt_id FROM managed_acceptance_attempts WHERE attempt_id=?1",
                    params![attempt_id],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .map(|_| load_attempt_sqlite(conn, attempt_id))
                .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                if client
                    .query_opt(
                        "SELECT attempt_id FROM managed_acceptance_attempts WHERE attempt_id=$1",
                        &[&attempt_id],
                    )
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    Ok(Some(load_attempt_pg(client, attempt_id)?))
                } else {
                    Ok(None)
                }
            }),
        }
    }

    /// Reload the exact runtime node from its two persistence owners.  This is
    /// intentionally narrower than the generic workflow-run projection: a
    /// corrupt unrelated edge/event must never be silently normalized while a
    /// production spawn is making its authority decision.
    fn load_managed_codex_runtime_node(
        &self,
        run_id: &str,
        node_id: &str,
    ) -> Result<(String, Value), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let (workflow_id, node_json): (String, String) = conn
                    .query_row(
                        "SELECT workflow_runs.workflow_id, workflow_run_nodes.node_json
                         FROM workflow_runs
                         JOIN workflow_run_nodes ON workflow_run_nodes.run_id=workflow_runs.run_id
                         WHERE workflow_runs.run_id=?1 AND workflow_run_nodes.node_id=?2",
                        params![run_id, node_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| {
                        "managed Codex workflow run/node owner is missing".to_string()
                    })?;
                let node: Value = serde_json::from_str(&node_json)
                    .map_err(|_| "managed Codex workflow node JSON is corrupt".to_string())?;
                if !node.is_object() {
                    return Err("managed Codex workflow node must be an object".to_string());
                }
                Ok((workflow_id, node))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let row = client
                    .query_opt(
                        "SELECT workflow_runs.workflow_id, workflow_run_nodes.node_json
                         FROM workflow_runs
                         JOIN workflow_run_nodes ON workflow_run_nodes.run_id=workflow_runs.run_id
                         WHERE workflow_runs.run_id=$1 AND workflow_run_nodes.node_id=$2",
                        &[&run_id, &node_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| {
                        "managed Codex workflow run/node owner is missing".to_string()
                    })?;
                let workflow_id: String = row.get(0);
                let node_json: String = row.get(1);
                let node: Value = serde_json::from_str(&node_json)
                    .map_err(|_| "managed Codex workflow node JSON is corrupt".to_string())?;
                if !node.is_object() {
                    return Err("managed Codex workflow node must be an object".to_string());
                }
                Ok((workflow_id, node))
            }),
        }
    }

    /// Persist the only scheduler-node binding that may later locate a managed
    /// Codex spend.  The binding is derived from existing store owners and is
    /// not an operator-supplied node-metadata assertion.
    pub fn bind_managed_codex_spend_to_product_node(
        &self,
        principal: &AuthenticatedPrincipal,
        spend_authorization_id: &str,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_ATTEMPT_ADMIT)?;
        let spend = self
            .get_managed_acceptance_spend_authorization(spend_authorization_id)?
            .ok_or_else(|| "managed Codex spend authorization not found".to_string())?;
        if spend.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id())
            || spend.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id())
            || spend.get("principal_kind").and_then(Value::as_str)
                != Some(principal.principal_kind().as_str())
        {
            return Err("managed Codex spend binding principal mismatch".to_string());
        }
        if spend.get("status").and_then(Value::as_str) != Some("active") {
            return Err("managed Codex spend binding requires an active spend".to_string());
        }
        let spend_body = spend
            .get("body_json")
            .cloned()
            .ok_or_else(|| "managed Codex spend body is missing".to_string())?;
        let task_id = required_str(&spend_body, "product_task_id")?;
        let workflow_id = required_str(&spend_body, "workflow_id")?;
        let node_id = required_str(&spend_body, "workflow_node_id")?;
        let execution_id = required_str(&spend_body, "execution_id")?;
        let attempt_id = required_str(&spend_body, "attempt_id")?;
        let task = self
            .get_product_task(&task_id)?
            .ok_or_else(|| "managed Codex ProductTask owner is missing".to_string())?;
        if task.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
            return Err("managed Codex ProductTask tenant mismatch".to_string());
        }
        let phase = self.validate_managed_acceptance_product_task_phase(
            principal.tenant_id(),
            &task_id,
            &required_str(&spend_body, "target_repo")?,
            &required_str(&spend_body, "target_main_sha")?,
        )?;
        require_managed_codex_pre_execution_phase(&phase)?;
        let run_id = required_str(&task, "run_id")?;
        let (persisted_workflow_id, node) =
            self.load_managed_codex_runtime_node(&run_id, &node_id)?;
        if persisted_workflow_id != workflow_id {
            return Err(
                "managed Codex spend workflow_id does not match ProductTask run".to_string(),
            );
        }
        validate_bindable_managed_codex_node(&node, &task_id, &node_id)?;
        validate_managed_codex_runtime_profile_node(&node, &spend_body)?;
        let binding = managed_codex_spawn_binding(
            &spend,
            &spend_body,
            &task,
            &run_id,
            &workflow_id,
            &node_id,
            &execution_id,
            &attempt_id,
        )?;
        let binding_sha256 = required_str(&binding, "binding_sha256")?;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let node_json: String = tx
                    .query_row(
                        "SELECT node_json FROM workflow_run_nodes WHERE run_id=?1 AND node_id=?2",
                        params![run_id, node_id],
                        |row| row.get(0),
                    )
                    .map_err(|_| "managed Codex workflow node disappeared during binding".to_string())?;
                let mut current: Value = serde_json::from_str(&node_json)
                    .map_err(|_| "managed Codex workflow node JSON is corrupt".to_string())?;
                validate_bindable_managed_codex_node(&current, &task_id, &node_id)?;
                if let Some(existing) = current.get("managed_codex_spawn_binding") {
                    if existing != &binding {
                        return Err("managed Codex node already has a conflicting spend binding".to_string());
                    }
                } else {
                    current
                        .as_object_mut()
                        .ok_or_else(|| "managed Codex workflow node must be an object".to_string())?
                        .insert("managed_codex_spawn_binding".to_string(), binding.clone());
                    let changed = tx
                        .execute(
                            "UPDATE workflow_run_nodes SET node_json=?1 WHERE run_id=?2 AND node_id=?3",
                            params![current.to_string(), run_id, node_id],
                        )
                        .map_err(|error| error.to_string())?;
                    if changed != 1 {
                        return Err("managed Codex workflow node binding lost ownership".to_string());
                    }
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(binding.clone())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let row = tx
                    .query_opt(
                        "SELECT node_json FROM workflow_run_nodes WHERE run_id=$1 AND node_id=$2 FOR UPDATE",
                        &[&run_id, &node_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "managed Codex workflow node disappeared during binding".to_string())?;
                let node_json: String = row.get(0);
                let mut current: Value = serde_json::from_str(&node_json)
                    .map_err(|_| "managed Codex workflow node JSON is corrupt".to_string())?;
                validate_bindable_managed_codex_node(&current, &task_id, &node_id)?;
                if let Some(existing) = current.get("managed_codex_spawn_binding") {
                    if existing != &binding {
                        return Err("managed Codex node already has a conflicting spend binding".to_string());
                    }
                } else {
                    current
                        .as_object_mut()
                        .ok_or_else(|| "managed Codex workflow node must be an object".to_string())?
                        .insert("managed_codex_spawn_binding".to_string(), binding.clone());
                    let changed = tx
                        .execute(
                            "UPDATE workflow_run_nodes SET node_json=$1 WHERE run_id=$2 AND node_id=$3",
                            &[&current.to_string(), &run_id, &node_id],
                        )
                        .map_err(|error| error.to_string())?;
                    if changed != 1 {
                        return Err("managed Codex workflow node binding lost ownership".to_string());
                    }
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(binding.clone())
            }),
        }
        .and_then(|bound| {
            self.append_audit(
                principal.principal_id(),
                "managed_acceptance.codex_spawn_bound",
                &run_id,
                &json!({
                    "node_id": node_id,
                    "spend_authorization_id": spend_authorization_id,
                    "binding_sha256": binding_sha256,
                    "content_excluded": true,
                }),
            )?;
            Ok(bound)
        })
    }

    /// Production admission for the managed Codex spawn boundary.  This path
    /// never accepts fixture authority.
    pub fn admit_managed_codex_spawn(
        &self,
        facts: &ManagedCodexLaunchFacts,
    ) -> Result<ManagedCodexSpawnLease, String> {
        self.admit_managed_codex_spawn_inner(facts, false)
    }

    /// Provider-free test seam.  Production callers must use
    /// [`Self::admit_managed_codex_spawn`], which rejects fixture principals.
    #[cfg(any(test, feature = "pg-tests"))]
    #[doc(hidden)]
    pub fn admit_managed_codex_spawn_for_test(
        &self,
        facts: &ManagedCodexLaunchFacts,
    ) -> Result<ManagedCodexSpawnLease, String> {
        self.admit_managed_codex_spawn_inner(facts, true)
    }

    fn admit_managed_codex_spawn_inner(
        &self,
        facts: &ManagedCodexLaunchFacts,
        allow_fixture_dry_run: bool,
    ) -> Result<ManagedCodexSpawnLease, String> {
        let prepared = self.prepare_managed_codex_spawn(
            facts,
            allow_fixture_dry_run,
            ManagedCodexSpawnSpendState::ActiveForLeaseAdmission,
        )?;
        let attempt = self.admit_managed_acceptance_attempt_internal(
            &prepared.principal,
            &prepared.attempt_id,
            &prepared.attempt_body,
            &prepared.spend_authorization_id,
            allow_fixture_dry_run,
        )?;
        if attempt.get("idempotent_replay").and_then(Value::as_bool) == Some(true) {
            return Err(
                "managed Codex attempt already has a lease; generic replay cannot start or expose it"
                    .to_string(),
            );
        }
        let lease_token = attempt
            .get("lease_token")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "managed Codex admission did not create an attempt lease".to_string())?
            .to_string();
        Ok(ManagedCodexSpawnLease {
            facts: facts.clone(),
            product_task_id: prepared.product_task_id,
            product_task_version: prepared.product_task_version,
            tenant_id: prepared.principal.tenant_id().to_string(),
            principal_id: prepared.principal.principal_id().to_string(),
            principal_kind: prepared.principal.principal_kind().clone(),
            spend_authorization_id: prepared.spend_authorization_id,
            attempt_id: prepared.attempt_id,
            execution_id: prepared.execution_id,
            lease_token,
            spend_body: prepared.spend_body,
        })
    }

    /// Read-only validation used by the runtime owner-derived preflight. It
    /// proves that the exact store-issued lease still owns the consumed spend
    /// immediately before a child could be spawned; it creates no new lease,
    /// does not change spend state, and does not grant any output authority.
    pub(crate) fn validate_managed_codex_preflight_lease(
        &self,
        lease: &ManagedCodexSpawnLease,
    ) -> Result<(), String> {
        self.assert_managed_codex_spawn_lease_current(lease)
    }

    /// Revalidate the current store owners and the actual launcher/gateway/
    /// journal attestation immediately before the child `Command::spawn`.
    pub fn confirm_managed_codex_spawn_before_child(
        &self,
        lease: &ManagedCodexSpawnLease,
        runtime: &crate::cli::codex_mediation_admission::ManagedCodexRuntimeAttestation,
    ) -> Result<(), String> {
        if !crate::product_golden_path::product_gate_enabled() {
            return Err("product golden path execution gate is disabled".to_string());
        }
        if crate::product_golden_path::product_scheduler_kill_active() {
            return Err("product golden path scheduler kill switch is active".to_string());
        }
        runtime.assert_required_mediation_owners()?;
        // Host-network sharing is a real runtime fact.  It is a residual
        // blocker, never a URL/string inference that can be waived by a node.
        if !runtime.network_confinement_enforced() {
            return Err(
                "managed Codex runtime network confinement is not proved by the launcher owner"
                    .to_string(),
            );
        }
        self.assert_managed_codex_spawn_lease_current(lease)?;
        let prepared = self.prepare_managed_codex_spawn(
            &lease.facts,
            false,
            ManagedCodexSpawnSpendState::ConsumedCurrentLease(lease),
        )?;
        if prepared.product_task_id != lease.product_task_id
            || prepared.product_task_version != lease.product_task_version
            || prepared.spend_authorization_id != lease.spend_authorization_id
            || prepared.attempt_id != lease.attempt_id
            || prepared.execution_id != lease.execution_id
            || prepared.principal.tenant_id() != lease.tenant_id
            || prepared.principal.principal_id() != lease.principal_id
            || prepared.principal.principal_kind() != &lease.principal_kind
        {
            return Err("managed Codex launch owners changed after lease admission".to_string());
        }
        Ok(())
    }

    /// Persist every process terminal state under the current attempt lease.
    /// A consumed spend is intentionally never returned to active state.
    pub fn terminalize_managed_codex_spawn(
        &self,
        lease: &ManagedCodexSpawnLease,
        status: &str,
        terminal_class: &str,
        receipt: &Value,
    ) -> Result<Value, String> {
        self.assert_managed_codex_spawn_lease_current(lease)?;
        self.complete_managed_acceptance_attempt(
            &lease.attempt_id,
            &lease.lease_token,
            status,
            terminal_class,
            receipt,
        )
    }

    fn assert_managed_codex_spawn_lease_current(
        &self,
        lease: &ManagedCodexSpawnLease,
    ) -> Result<(), String> {
        let attempt = self
            .get_managed_acceptance_attempt(&lease.attempt_id)?
            .ok_or_else(|| "managed Codex attempt lease owner is missing".to_string())?;
        if attempt.get("tenant_id").and_then(Value::as_str) != Some(lease.tenant_id.as_str())
            || attempt.get("product_task_id").and_then(Value::as_str)
                != Some(lease.product_task_id.as_str())
            || attempt.get("execution_id").and_then(Value::as_str)
                != Some(lease.execution_id.as_str())
            || attempt
                .get("spend_authorization_id")
                .and_then(Value::as_str)
                != Some(lease.spend_authorization_id.as_str())
            || attempt.get("status").and_then(Value::as_str) != Some("admitted")
        {
            return Err(
                "managed Codex attempt lease is stale, terminal, or owned by another caller"
                    .to_string(),
            );
        }
        if self.current_attempt_lease_token(&lease.attempt_id)? != lease.lease_token {
            return Err(
                "managed Codex attempt lease is stale, terminal, or owned by another caller"
                    .to_string(),
            );
        }
        let spend = self
            .get_managed_acceptance_spend_authorization(&lease.spend_authorization_id)?
            .ok_or_else(|| "managed Codex spend owner is missing".to_string())?;
        if spend.get("status").and_then(Value::as_str) != Some("consumed")
            || spend.get("consumed_by_attempt_id").and_then(Value::as_str)
                != Some(lease.attempt_id.as_str())
        {
            return Err("managed Codex spend is not consumed by the current lease".to_string());
        }
        Ok(())
    }

    fn prepare_managed_codex_spawn(
        &self,
        facts: &ManagedCodexLaunchFacts,
        allow_fixture_dry_run: bool,
        spend_state: ManagedCodexSpawnSpendState<'_>,
    ) -> Result<PreparedManagedCodexSpawn, String> {
        validate_managed_codex_launch_facts(facts)?;
        let (workflow_id, node) =
            self.load_managed_codex_runtime_node(&facts.run_id, &facts.node_id)?;
        if workflow_id != facts.workflow_id {
            return Err(
                "managed Codex runtime workflow_id does not match current run owner".to_string(),
            );
        }
        let binding = node
            .get("managed_codex_spawn_binding")
            .cloned()
            .ok_or_else(|| "managed Codex store-owned node spend binding is missing".to_string())?;
        let spend_authorization_id = required_str(&binding, "spend_authorization_id")?;
        let spend = self
            .get_managed_acceptance_spend_authorization(&spend_authorization_id)?
            .ok_or_else(|| "managed Codex bound spend owner is missing".to_string())?;
        match spend_state {
            ManagedCodexSpawnSpendState::ActiveForLeaseAdmission => {
                if spend.get("status").and_then(Value::as_str) != Some("active") {
                    return Err(
                        "managed Codex bound spend is not active for lease admission".to_string(),
                    );
                }
            }
            ManagedCodexSpawnSpendState::ConsumedCurrentLease(lease) => {
                self.assert_managed_codex_spawn_lease_current(lease)?;
                if lease.spend_authorization_id != spend_authorization_id
                    || spend.get("status").and_then(Value::as_str) != Some("consumed")
                    || spend.get("consumed_by_attempt_id").and_then(Value::as_str)
                        != Some(lease.attempt_id.as_str())
                {
                    return Err(
                        "managed Codex consumed spend does not belong to the current lease"
                            .to_string(),
                    );
                }
            }
        }
        let spend_body = spend
            .get("body_json")
            .cloned()
            .ok_or_else(|| "managed Codex bound spend body is missing".to_string())?;
        validate_managed_codex_spawn_binding(
            &binding,
            &spend,
            &spend_body,
            &node,
            &facts.run_id,
            facts,
        )?;
        let tenant_id = required_str(&spend, "tenant_id")?;
        let principal_id = required_str(&spend, "principal_id")?;
        let principal_kind = PrincipalKind::parse(&required_str(&spend, "principal_kind")?)?;
        let is_fixture_principal = principal_kind == PrincipalKind::FixturePrincipal;
        let principal = match &principal_kind {
            PrincipalKind::OperatorApiKey => {
                let authenticated = self.authenticate_managed_acceptance_principal(
                    &tenant_id,
                    &principal_id,
                    None,
                )?;
                if authenticated.principal_kind() != &PrincipalKind::OperatorApiKey {
                    return Err(
                        "managed Codex principal kind changed after spend issuance".to_string()
                    );
                }
                authenticated
            }
            PrincipalKind::FixturePrincipal if allow_fixture_dry_run => {
                AuthenticatedPrincipal::fixture_for_dry_run(&tenant_id, &principal_id)?
            }
            PrincipalKind::FixturePrincipal => {
                return Err(
                    "fixture principal cannot admit a production managed Codex spawn".to_string(),
                )
            }
        };
        if spend.get("fixture_only").and_then(Value::as_bool)
            != Some(matches!(
                principal.principal_kind(),
                &PrincipalKind::FixturePrincipal
            ))
        {
            return Err(
                "managed Codex spend fixture_only boolean is missing or inconsistent".to_string(),
            );
        }
        let logical_authorization_sha256 = required_str(&spend, "logical_authorization_sha256")?;
        if stable_spend_authorization_identity(&spend_body)? != logical_authorization_sha256 {
            return Err("managed Codex spend logical authorization hash is stale".to_string());
        }
        let risk_id = required_str(&spend, "risk_authorization_id")?;
        let risk = self
            .get_active_managed_acceptance_authorization(&risk_id)?
            .ok_or_else(|| "managed Codex active risk authorization is missing".to_string())?;
        validate_spend_risk_owner(&spend, &risk)?;
        if risk.get("tenant_id").and_then(Value::as_str) != Some(tenant_id.as_str())
            || risk.get("principal_id").and_then(Value::as_str) != Some(principal_id.as_str())
            || risk.get("principal_kind").and_then(Value::as_str) != Some(principal_kind.as_str())
            || risk.get("execution_granted").and_then(Value::as_bool) != Some(false)
            || risk
                .pointer("/body_json/fixture_only")
                .and_then(Value::as_bool)
                != Some(is_fixture_principal)
        {
            return Err("managed Codex risk authorization is stale or lacks persisted false/fixture booleans".to_string());
        }
        let decision_id = required_str(&spend, "decision_id")?;
        let decision = self
            .get_managed_acceptance_decision(&decision_id)?
            .ok_or_else(|| "managed Codex decision owner is missing".to_string())?;
        validate_risk_decision_owner(&risk, &decision)?;
        if decision.get("tenant_id").and_then(Value::as_str) != Some(tenant_id.as_str())
            || decision.get("principal_id").and_then(Value::as_str) != Some(principal_id.as_str())
            || decision.get("principal_kind").and_then(Value::as_str)
                != Some(principal_kind.as_str())
            || decision.get("status").and_then(Value::as_str) != Some("operator_accepted")
            || decision.get("decision_body_sha256").and_then(Value::as_str)
                != spend.get("decision_body_sha256").and_then(Value::as_str)
        {
            return Err("managed Codex decision is stale, unaccepted, or mismatched".to_string());
        }
        let transitions =
            self.list_managed_acceptance_decision_transition_receipts(&decision_id)?;
        validate_managed_acceptance_decision_transition_chain(&decision, &transitions)?;
        let product_task_id = required_str(&spend_body, "product_task_id")?;
        let target_repo = required_str(&spend_body, "target_repo")?;
        let target_main_sha = required_str(&spend_body, "target_main_sha")?;
        let phase = self.validate_managed_acceptance_product_task_phase(
            &tenant_id,
            &product_task_id,
            &target_repo,
            &target_main_sha,
        )?;
        require_managed_codex_pre_execution_phase(&phase)?;
        let task = phase
            .get("task")
            .cloned()
            .ok_or_else(|| "managed Codex ProductTask reload failed".to_string())?;
        let task_version = task
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "managed Codex ProductTask version is missing".to_string())?;
        if binding.get("product_task_version").and_then(Value::as_u64) != Some(task_version) {
            return Err(
                "managed Codex spawn binding is stale for the current ProductTask version"
                    .to_string(),
            );
        }
        validate_managed_codex_task_budget(&task, &spend_body)?;
        let task_run_id = required_str(&task, "run_id")?;
        if task_run_id != facts.run_id {
            return Err("managed Codex ProductTask run owner changed".to_string());
        }
        let workspace_record_id = required_str(&task, "workspace_record_id")?;
        let workspace = self
            .managed_acceptance_workspace_owner(&workspace_record_id)
            .map_err(|error| format!("managed Codex workspace owner is unreadable: {error}"))?;
        let workspace_path = workspace
            .get("workspace_path")
            .or_else(|| task.pointer("/workspace_binding/workspace_path"))
            .and_then(Value::as_str)
            .ok_or_else(|| "managed Codex workspace path owner is missing".to_string())?;
        let canonical_workspace = std::fs::canonicalize(workspace_path)
            .map_err(|error| format!("managed Codex workspace owner is unreadable: {error}"))?;
        if workspace.get("workspace_id").and_then(Value::as_str)
            != Some(workspace_record_id.as_str())
            || workspace.get("run_id").and_then(Value::as_str) != Some(facts.run_id.as_str())
            || workspace.get("target_id").and_then(Value::as_str) != Some(target_repo.as_str())
            || canonical_workspace != facts.workspace_path
            || workspace.get("source_revision").and_then(Value::as_str)
                != Some(target_main_sha.as_str())
        {
            return Err("managed Codex current workspace/source binding is stale".to_string());
        }
        validate_managed_codex_runtime_identity(&spend_body, facts)?;
        let attempt_id = required_str(&spend_body, "attempt_id")?;
        let execution_id = required_str(&spend_body, "execution_id")?;
        let attempt_body = managed_codex_attempt_body(&spend_body)?;
        Ok(PreparedManagedCodexSpawn {
            principal,
            product_task_id,
            product_task_version: task_version,
            spend_authorization_id,
            attempt_id,
            execution_id,
            spend_body,
            attempt_body,
        })
    }
}

/// The only two spend states permitted at the two store-owned spawn seams.
/// Admission may consume an active authorization once; final pre-child
/// confirmation may only revalidate that same consumed authorization under its
/// exact current lease. No caller may turn a consumed spend back into active.
enum ManagedCodexSpawnSpendState<'a> {
    ActiveForLeaseAdmission,
    ConsumedCurrentLease(&'a ManagedCodexSpawnLease),
}

struct PreparedManagedCodexSpawn {
    principal: AuthenticatedPrincipal,
    product_task_id: String,
    product_task_version: u64,
    spend_authorization_id: String,
    attempt_id: String,
    execution_id: String,
    spend_body: Value,
    attempt_body: Value,
}

/// Receipt validation remains available for verification, approval, output,
/// and terminal states, but a new managed-Codex child may only be admitted
/// before the first live execution.  This separates an auditable read of an
/// advanced state from authority to consume a fresh spend and spawn a process.
fn require_managed_codex_pre_execution_phase(phase: &Value) -> Result<(), String> {
    if phase.get("stage").and_then(Value::as_str) != Some("pre_execution_admission") {
        return Err(
            "managed Codex live spawn requires ProductTask pre_execution_admission phase"
                .to_string(),
        );
    }
    Ok(())
}

/// Construct the durable scheduler-node binding from store-owned records.  The
/// node never supplies the spend identity: it merely receives the resulting
/// immutable receipt for the later executor/store rendezvous.
fn managed_codex_spawn_binding(
    spend: &Value,
    spend_body: &Value,
    task: &Value,
    run_id: &str,
    workflow_id: &str,
    node_id: &str,
    execution_id: &str,
    attempt_id: &str,
) -> Result<Value, String> {
    let product_task_id = required_str(task, "task_id")?;
    let task_version = task
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| "managed Codex ProductTask version is missing".to_string())?;
    let spend_authorization_id = required_str(spend, "spend_authorization_id")?;
    let spend_body_sha256 = required_str(spend, "spend_body_sha256")?;
    let logical_authorization_sha256 = required_str(spend, "logical_authorization_sha256")?;
    let tenant_id = required_str(spend, "tenant_id")?;
    let principal_id = required_str(spend, "principal_id")?;
    let principal_kind = required_str(spend, "principal_kind")?;

    for (field, expected) in [
        ("product_task_id", product_task_id.as_str()),
        ("workflow_id", workflow_id),
        ("workflow_node_id", node_id),
        ("execution_id", execution_id),
        ("attempt_id", attempt_id),
    ] {
        if spend_body.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "managed Codex spend {field} does not match the store-owned node binding"
            ));
        }
    }

    let mut binding = sort_value(&json!({
        "schema_version": "managed_codex_spawn_binding.v1",
        "spend_authorization_id": spend_authorization_id,
        "spend_body_sha256": spend_body_sha256,
        "logical_authorization_sha256": logical_authorization_sha256,
        "tenant_id": tenant_id,
        "principal_id": principal_id,
        "principal_kind": principal_kind,
        "product_task_id": product_task_id,
        "product_task_version": task_version,
        "run_id": run_id,
        "workflow_id": workflow_id,
        "node_id": node_id,
        "execution_id": execution_id,
        "attempt_id": attempt_id,
        "target_repo": required_str(spend_body, "target_repo")?,
        "target_main_sha": required_str(spend_body, "target_main_sha")?,
        "runtime_profile_sha256": spend_body.get("runtime_profile_sha256").cloned().unwrap_or(Value::Null),
        "capability_probe_sha256": spend_body.get("capability_probe_sha256").cloned().unwrap_or(Value::Null),
    }));
    let binding_sha256 = managed_codex_spawn_binding_sha256(&binding)?;
    binding
        .as_object_mut()
        .expect("binding is always an object")
        .insert("binding_sha256".to_string(), Value::String(binding_sha256));
    Ok(binding)
}

fn managed_codex_spawn_binding_sha256(binding: &Value) -> Result<String, String> {
    let mut body = sort_value(binding);
    let object = body
        .as_object_mut()
        .ok_or_else(|| "managed Codex spawn binding must be an object".to_string())?;
    object.remove("binding_sha256");
    Ok(sha256_hex(canonical_json(&body)?.as_bytes()))
}

/// A workflow node is a routing record, never evidence for a security boolean.
/// This check only proves it is the persisted ProductTask apply node to which a
/// separately derived store receipt can be attached.
fn validate_bindable_managed_codex_node(
    node: &Value,
    product_task_id: &str,
    node_id: &str,
) -> Result<(), String> {
    if !node.is_object()
        || node.get("node_id").and_then(Value::as_str) != Some(node_id)
        || node.get("product_task_id").and_then(Value::as_str) != Some(product_task_id)
        || node
            .pointer("/managed_supervised_patch/operation")
            .and_then(Value::as_str)
            != Some("product_apply")
        || node.get("executor").and_then(Value::as_str) != Some("codex_cli")
    {
        return Err(
            "managed Codex workflow node is not the exact persisted ProductTask apply owner"
                .to_string(),
        );
    }
    Ok(())
}

/// Validate facts sampled by `CliNodeExecutor` from the actual filesystem and
/// launch configuration before any gateway or child process is started.
fn validate_managed_codex_launch_facts(facts: &ManagedCodexLaunchFacts) -> Result<(), String> {
    for (name, value) in [
        ("run_id", facts.run_id.as_str()),
        ("workflow_id", facts.workflow_id.as_str()),
        ("node_id", facts.node_id.as_str()),
        ("executable_version", facts.executable_version.as_str()),
        ("model", facts.model.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(format!("managed Codex runtime {name} is missing"));
        }
    }
    if facts.executable_sha256.len() != 64
        || !facts
            .executable_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("managed Codex runtime executable SHA-256 is invalid".to_string());
    }
    for (name, value) in [
        (
            "runtime_profile_sha256",
            facts.runtime_profile_sha256.as_deref(),
        ),
        (
            "capability_probe_sha256",
            facts.capability_probe_sha256.as_deref(),
        ),
    ] {
        if let Some(value) = value {
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(format!("managed Codex runtime {name} is invalid"));
            }
        }
    }
    let canonical_workspace = std::fs::canonicalize(&facts.workspace_path)
        .map_err(|error| format!("managed Codex runtime workspace is unreadable: {error}"))?;
    if canonical_workspace != facts.workspace_path || !canonical_workspace.is_dir() {
        return Err("managed Codex runtime workspace must be a canonical directory".to_string());
    }
    let canonical_executable = std::fs::canonicalize(&facts.executable_path)
        .map_err(|error| format!("managed Codex runtime executable is unreadable: {error}"))?;
    if canonical_executable != facts.executable_path || !canonical_executable.is_file() {
        return Err("managed Codex runtime executable must be a canonical file".to_string());
    }
    let observed_sha256 =
        sha256_hex(&std::fs::read(&canonical_executable).map_err(|error| {
            format!("managed Codex runtime executable cannot be read: {error}")
        })?);
    if observed_sha256 != facts.executable_sha256.to_ascii_lowercase() {
        return Err("managed Codex runtime executable SHA-256 changed".to_string());
    }
    Ok(())
}

/// Verify that the persisted binding is self-authenticating and still points to
/// the exact current spend and scheduler node.  A mismatch is not repaired at
/// admission time; it is a fail-closed stale-owner condition.
fn validate_managed_codex_spawn_binding(
    binding: &Value,
    spend: &Value,
    spend_body: &Value,
    node: &Value,
    run_id: &str,
    facts: &ManagedCodexLaunchFacts,
) -> Result<(), String> {
    if binding.get("schema_version").and_then(Value::as_str)
        != Some("managed_codex_spawn_binding.v1")
    {
        return Err("managed Codex spawn binding schema is unsupported".to_string());
    }
    let declared_sha256 = required_str(binding, "binding_sha256")?;
    if declared_sha256 != managed_codex_spawn_binding_sha256(binding)? {
        return Err("managed Codex spawn binding hash is stale".to_string());
    }
    let expected = [
        (
            "spend_authorization_id",
            required_str(spend, "spend_authorization_id")?,
        ),
        (
            "spend_body_sha256",
            required_str(spend, "spend_body_sha256")?,
        ),
        (
            "logical_authorization_sha256",
            required_str(spend, "logical_authorization_sha256")?,
        ),
        ("tenant_id", required_str(spend, "tenant_id")?),
        ("principal_id", required_str(spend, "principal_id")?),
        ("principal_kind", required_str(spend, "principal_kind")?),
        (
            "product_task_id",
            required_str(spend_body, "product_task_id")?,
        ),
        ("workflow_id", facts.workflow_id.clone()),
        ("node_id", facts.node_id.clone()),
        ("run_id", run_id.to_string()),
        ("execution_id", required_str(spend_body, "execution_id")?),
        ("attempt_id", required_str(spend_body, "attempt_id")?),
        ("target_repo", required_str(spend_body, "target_repo")?),
        (
            "target_main_sha",
            required_str(spend_body, "target_main_sha")?,
        ),
    ];
    for (field, expected) in expected {
        if binding.get(field).and_then(Value::as_str) != Some(expected.as_str()) {
            return Err(format!("managed Codex spawn binding {field} is stale"));
        }
    }
    for (field, observed) in [
        (
            "runtime_profile_sha256",
            facts.runtime_profile_sha256.as_deref(),
        ),
        (
            "capability_probe_sha256",
            facts.capability_probe_sha256.as_deref(),
        ),
    ] {
        let bound = binding.get(field).and_then(Value::as_str);
        if bound != observed {
            return Err(format!("managed Codex spawn binding {field} is stale"));
        }
    }
    let task_id = required_str(spend_body, "product_task_id")?;
    validate_bindable_managed_codex_node(node, &task_id, &facts.node_id)
}

/// Compare the launcher-observed executable/model identity to the spend body.
/// Provider and budget identity are read exclusively from the spend body and
/// later used to construct the gateway, never from environment declarations.
fn validate_managed_codex_runtime_identity(
    spend_body: &Value,
    facts: &ManagedCodexLaunchFacts,
) -> Result<(), String> {
    if spend_body.get("workflow_id").and_then(Value::as_str) != Some(facts.workflow_id.as_str())
        || spend_body.get("workflow_node_id").and_then(Value::as_str)
            != Some(facts.node_id.as_str())
        || spend_body.get("binary_version").and_then(Value::as_str)
            != Some(facts.executable_version.as_str())
        || spend_body.get("binary_sha256").and_then(Value::as_str)
            != Some(facts.executable_sha256.as_str())
        || spend_body.get("model").and_then(Value::as_str) != Some(facts.model.as_str())
        || spend_body
            .get("runtime_profile_sha256")
            .and_then(Value::as_str)
            != facts.runtime_profile_sha256.as_deref()
        || spend_body
            .get("capability_probe_sha256")
            .and_then(Value::as_str)
            != facts.capability_probe_sha256.as_deref()
    {
        return Err("managed Codex runtime identity does not match bound spend".to_string());
    }
    let bound_binary = required_str(spend_body, "binary_path")?;
    let canonical_bound_binary = std::fs::canonicalize(bound_binary)
        .map_err(|error| format!("managed Codex bound binary is unreadable: {error}"))?;
    if canonical_bound_binary != facts.executable_path {
        return Err("managed Codex runtime binary path does not match bound spend".to_string());
    }
    for field in [
        "provider_kind",
        "provider_host",
        "provider_base_url",
        "admitted_endpoint_paths",
        "max_provider_requests",
        "max_retries",
        "max_input_tokens",
        "max_output_tokens",
        "max_total_tokens",
        "max_wall_time_ms",
        "cost_authority",
    ] {
        if spend_body.get(field).is_none() || spend_body.get(field) == Some(&Value::Null) {
            return Err(format!("managed Codex spend {field} owner is missing"));
        }
    }
    Ok(())
}

/// New runtime-profile-backed spends must agree with the immutable graph node
/// selected by the scheduler. Historical Codex fixtures have no profile and
/// remain readable through the compatibility path above; they cannot be used
/// by the production launcher, which requires profile observations.
fn validate_managed_codex_runtime_profile_node(
    node: &Value,
    spend_body: &Value,
) -> Result<(), String> {
    let Some(profile_sha) = spend_body
        .get("runtime_profile_sha256")
        .and_then(Value::as_str)
    else {
        return Ok(());
    };
    let identity = node
        .get("managed_executor_identity")
        .and_then(Value::as_object)
        .ok_or_else(|| "managed Codex runtime profile node identity is missing".to_string())?;
    if identity.get("schema_version").and_then(Value::as_str)
        != Some("managed_executor_identity.v1")
        || identity.get("executor_type").and_then(Value::as_str) != Some("codex_cli")
        || identity
            .get("runtime_profile_sha256")
            .and_then(Value::as_str)
            != Some(profile_sha)
        || identity
            .get("capability_probe_sha256")
            .and_then(Value::as_str)
            != spend_body
                .get("capability_probe_sha256")
                .and_then(Value::as_str)
        || identity.get("binary_path").and_then(Value::as_str)
            != spend_body.get("binary_path").and_then(Value::as_str)
        || identity.get("binary_version").and_then(Value::as_str)
            != spend_body.get("binary_version").and_then(Value::as_str)
        || identity.get("binary_sha256").and_then(Value::as_str)
            != spend_body.get("binary_sha256").and_then(Value::as_str)
        || identity.get("model").and_then(Value::as_str)
            != spend_body.get("model").and_then(Value::as_str)
    {
        return Err(
            "managed Codex runtime profile identity does not match the scheduler node".to_string(),
        );
    }
    Ok(())
}

/// ProductTask budget is an independent owner from the decision/spend chain.
/// A decision may narrow it, but no live spend can enlarge the currently
/// persisted ProductTask limits.
fn validate_managed_codex_task_budget(task: &Value, spend_body: &Value) -> Result<(), String> {
    let budget = task
        .pointer("/intake/budget")
        .filter(|budget| budget.is_object())
        .ok_or_else(|| "managed Codex ProductTask budget owner is missing".to_string())?;
    for (task_field, spend_field) in [
        ("total_tokens", "max_total_tokens"),
        ("total_calls", "max_provider_requests"),
        ("total_elapsed_ms", "max_wall_time_ms"),
        ("max_retries", "max_retries"),
    ] {
        let task_limit = budget
            .get(task_field)
            .and_then(Value::as_u64)
            .filter(|value| *value > 0 || task_field == "max_retries")
            .ok_or_else(|| format!("managed Codex ProductTask budget.{task_field} is missing"))?;
        let spend_limit = spend_body
            .get(spend_field)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("managed Codex spend {spend_field} is missing"))?;
        if spend_limit > task_limit {
            return Err(format!(
                "managed Codex spend {spend_field} exceeds current ProductTask budget.{task_field}"
            ));
        }
    }
    if budget.get("max_concurrency").and_then(Value::as_u64) != Some(1) {
        return Err(
            "managed Codex ProductTask budget.max_concurrency must be persisted as one".to_string(),
        );
    }
    Ok(())
}

/// Build the attempt request solely from the persisted one-use spend.  The
/// canonical manifest makes every authority field independently checkable by
/// the atomic attempt-admission transaction.
fn managed_codex_attempt_body(spend_body: &Value) -> Result<Value, String> {
    let manifest = build_attempt_authority_manifest(spend_body)?;
    let manifest_sha256 = required_str(&manifest, "manifest_sha256")?;
    let mut body = spend_body.clone();
    let object = body
        .as_object_mut()
        .ok_or_else(|| "managed Codex spend body must be an object".to_string())?;
    object.insert("manifest".to_string(), manifest);
    object.insert(
        "manifest_sha256".to_string(),
        Value::String(manifest_sha256),
    );
    Ok(sort_value(&body))
}

// --- internal accept helpers ---

fn validate_accept_preconditions(
    principal: &AuthenticatedPrincipal,
    request: &RiskAcknowledgementRequest,
    decision: &Value,
    now: &str,
) -> Result<(Value, String, String, bool), String> {
    if decision.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id.as_str()) {
        return Err("decision tenant mismatch".into());
    }
    let status = decision.get("status").and_then(Value::as_str).unwrap_or("");
    if status != "draft_pending_operator" && status != "operator_accepted" {
        return Err(format!("decision status {status} cannot be accepted"));
    }
    if matches!(
        status,
        "revoked" | "invalidated" | "expired" | "operator_rejected"
    ) {
        return Err(format!("decision status {status} cannot be accepted"));
    }
    if decision.get("decision_body_sha256").and_then(Value::as_str)
        != Some(request.expected_decision_body_sha256.as_str())
    {
        return Err("decision_body_sha256 mismatch".into());
    }
    if decision
        .get("residual_finding_sha256")
        .and_then(Value::as_str)
        != Some(request.expected_residual_finding_sha256.as_str())
    {
        return Err("residual_finding_sha256 mismatch".into());
    }
    if let Some(exp) = decision.get("expires_at").and_then(Value::as_str) {
        if is_at_or_before(exp, now)? {
            return Err("decision expired".into());
        }
    }
    let body = decision.get("body_json").cloned().unwrap_or(Value::Null);
    let required_phrase = body
        .pointer("/acknowledgement/required_phrase")
        .and_then(Value::as_str)
        .ok_or("decision missing acknowledgement.required_phrase")?;
    if request.submitted_phrase != required_phrase {
        return Err("operator risk-acceptance phrase mismatch".into());
    }
    if body.get("effect_envelope").is_some() {
        let envelope = validate_effect_envelope_body(
            &body,
            principal.tenant_id.as_str(),
            Some(principal),
            now,
        )?;
        if decision.get("decision_id").and_then(Value::as_str)
            != Some(envelope.decision_id.as_str())
        {
            return Err("effect envelope decision owner is mismatched".into());
        }
        if decision.get("expires_at").and_then(Value::as_str) != Some(envelope.expires_at.as_str())
        {
            return Err("effect envelope decision expiry is mismatched".into());
        }
    }
    // Scope and max expiry derived from decision only — never invent far-future expiry.
    let expires_at = require_finite_expiry(decision.get("expires_at").and_then(Value::as_str))?;
    if is_at_or_before(&expires_at, now)? {
        return Err("decision expired".into());
    }
    let scope = json!({
        "source": "persisted_decision",
        "trial_envelope": body.get("trial_envelope").cloned().unwrap_or(Value::Null),
        "effect_envelope": body.get("effect_envelope").cloned().unwrap_or(Value::Null),
        "decision_id": request.decision_id,
        "decision_body_sha256": request.expected_decision_body_sha256,
        "residual_finding_sha256": request.expected_residual_finding_sha256,
        "expires_at": expires_at,
        "no_caller_scope_expansion": true,
    });
    let fixture_only = matches!(principal.principal_kind, PrincipalKind::FixturePrincipal);
    // Must still be draft for first accept; operator_accepted allows exact replay only.
    Ok((scope, expires_at, status.to_string(), fixture_only))
}

fn accept_on_sqlite(
    tx: &rusqlite::Transaction<'_>,
    principal: &AuthenticatedPrincipal,
    request: &RiskAcknowledgementRequest,
    decision: &Value,
    now: &str,
) -> Result<Value, String> {
    let (scope, expires_at, status, fixture_only) =
        validate_accept_preconditions(principal, request, decision, now)?;
    if let Some(existing_id) = tx
        .query_row(
            "SELECT authorization_id FROM managed_acceptance_authorizations
             WHERE decision_id=?1 AND principal_id=?2 AND decision_body_sha256=?3",
            params![
                request.decision_id,
                principal.principal_id,
                request.expected_decision_body_sha256
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        let existing = load_authorization_sqlite(tx, &existing_id)?;
        // Exact replay of full canonical auth body only.
        let expected_body = build_risk_auth_body(
            &existing_id,
            principal,
            request,
            &scope,
            &expires_at,
            fixture_only,
        );
        let expected_sha = sha256_hex(canonical_json(&expected_body)?.as_bytes());
        if existing.get("authorization_sha256").and_then(Value::as_str)
            != Some(expected_sha.as_str())
        {
            return Err("conflicting risk acknowledgement replay".into());
        }
        return Ok(existing);
    }
    if status != "draft_pending_operator" {
        return Err(format!(
            "decision status {status} cannot receive first acceptance"
        ));
    }
    let auth_id = format!("maa-{}", Uuid::new_v4());
    let auth_body = build_risk_auth_body(
        &auth_id,
        principal,
        request,
        &scope,
        &expires_at,
        fixture_only,
    );
    let authorization_sha256 = sha256_hex(canonical_json(&auth_body)?.as_bytes());
    tx.execute(
        "UPDATE managed_acceptance_decisions SET status='operator_accepted', principal_kind=?1, principal_id=?2, updated_at=?3 WHERE decision_id=?4",
        params![
            principal.principal_kind.as_str(),
            principal.principal_id,
            now,
            request.decision_id
        ],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO managed_acceptance_authorizations (
            authorization_id, decision_id, tenant_id, principal_kind, principal_id,
            decision_body_sha256, residual_finding_sha256, authorization_sha256,
            scope_json, status, mutation_authority, execution_granted, body_json,
            created_at, updated_at, expires_at, revoked_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'active','authorization_receipt_only',0,?10,?11,?11,?12,NULL)",
        params![
            auth_id,
            request.decision_id,
            principal.tenant_id,
            principal.principal_kind.as_str(),
            principal.principal_id,
            request.expected_decision_body_sha256,
            request.expected_residual_finding_sha256,
            authorization_sha256,
            scope.to_string(),
            auth_body.to_string(),
            now,
            expires_at,
        ],
    )
    .map_err(|e| e.to_string())?;
    // Mutable status is represented by a durable receipt; the immutable decision
    // authority hash is never rewritten.
    let transition = persist_transition_sqlite(
        tx,
        decision,
        "draft_pending_operator",
        "operator_accepted",
        principal,
        now,
        "risk_acknowledgement_accepted",
    )?;
    append_audit_locked(
        tx,
        now,
        &principal.principal_id,
        "managed_acceptance.decision_accepted",
        &request.decision_id,
        &json!({
            "authorization_id": auth_id,
            "authorization_sha256": authorization_sha256,
            "execution_granted": false,
            "status_transition": transition,
        }),
    )?;
    load_authorization_sqlite(tx, &auth_id)
}

#[cfg(feature = "pg")]
fn accept_on_pg(
    tx: &mut postgres::Transaction<'_>,
    principal: &AuthenticatedPrincipal,
    request: &RiskAcknowledgementRequest,
    decision: &Value,
    now: &str,
) -> Result<Value, String> {
    let (scope, expires_at, status, fixture_only) =
        validate_accept_preconditions(principal, request, decision, now)?;
    if let Some(row) = tx
        .query_opt(
            "SELECT authorization_id FROM managed_acceptance_authorizations
             WHERE decision_id=$1 AND principal_id=$2 AND decision_body_sha256=$3 FOR UPDATE",
            &[
                &request.decision_id,
                &principal.principal_id,
                &request.expected_decision_body_sha256,
            ],
        )
        .map_err(|e| e.to_string())?
    {
        let existing_id: String = row.get(0);
        let existing = load_authorization_pg(tx, &existing_id)?;
        let expected_body = build_risk_auth_body(
            &existing_id,
            principal,
            request,
            &scope,
            &expires_at,
            fixture_only,
        );
        let expected_sha = sha256_hex(canonical_json(&expected_body)?.as_bytes());
        if existing.get("authorization_sha256").and_then(Value::as_str)
            != Some(expected_sha.as_str())
        {
            return Err("conflicting risk acknowledgement replay".into());
        }
        return Ok(existing);
    }
    if status != "draft_pending_operator" {
        return Err(format!(
            "decision status {status} cannot receive first acceptance"
        ));
    }
    let auth_id = format!("maa-{}", Uuid::new_v4());
    let auth_body = build_risk_auth_body(
        &auth_id,
        principal,
        request,
        &scope,
        &expires_at,
        fixture_only,
    );
    let authorization_sha256 = sha256_hex(canonical_json(&auth_body)?.as_bytes());
    let pk = principal.principal_kind.as_str();
    tx.execute(
        "UPDATE managed_acceptance_decisions SET status='operator_accepted', principal_kind=$1, principal_id=$2, updated_at=$3 WHERE decision_id=$4",
        &[&pk, &principal.principal_id, &now, &request.decision_id],
    )
    .map_err(|e| e.to_string())?;
    let zero: i32 = 0;
    let active = "active";
    let mut_auth = "authorization_receipt_only";
    tx.execute(
        "INSERT INTO managed_acceptance_authorizations (
            authorization_id, decision_id, tenant_id, principal_kind, principal_id,
            decision_body_sha256, residual_finding_sha256, authorization_sha256,
            scope_json, status, mutation_authority, execution_granted, body_json,
            created_at, updated_at, expires_at, revoked_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$14,$15,NULL)",
        &[
            &auth_id,
            &request.decision_id,
            &principal.tenant_id,
            &pk,
            &principal.principal_id,
            &request.expected_decision_body_sha256,
            &request.expected_residual_finding_sha256,
            &authorization_sha256,
            &scope.to_string(),
            &active,
            &mut_auth,
            &zero,
            &auth_body.to_string(),
            &now,
            &expires_at,
        ],
    )
    .map_err(|e| e.to_string())?;
    let transition = persist_transition_pg(
        tx,
        decision,
        "draft_pending_operator",
        "operator_accepted",
        principal,
        now,
        "risk_acknowledgement_accepted",
    )?;
    append_effect_audit_pg(
        tx,
        now,
        &principal.principal_id,
        "managed_acceptance.decision_accepted",
        &request.decision_id,
        &json!({
            "authorization_id": auth_id,
            "authorization_sha256": authorization_sha256,
            "execution_granted": false,
            "status_transition": transition,
        }),
    )?;
    load_authorization_pg(tx, &auth_id)
}

fn build_risk_auth_body(
    auth_id: &str,
    principal: &AuthenticatedPrincipal,
    request: &RiskAcknowledgementRequest,
    scope: &Value,
    expires_at: &str,
    fixture_only: bool,
) -> Value {
    sort_value(&json!({
        "schema_version": "managed_acceptance_authorization.v1",
        "authorization_id": auth_id,
        "decision_id": request.decision_id,
        "tenant_id": principal.tenant_id,
        "principal_kind": principal.principal_kind.as_str(),
        "principal_id": principal.principal_id,
        "decision_body_sha256": request.expected_decision_body_sha256,
        "residual_finding_sha256": request.expected_residual_finding_sha256,
        "scope": scope,
        "expires_at": expires_at,
        "mutation_authority": "authorization_receipt_only",
        "execution_granted": false,
        "fixture_only": fixture_only,
    }))
}

fn issue_spend_sqlite(
    tx: &rusqlite::Transaction<'_>,
    principal: &AuthenticatedPrincipal,
    request: &SpendAuthorizationRequest,
    fixture_only: bool,
    now: &str,
) -> Result<Value, String> {
    let risk = load_authorization_sqlite(tx, &request.risk_authorization_id)?;
    validate_risk_for_spend(&risk, principal, now, fixture_only)?;
    // Risk ack never grants execution.
    if risk.get("execution_granted").and_then(Value::as_bool) != Some(false) {
        return Err(
            "risk acknowledgement execution_granted must be a persisted false boolean".into(),
        );
    }
    let decision_id = risk
        .get("decision_id")
        .and_then(Value::as_str)
        .ok_or("risk auth missing decision_id")?;
    let decision = load_decision_sqlite(tx, decision_id)?;
    let decision_body = decision.get("body_json").cloned().unwrap_or(Value::Null);
    validate_spend_against_decision(request, &decision_body)?;
    let logical_body = build_spend_body(
        "",
        principal,
        request,
        &risk,
        decision_id,
        fixture_only,
        "",
        None,
    )?;
    let logical_authorization_sha256 = stable_spend_authorization_identity(&logical_body)?;
    if let Some(existing_id) =
        find_active_spend_sqlite(tx, &principal.tenant_id, &logical_authorization_sha256)?
    {
        return load_spend_sqlite(tx, &existing_id);
    }
    let spend_id = format!("mas-{}", Uuid::new_v4());
    let body = build_spend_body(
        &spend_id,
        principal,
        request,
        &risk,
        decision_id,
        fixture_only,
        now,
        Some(&logical_authorization_sha256),
    )?;
    let spend_body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
    let expires_at = risk
        .get("expires_at")
        .and_then(Value::as_str)
        .unwrap_or(now)
        .to_string();
    let risk_sha = risk
        .get("authorization_sha256")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let dsha = risk
        .get("decision_body_sha256")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let rsha = risk
        .get("residual_finding_sha256")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let inserted = tx
        .execute(
            "INSERT INTO managed_acceptance_spend_authorizations (
            spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
            principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
            logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
            fixture_only, status, body_json,
            created_at, updated_at, expires_at, consumed_at, consumed_by_attempt_id, revoked_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'active',?13,?14,?14,?15,NULL,NULL,NULL)
           ON CONFLICT DO NOTHING",
            params![
                spend_id,
                decision_id,
                request.risk_authorization_id,
                principal.tenant_id,
                principal.principal_kind.as_str(),
                principal.principal_id,
                spend_body_sha256,
                risk_sha,
                logical_authorization_sha256,
                dsha,
                rsha,
                if fixture_only { 1 } else { 0 },
                body.to_string(),
                now,
                expires_at,
            ],
        )
        .map_err(|e| e.to_string())?;
    if inserted == 0 {
        let existing_id =
            find_active_spend_sqlite(tx, &principal.tenant_id, &logical_authorization_sha256)?
                .ok_or("spend issuance conflict did not resolve to an active logical receipt")?;
        return load_spend_sqlite(tx, &existing_id);
    }
    append_audit_locked(
        tx,
        now,
        &principal.principal_id,
        "managed_acceptance.spend_issued",
        &spend_id,
        &json!({
            "spend_body_sha256": spend_body_sha256,
            "logical_authorization_sha256": logical_authorization_sha256,
            "risk_authorization_id": request.risk_authorization_id,
            "fixture_only": fixture_only,
        }),
    )?;
    load_spend_sqlite(tx, &spend_id)
}

#[cfg(feature = "pg")]
fn issue_spend_pg(
    tx: &mut postgres::Transaction<'_>,
    principal: &AuthenticatedPrincipal,
    request: &SpendAuthorizationRequest,
    fixture_only: bool,
    now: &str,
) -> Result<Value, String> {
    let risk = load_authorization_pg(tx, &request.risk_authorization_id)?;
    validate_risk_for_spend(&risk, principal, now, fixture_only)?;
    if risk.get("execution_granted").and_then(Value::as_bool) != Some(false) {
        return Err(
            "risk acknowledgement execution_granted must be a persisted false boolean".into(),
        );
    }
    let decision_id = risk
        .get("decision_id")
        .and_then(Value::as_str)
        .ok_or("risk auth missing decision_id")?
        .to_string();
    let decision = load_decision_pg(tx, &decision_id)?;
    let decision_body = decision.get("body_json").cloned().unwrap_or(Value::Null);
    validate_spend_against_decision(request, &decision_body)?;
    let logical_body = build_spend_body(
        "",
        principal,
        request,
        &risk,
        &decision_id,
        fixture_only,
        "",
        None,
    )?;
    let logical_authorization_sha256 = stable_spend_authorization_identity(&logical_body)?;
    if let Some(existing) =
        find_active_spend_pg(tx, &principal.tenant_id, &logical_authorization_sha256)?
    {
        return load_spend_pg(tx, &existing);
    }
    let spend_id = format!("mas-{}", Uuid::new_v4());
    let body = build_spend_body(
        &spend_id,
        principal,
        request,
        &risk,
        &decision_id,
        fixture_only,
        now,
        Some(&logical_authorization_sha256),
    )?;
    let spend_body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
    let expires_at = risk
        .get("expires_at")
        .and_then(Value::as_str)
        .unwrap_or(now)
        .to_string();
    let risk_sha = risk
        .get("authorization_sha256")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let dsha = risk
        .get("decision_body_sha256")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let rsha = risk
        .get("residual_finding_sha256")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let pk = principal.principal_kind.as_str();
    let fixture_i: i32 = if fixture_only { 1 } else { 0 };
    let active = "active";
    let inserted = tx
        .execute(
            "INSERT INTO managed_acceptance_spend_authorizations (
            spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
            principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
            logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
            fixture_only, status, body_json,
            created_at, updated_at, expires_at, consumed_at, consumed_by_attempt_id, revoked_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$15,$16,NULL,NULL,NULL)
           ON CONFLICT DO NOTHING",
            &[
                &spend_id,
                &decision_id,
                &request.risk_authorization_id,
                &principal.tenant_id,
                &pk,
                &principal.principal_id,
                &spend_body_sha256,
                &risk_sha,
                &logical_authorization_sha256,
                &dsha,
                &rsha,
                &fixture_i,
                &active,
                &body.to_string(),
                &now,
                &expires_at,
            ],
        )
        .map_err(|e| e.to_string())?;
    if inserted == 0 {
        let existing =
            find_active_spend_pg(tx, &principal.tenant_id, &logical_authorization_sha256)?
                .ok_or("spend issuance conflict did not resolve to an active logical receipt")?;
        return load_spend_pg(tx, &existing);
    }
    load_spend_pg(tx, &spend_id)
}

/// Find an active receipt by its stable authorization identity. V33 makes the
/// identity mandatory for every active row; any null value is database
/// corruption and must fail closed rather than fall back to a legacy replay.
fn find_active_spend_sqlite(
    tx: &rusqlite::Transaction<'_>,
    tenant_id: &str,
    logical_authorization_sha256: &str,
) -> Result<Option<String>, String> {
    if tx
        .query_row(
            "SELECT spend_authorization_id FROM managed_acceptance_spend_authorizations
             WHERE tenant_id=?1 AND status='active'
               AND logical_authorization_sha256 IS NULL LIMIT 1",
            params![tenant_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Err("active spend is missing mandatory logical authorization identity".to_string());
    }
    if let Some(id) = tx
        .query_row(
            "SELECT spend_authorization_id FROM managed_acceptance_spend_authorizations
             WHERE tenant_id=?1 AND logical_authorization_sha256=?2 AND status='active'",
            params![tenant_id, logical_authorization_sha256],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    {
        return Ok(Some(id));
    }
    Ok(None)
}

#[cfg(feature = "pg")]
fn find_active_spend_pg(
    tx: &mut postgres::Transaction<'_>,
    tenant_id: &str,
    logical_authorization_sha256: &str,
) -> Result<Option<String>, String> {
    if tx
        .query_opt(
            "SELECT spend_authorization_id FROM managed_acceptance_spend_authorizations
             WHERE tenant_id=$1 AND status='active'
               AND logical_authorization_sha256 IS NULL
             LIMIT 1 FOR UPDATE",
            &[&tenant_id],
        )
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Err("active spend is missing mandatory logical authorization identity".to_string());
    }
    if let Some(row) = tx
        .query_opt(
            "SELECT spend_authorization_id FROM managed_acceptance_spend_authorizations
             WHERE tenant_id=$1 AND logical_authorization_sha256=$2 AND status='active'
             FOR UPDATE",
            &[&tenant_id, &logical_authorization_sha256],
        )
        .map_err(|error| error.to_string())?
    {
        return Ok(Some(row.get(0)));
    }
    Ok(None)
}

fn validate_risk_for_spend(
    risk: &Value,
    principal: &AuthenticatedPrincipal,
    now: &str,
    fixture_only: bool,
) -> Result<(), String> {
    if risk.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id.as_str()) {
        return Err("risk authorization tenant mismatch".into());
    }
    if risk.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id.as_str()) {
        return Err("risk authorization principal mismatch".into());
    }
    if risk.get("status").and_then(Value::as_str) != Some("active") {
        return Err("risk authorization is not active".into());
    }
    if let Some(exp) = risk.get("expires_at").and_then(Value::as_str) {
        if is_at_or_before(exp, now)? {
            return Err("risk authorization expired".into());
        }
    }
    let risk_fixture = risk
        .get("fixture_only")
        .and_then(Value::as_bool)
        .ok_or("risk authorization fixture_only must be a persisted boolean")?;
    if fixture_only != risk_fixture {
        return Err("fixture_only mismatch between principal and risk auth".into());
    }
    Ok(())
}

fn validate_spend_against_decision(
    request: &SpendAuthorizationRequest,
    decision_body: &Value,
) -> Result<(), String> {
    let trial = decision_body
        .get("trial_envelope")
        .cloned()
        .ok_or("decision missing trial_envelope")?;
    validate_trial_envelope_shape(&trial)?;

    // Budgets: exact match to trial (no expansion, no silent under-binding for production).
    eq_u64_field(
        &trial,
        "max_provider_requests",
        request.max_provider_requests,
    )?;
    eq_u64_field(&trial, "max_retries", request.max_retries)?;
    eq_u64_field(&trial, "max_input_tokens", request.max_input_tokens)?;
    eq_u64_field(&trial, "max_output_tokens", request.max_output_tokens)?;
    eq_u64_field(&trial, "max_total_tokens", request.max_total_tokens)?;
    eq_u64_field(&trial, "max_wall_time_ms", request.max_wall_time_ms)?;
    if request.max_retries > 0 {
        return Err("max_retries must be 0 under residual_admission_no_go".into());
    }
    if request.max_provider_requests == 0 {
        return Err("max_provider_requests must be positive".into());
    }
    if request.max_total_tokens == 0
        || request.max_input_tokens == 0
        || request.max_output_tokens == 0
        || request.max_wall_time_ms == 0
    {
        return Err("token and wall-time budgets must be positive".into());
    }

    // Provider / model / endpoint identities must match trial exactly.
    eq_str_field(&trial, "provider_kind", &request.provider_kind)?;
    eq_str_field(&trial, "provider_host", &request.provider_host)?;
    eq_str_field(&trial, "provider_base_url", &request.provider_base_url)?;
    eq_str_field(&trial, "model", &request.model)?;
    eq_str_field(&trial, "product_task_id", &request.product_task_id)?;
    eq_value_field(
        &trial,
        "workflow_id",
        &serde_json::to_value(&request.workflow_id).map_err(|error| error.to_string())?,
    )?;
    eq_value_field(
        &trial,
        "workflow_node_id",
        &serde_json::to_value(&request.workflow_node_id).map_err(|error| error.to_string())?,
    )?;
    eq_str_field(&trial, "execution_id", &request.execution_id)?;
    eq_str_field(&trial, "attempt_id", &request.attempt_id)?;
    eq_str_field(&trial, "target_repo", &request.target_repo)?;
    eq_str_field(&trial, "target_main_sha", &request.target_main_sha)?;
    eq_str_field(&trial, "exact_codex_path", &request.binary_path)?;
    eq_str_field(&trial, "exact_codex_sha256", &request.binary_sha256)?;
    eq_str_field(
        &trial,
        "cancellation_identity",
        &request.cancellation_identity,
    )?;
    eq_str_field(&trial, "rollback_identity", &request.rollback_identity)?;
    eq_str_field(
        &trial,
        "output_branch_prefix",
        &request.output_branch_prefix,
    )?;
    eq_value_field(&trial, "cost_authority", &request.cost_authority.to_json())?;
    if let Some(paths) = trial
        .get("admitted_endpoint_paths")
        .and_then(Value::as_array)
    {
        let trial_paths: Vec<String> = paths
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if trial_paths != request.admitted_endpoint_paths {
            return Err("admitted_endpoint_paths mismatch vs decision trial envelope".into());
        }
    } else {
        return Err("trial_envelope.admitted_endpoint_paths required".into());
    }
    eq_str_field(&trial, "exact_codex_version", &request.binary_version)?;
    if let Some(profile_sha) = request.runtime_profile_sha256.as_deref() {
        eq_str_field(&trial, "runtime_profile_sha256", profile_sha)?;
        let capability_sha = request
            .capability_probe_sha256
            .as_deref()
            .ok_or("runtime-profile spend requires capability_probe_sha256")?;
        eq_str_field(&trial, "capability_probe_sha256", capability_sha)?;
    }
    if trial
        .get("exact_codex_sha_required")
        .and_then(Value::as_bool)
        != Some(true)
        || request.binary_sha256.len() != 64
        || !request.binary_sha256.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("binary_sha256 must be 64 hex chars when exact_codex_sha_required".into());
    }
    if request.binary_path.trim().is_empty() || !request.binary_path.starts_with('/') {
        return Err("binary_path must be absolute".into());
    }

    if trial.get("draft_pr_only").and_then(Value::as_bool) != Some(request.draft_pr_only)
        || !request.draft_pr_only
    {
        return Err("spend draft_pr_only mismatches decision trial envelope".into());
    }
    if trial.get("auto_merge_disabled").and_then(Value::as_bool) != Some(true) {
        return Err("trial_envelope.auto_merge_disabled must be true".into());
    }
    if request.target_main_sha.len() != 40 && request.target_main_sha.len() != 64 {
        return Err("target_main_sha invalid".into());
    }
    if request.target_repo.trim().is_empty() {
        return Err("target_repo required".into());
    }
    if !request.output_branch_prefix.starts_with("acp/") {
        return Err("output_branch_prefix must be acp/*".into());
    }
    if request.product_task_id.trim().is_empty()
        || request.execution_id.trim().is_empty()
        || request.attempt_id.trim().is_empty()
    {
        return Err("product_task_id/execution_id/attempt_id required".into());
    }
    if request.cancellation_identity.trim().is_empty()
        || request.rollback_identity.trim().is_empty()
    {
        return Err("cancellation_identity and rollback_identity required".into());
    }
    // cost_authority self-validates via construction and was equality-bound above.
    let _ = CostAuthority::from_json(&request.cost_authority.to_json())?;
    // If trial has monetary estimate text only, cost must not claim provider_reported without ceiling.
    if matches!(
        request.cost_authority,
        CostAuthority::ProviderReported { .. }
    ) && trial.get("max_cost_usd_estimate").is_some()
        && trial
            .get("max_cost_usd_estimate")
            .and_then(Value::as_str)
            .is_some_and(|s| s.contains("unresolved") || s.contains("estimate_only"))
    {
        return Err(
            "cost_authority provider_reported rejected while trial marks cost estimate unresolved"
                .into(),
        );
    }
    Ok(())
}

fn eq_u64_field(trial: &Value, key: &str, observed: u64) -> Result<(), String> {
    let expected = trial
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("trial_envelope.{key} required"))?;
    if expected != observed {
        return Err(format!(
            "spend {key}={observed} mismatches decision trial envelope {key}={expected}"
        ));
    }
    Ok(())
}

fn eq_str_field(trial: &Value, key: &str, observed: &str) -> Result<(), String> {
    let expected = trial
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("trial_envelope.{key} required"))?;
    if expected != observed {
        return Err(format!("spend {key} mismatches decision trial envelope"));
    }
    Ok(())
}

fn eq_value_field(trial: &Value, key: &str, observed: &Value) -> Result<(), String> {
    let expected = trial
        .get(key)
        .ok_or_else(|| format!("trial_envelope.{key} required"))?;
    if sort_value(expected) != sort_value(observed) {
        return Err(format!("spend {key} mismatches decision trial envelope"));
    }
    Ok(())
}

/// Stable authorization identity for one logical spend request. Receipt IDs and
/// timestamps deliberately do not participate, so a retry cannot mint a second
/// active one-use grant for the same attempt.
pub(super) fn stable_spend_authorization_identity(body: &Value) -> Result<String, String> {
    let mut identity = body.clone();
    let object = identity
        .as_object_mut()
        .ok_or("spend authorization body must be an object")?;
    object.remove("spend_authorization_id");
    object.remove("created_at");
    object.remove("logical_authorization_sha256");
    // A risk acknowledgement is the parent authority chain, not the logical
    // attempt identity. Revocation cascades to active spends, so a retry under
    // an equivalent current acknowledgement must collapse to the same active
    // spend rather than minting another one solely because its parent receipt
    // UUID/timestamp changed.
    object.remove("risk_authorization_id");
    object.remove("risk_authorization_sha256");
    Ok(sha256_hex(
        canonical_json(&sort_value(&identity))?.as_bytes(),
    ))
}

fn build_spend_body(
    spend_id: &str,
    principal: &AuthenticatedPrincipal,
    request: &SpendAuthorizationRequest,
    risk: &Value,
    decision_id: &str,
    fixture_only: bool,
    now: &str,
    logical_authorization_sha256: Option<&str>,
) -> Result<Value, String> {
    Ok(sort_value(&json!({
        "schema_version": "managed_acceptance_spend_authorization.v1",
        "spend_authorization_id": spend_id,
        "decision_id": decision_id,
        "risk_authorization_id": request.risk_authorization_id,
        "risk_authorization_sha256": risk.get("authorization_sha256"),
        "decision_body_sha256": risk.get("decision_body_sha256"),
        "residual_finding_sha256": risk.get("residual_finding_sha256"),
        "tenant_id": principal.tenant_id,
        "principal_kind": principal.principal_kind.as_str(),
        "principal_id": principal.principal_id,
        "logical_authorization_sha256": logical_authorization_sha256,
        "product_task_id": request.product_task_id,
        "workflow_id": request.workflow_id,
        "workflow_node_id": request.workflow_node_id,
        "execution_id": request.execution_id,
        "attempt_id": request.attempt_id,
        "binary_path": request.binary_path,
        "binary_version": request.binary_version,
        "binary_sha256": request.binary_sha256,
        "runtime_profile_sha256": request.runtime_profile_sha256,
        "capability_probe_sha256": request.capability_probe_sha256,
        "provider_kind": request.provider_kind,
        "provider_host": request.provider_host,
        "provider_base_url": request.provider_base_url,
        "admitted_endpoint_paths": request.admitted_endpoint_paths,
        "model": request.model,
        "target_repo": request.target_repo,
        "target_main_sha": request.target_main_sha,
        "output_branch_prefix": request.output_branch_prefix,
        "draft_pr_only": request.draft_pr_only,
        "max_provider_requests": request.max_provider_requests,
        "max_retries": request.max_retries,
        "max_input_tokens": request.max_input_tokens,
        "max_output_tokens": request.max_output_tokens,
        "max_total_tokens": request.max_total_tokens,
        "max_wall_time_ms": request.max_wall_time_ms,
        "cost_authority": request.cost_authority.to_json(),
        "cancellation_identity": request.cancellation_identity,
        "rollback_identity": request.rollback_identity,
        "one_use": true,
        "fixture_only": fixture_only,
        "created_at": now,
        "expires_at": risk.get("expires_at"),
    })))
}

/// Prove attempt body / complete canonical manifest match the one-use spend authorization
/// before atomic consumption. Every spend-bound field is required (no optional skip).
fn validate_attempt_body_matches_spend(
    attempt_id: &str,
    attempt_body: &Value,
    spend: &Value,
) -> Result<(), String> {
    let spend_body = spend
        .get("body_json")
        .cloned()
        .unwrap_or_else(|| spend.clone());
    let require_match = |field: &str| -> Result<(), String> {
        let expected = spend_body
            .get(field)
            .cloned()
            .or_else(|| spend.get(field).cloned())
            .ok_or_else(|| format!("spend missing bound field {field}"))?;
        let observed = attempt_body
            .get(field)
            .cloned()
            .ok_or_else(|| format!("attempt body missing required spend-bound field {field}"))?;
        if expected != observed {
            return Err(format!(
                "attempt body {field} mismatch vs spend authorization (attempt={observed}, spend={expected})"
            ));
        }
        Ok(())
    };
    for field in [
        "product_task_id",
        "workflow_id",
        "workflow_node_id",
        "execution_id",
        "binary_path",
        "binary_version",
        "binary_sha256",
        "runtime_profile_sha256",
        "capability_probe_sha256",
        "provider_kind",
        "provider_host",
        "provider_base_url",
        "admitted_endpoint_paths",
        "model",
        "target_repo",
        "target_main_sha",
        "output_branch_prefix",
        "draft_pr_only",
        "max_provider_requests",
        "max_retries",
        "max_input_tokens",
        "max_output_tokens",
        "max_total_tokens",
        "max_wall_time_ms",
        "cost_authority",
        "cancellation_identity",
        "rollback_identity",
    ] {
        require_match(field)?;
    }
    let spend_attempt = spend_body
        .get("attempt_id")
        .and_then(Value::as_str)
        .or_else(|| spend.get("attempt_id").and_then(Value::as_str))
        .ok_or("spend missing attempt_id")?;
    if spend_attempt != attempt_id {
        return Err("attempt_id mismatch vs spend authorization".into());
    }

    // Complete canonical manifest is required; recompute sha and require every authority field.
    let manifest = attempt_body
        .get("manifest")
        .ok_or("attempt body requires complete canonical manifest")?;
    let expected_manifest = build_attempt_authority_manifest(&spend_body)?;
    for field in [
        "product_task_id",
        "workflow_id",
        "workflow_node_id",
        "execution_id",
        "attempt_id",
        "binary_path",
        "binary_version",
        "binary_sha256",
        "runtime_profile_sha256",
        "capability_probe_sha256",
        "provider_kind",
        "provider_host",
        "provider_base_url",
        "admitted_endpoint_paths",
        "model",
        "target_repo",
        "target_main_sha",
        "output_branch_prefix",
        "draft_pr_only",
        "max_provider_requests",
        "max_retries",
        "max_input_tokens",
        "max_output_tokens",
        "max_total_tokens",
        "max_wall_time_ms",
        "cost_authority",
        "cancellation_identity",
        "rollback_identity",
    ] {
        let expected = expected_manifest
            .get(field)
            .ok_or_else(|| format!("canonical manifest missing {field}"))?;
        let observed = manifest
            .get(field)
            .ok_or_else(|| format!("attempt manifest missing authority field {field}"))?;
        if expected != observed {
            return Err(format!("manifest.{field} mismatches spend-bound authority"));
        }
    }
    let recomputed = compute_attempt_manifest_sha256(manifest)?;
    let declared = attempt_body
        .get("manifest_sha256")
        .and_then(Value::as_str)
        .ok_or("attempt body requires manifest_sha256")?;
    if declared != recomputed {
        return Err(
            "manifest_sha256 does not match recomputed complete canonical manifest hash".into(),
        );
    }
    if let Some(manifest_sha) = manifest.get("manifest_sha256").and_then(Value::as_str) {
        if manifest_sha != recomputed {
            return Err("embedded manifest.manifest_sha256 mismatches recomputed hash".into());
        }
    }
    Ok(())
}

fn admit_sqlite(
    tx: &rusqlite::Transaction<'_>,
    principal: &AuthenticatedPrincipal,
    attempt_id: &str,
    body: &Value,
    attempt_body_sha256: &str,
    spend_authorization_id: &str,
    allow_fixture_dry_run: bool,
    now: &str,
) -> Result<Value, String> {
    // Idempotent exact attempt replay first.
    if let Some((existing_sha, _status)) = tx
        .query_row(
            "SELECT attempt_body_sha256, status FROM managed_acceptance_attempts WHERE tenant_id=?1 AND attempt_id=?2",
            params![principal.tenant_id, attempt_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        if existing_sha != attempt_body_sha256 {
            return Err("conflicting managed acceptance attempt body".into());
        }
        let mut row = load_attempt_sqlite(tx, attempt_id)?;
        if let Value::Object(ref mut m) = row {
            m.insert("idempotent_replay".into(), json!(true));
        }
        return Ok(row);
    }
    if tx
        .query_row(
            "SELECT attempt_id FROM managed_acceptance_attempts WHERE tenant_id=?1 AND attempt_body_sha256=?2",
            params![principal.tenant_id, attempt_body_sha256],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("attempt body already admitted under another attempt_id".into());
    }

    let spend = load_spend_sqlite(tx, spend_authorization_id)?;
    validate_spend_for_admit(&spend, principal, allow_fixture_dry_run, now)?;
    validate_attempt_body_matches_spend(attempt_id, body, &spend)?;
    let risk_authorization_id = spend
        .get("risk_authorization_id")
        .and_then(Value::as_str)
        .ok_or("spend missing risk_authorization_id")?;
    let risk = load_authorization_sqlite(tx, risk_authorization_id)?;
    validate_spend_risk_owner(&spend, &risk)?;
    let decision_id = spend
        .get("decision_id")
        .and_then(Value::as_str)
        .ok_or("spend missing decision_id")?;
    let decision = load_decision_sqlite(tx, decision_id)?;
    validate_risk_decision_owner(&risk, &decision)?;
    // Consume one-use spend atomically only after identity match.
    let updated = tx
        .execute(
            "UPDATE managed_acceptance_spend_authorizations
             SET status='consumed', consumed_at=?1, consumed_by_attempt_id=?2, updated_at=?1
             WHERE spend_authorization_id=?3 AND status='active'",
            params![now, attempt_id, spend_authorization_id],
        )
        .map_err(|e| e.to_string())?;
    if updated != 1 {
        return Err("spend authorization not active or already consumed".into());
    }
    if risk.get("execution_granted").and_then(Value::as_bool) != Some(false) {
        return Err("risk ack execution_granted must be a persisted false boolean".into());
    }
    let decision_id = decision_id.to_string();
    let manifest_sha256 = body
        .get("manifest_sha256")
        .and_then(Value::as_str)
        .ok_or("attempt body requires manifest_sha256")?;
    let execution_id = body
        .get("execution_id")
        .and_then(Value::as_str)
        .unwrap_or(attempt_id)
        .to_string();
    let product_task_id = body
        .get("product_task_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let workflow_node_id = body
        .get("workflow_node_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let lease_token = format!("lease-{}", Uuid::new_v4());
    tx.execute(
        "INSERT INTO managed_acceptance_attempts (
            attempt_id, tenant_id, product_task_id, workflow_node_id, execution_id,
            decision_id, authorization_id, spend_authorization_id, manifest_sha256, attempt_body_sha256,
            status, terminal_class, body_json, receipt_json, receipt_sha256, lease_token, created_at, updated_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'admitted',NULL,?11,NULL,NULL,?12,?13,?13)",
        params![
            attempt_id,
            principal.tenant_id,
            product_task_id,
            workflow_node_id,
            execution_id,
            decision_id,
            risk_authorization_id,
            spend_authorization_id,
            manifest_sha256,
            attempt_body_sha256,
            body.to_string(),
            lease_token,
            now,
        ],
    )
    .map_err(|e| e.to_string())?;
    append_audit_locked(
        tx,
        now,
        &principal.principal_id,
        "managed_acceptance.attempt_admitted",
        attempt_id,
        &json!({
            "spend_authorization_id": spend_authorization_id,
            "attempt_body_sha256": attempt_body_sha256,
            "lease_token_present": true,
        }),
    )?;
    load_attempt_sqlite(tx, attempt_id)
}

#[cfg(feature = "pg")]
fn admit_pg(
    tx: &mut postgres::Transaction<'_>,
    principal: &AuthenticatedPrincipal,
    attempt_id: &str,
    body: &Value,
    attempt_body_sha256: &str,
    spend_authorization_id: &str,
    allow_fixture_dry_run: bool,
    now: &str,
) -> Result<Value, String> {
    if let Some(row) = tx
        .query_opt(
            "SELECT attempt_body_sha256, status FROM managed_acceptance_attempts WHERE tenant_id=$1 AND attempt_id=$2 FOR UPDATE",
            &[&principal.tenant_id, &attempt_id],
        )
        .map_err(|e| e.to_string())?
    {
        let existing_sha: String = row.get(0);
        if existing_sha != attempt_body_sha256 {
            return Err("conflicting managed acceptance attempt body".into());
        }
        let mut existing = load_attempt_pg(tx, attempt_id)?;
        if let Value::Object(ref mut m) = existing {
            m.insert("idempotent_replay".into(), json!(true));
        }
        return Ok(existing);
    }
    if tx
        .query_opt(
            "SELECT attempt_id FROM managed_acceptance_attempts WHERE tenant_id=$1 AND attempt_body_sha256=$2",
            &[&principal.tenant_id, &attempt_body_sha256],
        )
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("attempt body already admitted under another attempt_id".into());
    }
    let spend = load_spend_pg(tx, spend_authorization_id)?;
    validate_spend_for_admit(&spend, principal, allow_fixture_dry_run, now)?;
    validate_attempt_body_matches_spend(attempt_id, body, &spend)?;
    let risk_authorization_id = spend
        .get("risk_authorization_id")
        .and_then(Value::as_str)
        .ok_or("spend missing risk_authorization_id")?
        .to_string();
    let risk = load_authorization_pg(tx, &risk_authorization_id)?;
    validate_spend_risk_owner(&spend, &risk)?;
    let decision_id = spend
        .get("decision_id")
        .and_then(Value::as_str)
        .ok_or("spend missing decision_id")?;
    let decision = load_decision_pg(tx, decision_id)?;
    validate_risk_decision_owner(&risk, &decision)?;
    if risk.get("execution_granted").and_then(Value::as_bool) != Some(false) {
        return Err("risk ack execution_granted must be a persisted false boolean".into());
    }
    let updated = tx
        .execute(
            "UPDATE managed_acceptance_spend_authorizations
             SET status='consumed', consumed_at=$1, consumed_by_attempt_id=$2, updated_at=$1
             WHERE spend_authorization_id=$3 AND status='active'",
            &[&now, &attempt_id, &spend_authorization_id],
        )
        .map_err(|e| e.to_string())?;
    if updated != 1 {
        return Err("spend authorization not active or already consumed".into());
    }
    let decision_id = decision_id.to_string();
    let manifest_sha256 = body
        .get("manifest_sha256")
        .and_then(Value::as_str)
        .ok_or("attempt body requires manifest_sha256")?
        .to_string();
    let execution_id = body
        .get("execution_id")
        .and_then(Value::as_str)
        .unwrap_or(attempt_id)
        .to_string();
    let product_task_id = body.get("product_task_id").and_then(Value::as_str);
    let workflow_node_id = body.get("workflow_node_id").and_then(Value::as_str);
    let lease_token = format!("lease-{}", Uuid::new_v4());
    let status = "admitted";
    tx.execute(
        "INSERT INTO managed_acceptance_attempts (
            attempt_id, tenant_id, product_task_id, workflow_node_id, execution_id,
            decision_id, authorization_id, spend_authorization_id, manifest_sha256, attempt_body_sha256,
            status, terminal_class, body_json, receipt_json, receipt_sha256, lease_token, created_at, updated_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,NULL,$12,NULL,NULL,$13,$14,$14)",
        &[
            &attempt_id,
            &principal.tenant_id,
            &product_task_id,
            &workflow_node_id,
            &execution_id,
            &decision_id,
            &risk_authorization_id,
            &spend_authorization_id,
            &manifest_sha256,
            &attempt_body_sha256,
            &status,
            &body.to_string(),
            &lease_token,
            &now,
        ],
    )
    .map_err(|e| e.to_string())?;
    load_attempt_pg(tx, attempt_id)
}

fn validate_spend_for_admit(
    spend: &Value,
    principal: &AuthenticatedPrincipal,
    allow_fixture_dry_run: bool,
    now: &str,
) -> Result<(), String> {
    if spend.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id.as_str()) {
        return Err("spend tenant mismatch".into());
    }
    if spend.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id.as_str()) {
        return Err("spend principal mismatch".into());
    }
    if spend.get("status").and_then(Value::as_str) != Some("active") {
        return Err("spend authorization is not active".into());
    }
    if let Some(exp) = spend.get("expires_at").and_then(Value::as_str) {
        if is_at_or_before(exp, now)? {
            return Err("spend authorization expired".into());
        }
    }
    let spend_principal_kind = PrincipalKind::parse(
        spend
            .get("principal_kind")
            .and_then(Value::as_str)
            .ok_or("spend principal_kind is missing")?,
    )?;
    if &spend_principal_kind != principal.principal_kind() {
        return Err("spend principal kind mismatch".into());
    }
    let fixture_only = spend
        .get("fixture_only")
        .and_then(Value::as_bool)
        .ok_or("spend fixture_only must be a persisted boolean")?;
    let spend_is_fixture = spend_principal_kind == PrincipalKind::FixturePrincipal;
    if fixture_only != spend_is_fixture {
        return Err("spend fixture_only boolean is inconsistent with principal kind".into());
    }
    if spend_is_fixture {
        if !allow_fixture_dry_run {
            return Err("fixture principal cannot admit production live attempt".into());
        }
    } else if !principal.may_authorize_production_live_start() {
        return Err("principal cannot admit production live attempt".into());
    }
    Ok(())
}

fn validate_spend_risk_owner(spend: &Value, risk: &Value) -> Result<(), String> {
    for (spend_field, risk_field) in [
        ("risk_authorization_id", "authorization_id"),
        ("risk_authorization_sha256", "authorization_sha256"),
        ("decision_id", "decision_id"),
        ("tenant_id", "tenant_id"),
        ("principal_kind", "principal_kind"),
        ("principal_id", "principal_id"),
        ("decision_body_sha256", "decision_body_sha256"),
        ("residual_finding_sha256", "residual_finding_sha256"),
    ] {
        if required_str(spend, spend_field)? != required_str(risk, risk_field)? {
            return Err(format!(
                "spend {spend_field} does not match its risk authorization owner"
            ));
        }
    }
    if risk.get("execution_granted").and_then(Value::as_bool) != Some(false) {
        return Err(
            "risk authorization execution_granted must be a persisted false boolean".into(),
        );
    }
    Ok(())
}

fn validate_risk_decision_owner(risk: &Value, decision: &Value) -> Result<(), String> {
    for (risk_field, decision_field) in [
        ("decision_id", "decision_id"),
        ("tenant_id", "tenant_id"),
        ("decision_body_sha256", "decision_body_sha256"),
        ("residual_finding_sha256", "residual_finding_sha256"),
    ] {
        if required_str(risk, risk_field)? != required_str(decision, decision_field)? {
            return Err(format!(
                "risk {risk_field} does not match its decision owner"
            ));
        }
    }
    Ok(())
}

fn effect_parent_from_authority(
    parent_authorization: &Value,
    decision: &Value,
    principal: &AuthenticatedPrincipal,
    now: &str,
) -> Result<EffectEnvelopeContract, String> {
    if parent_authorization.get("status").and_then(Value::as_str) != Some("active") {
        return Err("effect parent authorization is not active".into());
    }
    if parent_authorization
        .get("tenant_id")
        .and_then(Value::as_str)
        != Some(principal.tenant_id.as_str())
    {
        return Err("effect parent authorization tenant mismatch".into());
    }
    if parent_authorization
        .get("principal_kind")
        .and_then(Value::as_str)
        != Some(PrincipalKind::OperatorApiKey.as_str())
        || parent_authorization
            .get("execution_granted")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err("effect parent authorization is not an owner risk receipt".into());
    }
    if decision.get("status").and_then(Value::as_str) != Some("operator_accepted") {
        return Err("effect parent decision is not owner accepted".into());
    }
    validate_risk_decision_owner(parent_authorization, decision)?;
    let body = decision
        .get("body_json")
        .ok_or("effect parent decision body is missing")?;
    let envelope = validate_effect_envelope_body(body, principal.tenant_id.as_str(), None, now)?;
    if decision.get("decision_id").and_then(Value::as_str) != Some(envelope.decision_id.as_str())
        || decision.get("principal_id").and_then(Value::as_str)
            != Some(envelope.owner_principal_id.as_str())
        || parent_authorization
            .get("principal_id")
            .and_then(Value::as_str)
            != Some(envelope.owner_principal_id.as_str())
        || principal.principal_id != envelope.owner_principal_id
    {
        return Err("effect parent owner binding is stale or mismatched".into());
    }
    if decision.get("expires_at").and_then(Value::as_str) != Some(envelope.expires_at.as_str()) {
        return Err("effect parent expiry is stale or mismatched".into());
    }
    let scope_envelope = parent_authorization
        .pointer("/scope/effect_envelope")
        .ok_or("effect parent authorization scope is missing effect_envelope")?;
    if sort_value(scope_envelope) != envelope.body() {
        return Err("effect parent authorization scope is stale or mismatched".into());
    }
    Ok(envelope)
}

fn effect_child_replay_identity(body: &Value) -> Result<Value, String> {
    let mut identity = body.clone();
    let object = identity
        .as_object_mut()
        .ok_or("effect child body must be an object")?;
    for key in [
        "spend_authorization_id",
        "spend_body_sha256",
        "logical_authorization_sha256",
        "created_at",
    ] {
        object.remove(key);
    }
    Ok(sort_value(&identity))
}

fn effect_child_body(
    principal: &AuthenticatedPrincipal,
    parent_authorization: &Value,
    decision: &Value,
    envelope: &EffectEnvelopeContract,
    request: &EffectChildAuthorizationRequest,
    expires_at: &str,
    now: &str,
) -> Result<Value, String> {
    let risk_authorization_id = required_str(parent_authorization, "authorization_id")?;
    let risk_authorization_sha256 = required_str(parent_authorization, "authorization_sha256")?;
    let decision_id = required_str(decision, "decision_id")?;
    let decision_body_sha256 = required_str(decision, "decision_body_sha256")?;
    let residual_finding_sha256 = required_str(decision, "residual_finding_sha256")?;
    let body = json!({
        "schema_version": EFFECT_CHILD_AUTHORIZATION_SCHEMA_VERSION,
        "spend_authorization_id": request.child_authorization_id,
        "child_authorization_id": request.child_authorization_id,
        "decision_id": decision_id,
        "risk_authorization_id": risk_authorization_id,
        "risk_authorization_sha256": risk_authorization_sha256,
        "decision_body_sha256": decision_body_sha256,
        "residual_finding_sha256": residual_finding_sha256,
        "tenant_id": principal.tenant_id,
        "principal_kind": principal.principal_kind.as_str(),
        "principal_id": principal.principal_id,
        "parent_envelope_id": envelope.envelope_id,
        "parent_envelope_sha256": envelope.sha256()?,
        "parent_envelope": envelope.body(),
        "effect_kind": request.effect_kind,
        "operation_id": request.operation_id,
        "target_repository": request.target_repository,
        "target_main_sha": request.target_main_sha,
        "max_cost_usd": request.max_cost_usd,
        "expires_at": expires_at,
        "one_use": true,
        "outcome_unknown_retry": false,
        "fixture_only": false,
        "created_at": now,
    });
    Ok(sort_value(&body))
}

fn effect_child_receipt(spend: &Value, body: &Value, replayed: bool) -> Result<Value, String> {
    Ok(json!({
        "schema_version": EFFECT_CHILD_AUTHORIZATION_SCHEMA_VERSION,
        "child_authorization_id": body.get("child_authorization_id"),
        "parent_envelope_id": body.get("parent_envelope_id"),
        "parent_envelope_sha256": body.get("parent_envelope_sha256"),
        "effect_kind": body.get("effect_kind"),
        "operation_id": body.get("operation_id"),
        "target_repository": body.get("target_repository"),
        "target_main_sha": body.get("target_main_sha"),
        "max_cost_usd": body.get("max_cost_usd"),
        "expires_at": body.get("expires_at"),
        "spend_authorization_id": spend.get("spend_authorization_id"),
        "spend_body_sha256": spend.get("spend_body_sha256"),
        "status": spend.get("status"),
        "one_use": true,
        "outcome_unknown_retry": false,
        "replayed": replayed,
    }))
}

fn validate_effect_child_for_settlement(
    spend: &Value,
    parent_authorization: &Value,
    decision: &Value,
    principal: &AuthenticatedPrincipal,
) -> Result<(Value, EffectEnvelopeContract), String> {
    if spend.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id.as_str())
        || spend.get("principal_id").and_then(Value::as_str)
            != Some(principal.principal_id.as_str())
        || spend.get("principal_kind").and_then(Value::as_str)
            != Some(principal.principal_kind.as_str())
    {
        return Err("effect child settlement principal binding is invalid".into());
    }
    if spend.get("fixture_only").and_then(Value::as_bool) != Some(false) {
        return Err("effect child settlement cannot use fixture authority".into());
    }
    if !matches!(
        parent_authorization.get("status").and_then(Value::as_str),
        Some("active") | Some("revoked") | Some("expired")
    ) {
        return Err("effect parent authorization has an invalid terminal state".into());
    }
    validate_spend_risk_owner(spend, parent_authorization)?;
    validate_risk_decision_owner(parent_authorization, decision)?;
    let child = spend
        .get("body_json")
        .cloned()
        .ok_or("effect child body is missing")?;
    if child.get("schema_version").and_then(Value::as_str)
        != Some(EFFECT_CHILD_AUTHORIZATION_SCHEMA_VERSION)
    {
        return Err("effect child body schema_version is invalid".into());
    }
    if child.get("child_authorization_id").and_then(Value::as_str)
        != spend.get("spend_authorization_id").and_then(Value::as_str)
    {
        return Err("effect child id is not bound to its spend row".into());
    }
    if child.get("spend_authorization_id").and_then(Value::as_str)
        != spend.get("spend_authorization_id").and_then(Value::as_str)
    {
        return Err("effect child spend id is not self-bound".into());
    }
    let decision_body = decision
        .get("body_json")
        .ok_or("effect parent decision body is missing")?;
    // Settlement is terminal reconciliation for an already-derived child. It
    // must retain a definitive or unknown outcome after the authorization
    // window closes; expiry is enforced at parent/child derivation instead.
    let envelope_hint: EffectEnvelopeContract = serde_json::from_value(
        decision_body
            .get("effect_envelope")
            .cloned()
            .ok_or("effect parent decision has no effect envelope")?,
    )
    .map_err(|error| format!("effect parent envelope is malformed: {error}"))?;
    let envelope = validate_effect_envelope_body(
        decision_body,
        principal.tenant_id.as_str(),
        None,
        &envelope_hint.created_at,
    )?;
    if parent_authorization
        .get("principal_id")
        .and_then(Value::as_str)
        != Some(envelope.owner_principal_id.as_str())
        || decision.get("principal_id").and_then(Value::as_str)
            != Some(envelope.owner_principal_id.as_str())
        || principal.principal_id != envelope.owner_principal_id
    {
        return Err("effect parent owner binding is stale or mismatched".into());
    }
    if decision.get("decision_id").and_then(Value::as_str) != Some(envelope.decision_id.as_str())
        || child.get("decision_id").and_then(Value::as_str) != Some(envelope.decision_id.as_str())
    {
        return Err("effect child decision binding is stale or mismatched".into());
    }
    if child.get("risk_authorization_id").and_then(Value::as_str)
        != parent_authorization
            .get("authorization_id")
            .and_then(Value::as_str)
    {
        return Err("effect child parent authorization binding is stale".into());
    }
    let parent_envelope_sha256 = envelope.sha256()?;
    if child.get("parent_envelope").map(sort_value) != Some(envelope.body())
        || child.get("parent_envelope_sha256").and_then(Value::as_str)
            != Some(parent_envelope_sha256.as_str())
    {
        return Err("effect child parent envelope hash binding is invalid".into());
    }
    if parent_authorization
        .pointer("/scope/effect_envelope")
        .map(sort_value)
        != Some(envelope.body())
    {
        return Err("effect parent authorization scope is stale or mismatched".into());
    }
    for (field, expected) in [
        ("effect_kind", envelope.effect_kind.as_str()),
        ("target_repository", envelope.target_repository.as_str()),
        ("target_main_sha", envelope.target_main_sha.as_str()),
    ] {
        if child.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!("effect child {field} is stale or mismatched"));
        }
    }
    let max_cost = child
        .get("max_cost_usd")
        .and_then(Value::as_f64)
        .ok_or("effect child max_cost_usd is missing")?;
    if !max_cost.is_finite() || max_cost < 0.0 || max_cost > envelope.max_total_cost_usd {
        return Err("effect child max_cost_usd exceeds parent envelope".into());
    }
    let child_expires_at = child
        .get("expires_at")
        .and_then(Value::as_str)
        .ok_or("effect child expires_at is missing")?;
    if require_finite_expiry(Some(child_expires_at))? > envelope.expires_at {
        return Err("effect child expiry exceeds parent envelope".into());
    }
    if child.get("one_use").and_then(Value::as_bool) != Some(true)
        || child.get("outcome_unknown_retry").and_then(Value::as_bool) != Some(false)
        || child.get("fixture_only").and_then(Value::as_bool) != Some(false)
    {
        return Err("effect child one-use or no-retry binding is invalid".into());
    }
    if child.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id.as_str())
        || child.get("principal_kind").and_then(Value::as_str)
            != Some(principal.principal_kind.as_str())
    {
        return Err("effect child owner binding is stale or mismatched".into());
    }
    Ok((child, envelope))
}

fn validate_effect_outcome_evidence(evidence: &Value) -> Result<Value, String> {
    let object = evidence
        .as_object()
        .ok_or("effect outcome evidence must be a digest object")?;
    if object.len() != 2
        || !object.contains_key("evidence")
        || !object.contains_key("evidence_sha256")
    {
        return Err("effect outcome evidence must contain evidence and evidence_sha256".into());
    }
    let payload = object
        .get("evidence")
        .and_then(Value::as_object)
        .ok_or("effect outcome evidence must contain a metadata object")?;
    if payload.len() != 3
        || !payload.contains_key("kind")
        || !payload.contains_key("child_authorization_id")
        || !payload.contains_key("effect_executed")
    {
        return Err("effect outcome evidence metadata schema is invalid".into());
    }
    if payload.get("kind").and_then(Value::as_str) != Some("provider_free_canary") {
        return Err("effect outcome evidence kind is not provider_free_canary".into());
    }
    let child_authorization_id = payload
        .get("child_authorization_id")
        .and_then(Value::as_str)
        .ok_or("effect outcome evidence child id is missing")?;
    if child_authorization_id.trim().is_empty()
        || child_authorization_id.chars().count() > 256
        || payload
            .get("effect_executed")
            .and_then(Value::as_bool)
            .is_none()
        || payload.get("effect_executed").and_then(Value::as_bool) != Some(false)
    {
        return Err("effect outcome evidence metadata values are invalid".into());
    }
    let payload = Value::Object(payload.clone());
    let payload_json = canonical_json(&payload)?;
    if payload_json.len() > MAX_EFFECT_OUTCOME_EVIDENCE_BYTES
        || contains_sensitive_patterns(&payload_json)
    {
        return Err("effect outcome evidence metadata is oversized or sensitive".into());
    }
    let digest = object
        .get("evidence_sha256")
        .and_then(Value::as_str)
        .ok_or("effect outcome evidence_sha256 is missing")?;
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("effect outcome evidence_sha256 must be 64 hex chars".into());
    }
    let expected = sha256_hex(payload_json.as_bytes());
    if digest != expected {
        return Err("effect outcome evidence_sha256 does not match evidence metadata".into());
    }
    Ok(sort_value(evidence))
}

fn effect_terminal_class(status: &str) -> &'static str {
    match status {
        "succeeded" => "effect_succeeded",
        "failed" => "effect_failed",
        EFFECT_OUTCOME_UNKNOWN => "effect_outcome_unknown",
        _ => "effect_invalid",
    }
}

fn effect_outcome_receipt(
    spend: &Value,
    child: &Value,
    attempt_id: &str,
    status: &str,
    evidence: &Value,
) -> Result<Value, String> {
    Ok(sort_value(&json!({
        "schema_version": "managed_effect_outcome_receipt.v1",
        "child_authorization_id": child.get("child_authorization_id"),
        "spend_authorization_id": spend.get("spend_authorization_id"),
        "parent_envelope_id": child.get("parent_envelope_id"),
        "attempt_id": attempt_id,
        "operation_id": child.get("operation_id"),
        "outcome": status,
        "evidence": evidence,
        "outcome_unknown_retry": false,
        "outcome_unknown_no_retry": status == EFFECT_OUTCOME_UNKNOWN,
    })))
}

fn effect_attempt_body(
    spend: &Value,
    child: &Value,
    attempt_id: &str,
    status: &str,
    terminal_class: &str,
    receipt_sha256: &str,
) -> Result<Value, String> {
    Ok(sort_value(&json!({
        "schema_version": "managed_effect_attempt.v1",
        "attempt_id": attempt_id,
        "child_authorization_id": child.get("child_authorization_id"),
        "spend_authorization_id": spend.get("spend_authorization_id"),
        "decision_id": spend.get("decision_id"),
        "risk_authorization_id": spend.get("risk_authorization_id"),
        "operation_id": child.get("operation_id"),
        "effect_kind": child.get("effect_kind"),
        "target_repository": child.get("target_repository"),
        "target_main_sha": child.get("target_main_sha"),
        "max_cost_usd": child.get("max_cost_usd"),
        "status": status,
        "terminal_class": terminal_class,
        "receipt_sha256": receipt_sha256,
        "one_use": true,
        "outcome_unknown_retry": false,
        "fixture_only": false,
    })))
}

#[cfg(feature = "pg")]
fn append_effect_audit_pg(
    tx: &mut postgres::Transaction<'_>,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> Result<(), String> {
    let details_json = details.to_string();
    tx.execute(
        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
         VALUES ($1, $2, $3, $4, $5)",
        &[&now, &actor, &action, &resource, &details_json],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn validate_decision_status(status: &str) -> Result<(), String> {
    match status {
        "draft_pending_operator"
        | "operator_accepted"
        | "operator_rejected"
        | "invalidated"
        | "revoked"
        | "expired" => Ok(()),
        other => Err(format!("invalid decision status {other}")),
    }
}

/// Finite expiry is mandatory. Parses RFC3339, normalizes to UTC `Z`, rejects far-future.
fn require_finite_expiry(expires_at: Option<&str>) -> Result<String, String> {
    let raw = expires_at
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("finite expires_at is mandatory for managed acceptance decisions")?;
    let dt = parse_rfc3339_utc("expires_at", raw)?;
    // Reject unbounded / placeholder far-future (UTC year).
    let year = dt.format("%Y").to_string();
    if year == "2099" || year == "9999" || year == "2100" {
        return Err("expires_at far-future placeholder is not a finite expiry".into());
    }
    let year_n: i32 = year
        .parse()
        .map_err(|_| "expires_at year unparseable".to_string())?;
    if !(2020..=2036).contains(&year_n) {
        return Err("expires_at year out of bounded finite range".into());
    }
    // Canonical UTC form so later comparisons are stable.
    Ok(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

fn validate_trial_envelope_shape(trial: &Value) -> Result<(), String> {
    if !trial.is_object() {
        return Err("trial_envelope must be an object".into());
    }
    for key in [
        "max_retries",
        "max_provider_requests",
        "max_input_tokens",
        "max_output_tokens",
        "max_total_tokens",
        "max_wall_time_ms",
        "provider_kind",
        "provider_host",
        "provider_base_url",
        "model",
        "admitted_endpoint_paths",
        "draft_pr_only",
        "auto_merge_disabled",
        "product_task_id",
        "workflow_id",
        "workflow_node_id",
        "execution_id",
        "attempt_id",
        "target_repo",
        "target_main_sha",
        "exact_codex_path",
        "exact_codex_sha256",
        "cancellation_identity",
        "rollback_identity",
        "output_branch_prefix",
        "cost_authority",
    ] {
        if trial.get(key).is_none() {
            return Err(format!("trial_envelope.{key} required"));
        }
    }
    Ok(())
}

/// Immutable decision authority hash: body + residual + finite expiry + invalidation state.
///
/// Mutable lifecycle status is **not** included. Status changes are recorded as separate
/// hash-linked transition receipts so the accepted decision hash stays stable.
pub fn canonical_decision_authority_hash(
    decision_body: &Value,
    residual_finding_sha256: &str,
    expires_at: &str,
    invalidation_state: &str,
) -> Result<String, String> {
    // Normalize expiry into the hash as canonical UTC RFC3339.
    let expires_at = require_finite_expiry(Some(expires_at))?;
    let envelope = sort_value(&json!({
        "schema_version": "managed_acceptance_decision_authority_envelope.v2",
        "decision_body": decision_body,
        "residual_finding_sha256": residual_finding_sha256,
        "expires_at": expires_at,
        "invalidation_state": invalidation_state,
    }));
    Ok(sha256_hex(canonical_json(&envelope)?.as_bytes()))
}

fn validate_attempt_terminal_status(status: &str) -> Result<(), String> {
    match status {
        "succeeded" | "failed" | "cancelled" | "outcome_unknown" | "replayed" => Ok(()),
        other => Err(format!("invalid attempt status {other}")),
    }
}

#[cfg(feature = "pg")]
fn parse_managed_acceptance_json(raw: &str, owner: &str) -> Result<Value, String> {
    serde_json::from_str(raw)
        .map_err(|error| format!("managed acceptance {owner} is invalid JSON: {error}"))
}

fn parse_managed_acceptance_sqlite_json(
    raw: &str,
    owner: &str,
    column: usize,
) -> rusqlite::Result<Value> {
    serde_json::from_str(raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("managed acceptance {owner} is invalid JSON: {error}"),
            )),
        )
    })
}

fn strict_managed_acceptance_sqlite_bool(
    value: i64,
    owner: &str,
    column: usize,
) -> rusqlite::Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "managed acceptance {owner} is not a persisted boolean (expected 0 or 1, got {other})"
                ),
            )),
        )),
    }
}

#[cfg(feature = "pg")]
fn strict_managed_acceptance_pg_bool(value: i32, owner: &str) -> Result<bool, String> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(format!(
            "managed acceptance {owner} is not a persisted boolean (expected 0 or 1, got {other})"
        )),
    }
}

fn persisted_body_field(body: &Value, owner: &str, field: &str) -> Result<Value, String> {
    body.get(field)
        .cloned()
        .ok_or_else(|| format!("{owner} body_json missing {field}"))
}

fn persisted_body_str(body: &Value, owner: &str, field: &str) -> Result<String, String> {
    persisted_body_field(body, owner, field)?
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{owner} body_json {field} must be a non-empty string"))
}

fn validate_decision_owner_integrity(decision: &Value) -> Result<(), String> {
    let owner = "managed acceptance decision";
    let body = decision
        .get("body_json")
        .ok_or_else(|| format!("{owner} body_json is missing"))?;
    let decision_id = required_str(decision, "decision_id")?;
    if persisted_body_str(body, owner, "decision_id")? != decision_id {
        return Err(format!(
            "{owner} body decision_id does not match the owner row"
        ));
    }
    if persisted_body_str(body, owner, "status")? != "draft_pending_operator" {
        return Err(format!(
            "{owner} body status is not the immutable draft status"
        ));
    }
    let residual = required_str(decision, "residual_finding_sha256")?;
    let expires_at = required_str(decision, "expires_at")?;
    let invalidation_state = match body.get("invalidation_state") {
        None => "none",
        Some(Value::String(value)) => value.as_str(),
        Some(_) => return Err(format!("{owner} body invalidation_state must be a string")),
    };
    let recomputed =
        canonical_decision_authority_hash(body, &residual, &expires_at, invalidation_state)?;
    if required_str(decision, "decision_body_sha256")? != recomputed {
        return Err(format!(
            "{owner} decision_body_sha256 does not match body_json"
        ));
    }
    Ok(())
}

fn validate_authorization_owner_integrity(authorization: &Value) -> Result<(), String> {
    let owner = "managed acceptance authorization";
    let body = authorization
        .get("body_json")
        .ok_or_else(|| format!("{owner} body_json is missing"))?;
    for field in [
        "authorization_id",
        "decision_id",
        "tenant_id",
        "principal_kind",
        "principal_id",
        "decision_body_sha256",
        "residual_finding_sha256",
        "mutation_authority",
        "expires_at",
    ] {
        if persisted_body_str(body, owner, field)? != required_str(authorization, field)? {
            return Err(format!(
                "{owner} body_json {field} does not match the owner row"
            ));
        }
    }
    let principal_kind = PrincipalKind::parse(&required_str(authorization, "principal_kind")?)?;
    let body_scope = persisted_body_field(body, owner, "scope")?;
    if sort_value(&body_scope)
        != sort_value(
            authorization
                .get("scope")
                .ok_or_else(|| format!("{owner} scope is missing"))?,
        )
    {
        return Err(format!("{owner} scope does not match body_json"));
    }
    let body_execution_granted = persisted_body_field(body, owner, "execution_granted")?
        .as_bool()
        .ok_or_else(|| format!("{owner} body execution_granted must be a boolean"))?;
    if authorization
        .get("execution_granted")
        .and_then(Value::as_bool)
        != Some(body_execution_granted)
    {
        return Err(format!(
            "{owner} execution_granted is not a persisted boolean match"
        ));
    }
    let body_fixture_only = persisted_body_field(body, owner, "fixture_only")?
        .as_bool()
        .ok_or_else(|| format!("{owner} body fixture_only must be a boolean"))?;
    if authorization.get("fixture_only").and_then(Value::as_bool) != Some(body_fixture_only)
        || body_fixture_only != (principal_kind == PrincipalKind::FixturePrincipal)
    {
        return Err(format!(
            "{owner} fixture_only is not a persisted boolean match"
        ));
    }
    let body_sha = sha256_hex(canonical_json(body)?.as_bytes());
    if required_str(authorization, "authorization_sha256")? != body_sha {
        return Err(format!(
            "{owner} authorization_sha256 does not match body_json"
        ));
    }
    Ok(())
}

fn validate_spend_owner_integrity(spend: &Value) -> Result<(), String> {
    let owner = "managed acceptance spend authorization";
    let body = spend
        .get("body_json")
        .ok_or_else(|| format!("{owner} body_json is missing"))?;
    for field in [
        "spend_authorization_id",
        "decision_id",
        "risk_authorization_id",
        "tenant_id",
        "principal_kind",
        "principal_id",
        "risk_authorization_sha256",
        "decision_body_sha256",
        "residual_finding_sha256",
        "expires_at",
    ] {
        if persisted_body_str(body, owner, field)? != required_str(spend, field)? {
            return Err(format!(
                "{owner} body_json {field} does not match the owner row"
            ));
        }
    }
    let principal_kind = PrincipalKind::parse(&required_str(spend, "principal_kind")?)?;
    let body_fixture_only = persisted_body_field(body, owner, "fixture_only")?
        .as_bool()
        .ok_or_else(|| format!("{owner} body fixture_only must be a boolean"))?;
    if spend.get("fixture_only").and_then(Value::as_bool) != Some(body_fixture_only)
        || body_fixture_only != (principal_kind == PrincipalKind::FixturePrincipal)
    {
        return Err(format!(
            "{owner} fixture_only is not a persisted boolean match"
        ));
    }
    if persisted_body_field(body, owner, "one_use")?.as_bool() != Some(true) {
        return Err(format!(
            "{owner} body one_use must be the persisted true boolean"
        ));
    }
    let body_sha = sha256_hex(canonical_json(body)?.as_bytes());
    if required_str(spend, "spend_body_sha256")? != body_sha {
        return Err(format!(
            "{owner} spend_body_sha256 does not match body_json"
        ));
    }
    if let Some(logical) = spend.get("logical_authorization_sha256") {
        let logical = logical
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{owner} logical_authorization_sha256 is invalid"))?;
        if body
            .get("logical_authorization_sha256")
            .and_then(Value::as_str)
            != Some(logical)
        {
            return Err(format!(
                "{owner} logical_authorization_sha256 does not match body_json"
            ));
        }
        if stable_spend_authorization_identity(body)? != logical {
            return Err(format!(
                "{owner} logical_authorization_sha256 does not match the stable body identity"
            ));
        }
    } else if spend.get("status").and_then(Value::as_str) == Some("active") {
        return Err(format!(
            "{owner} active row is missing logical_authorization_sha256"
        ));
    }
    Ok(())
}

fn load_decision_sqlite(conn: &rusqlite::Connection, decision_id: &str) -> Result<Value, String> {
    conn.query_row(
        "SELECT decision_id, tenant_id, decision_body_sha256, residual_finding_sha256, status,
                principal_kind, principal_id, body_json, created_at, updated_at, expires_at, revoked_at
         FROM managed_acceptance_decisions WHERE decision_id=?1",
        params![decision_id],
        |row| {
            let decision = json!({
                "schema_version": "managed_acceptance_decision.v1",
                "decision_id": row.get::<_, String>(0)?,
                "tenant_id": row.get::<_, String>(1)?,
                "decision_body_sha256": row.get::<_, String>(2)?,
                "residual_finding_sha256": row.get::<_, String>(3)?,
                "status": row.get::<_, String>(4)?,
                "principal_kind": row.get::<_, String>(5)?,
                "principal_id": row.get::<_, Option<String>>(6)?,
                "body_json": parse_managed_acceptance_sqlite_json(
                    &row.get::<_, String>(7)?,
                    "decision body_json",
                    7,
                )?,
                "created_at": row.get::<_, String>(8)?,
                "updated_at": row.get::<_, String>(9)?,
                "expires_at": row.get::<_, Option<String>>(10)?,
                "revoked_at": row.get::<_, Option<String>>(11)?,
            });
            validate_decision_owner_integrity(&decision)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(error))))?;
            Ok(decision)
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn load_decision_pg(
    client: &mut impl postgres::GenericClient,
    decision_id: &str,
) -> Result<Value, String> {
    let row = client
        .query_one(
            "SELECT decision_id, tenant_id, decision_body_sha256, residual_finding_sha256, status,
                    principal_kind, principal_id, body_json, created_at, updated_at, expires_at, revoked_at
             FROM managed_acceptance_decisions WHERE decision_id=$1",
            &[&decision_id],
        )
        .map_err(|e| e.to_string())?;
    let body_s: String = row.get(7);
    let body = parse_managed_acceptance_json(&body_s, "decision body_json")?;
    let decision = json!({
        "schema_version": "managed_acceptance_decision.v1",
        "decision_id": row.get::<_, String>(0),
        "tenant_id": row.get::<_, String>(1),
        "decision_body_sha256": row.get::<_, String>(2),
        "residual_finding_sha256": row.get::<_, String>(3),
        "status": row.get::<_, String>(4),
        "principal_kind": row.get::<_, String>(5),
        "principal_id": row.get::<_, Option<String>>(6),
        "body_json": body,
        "created_at": row.get::<_, String>(8),
        "updated_at": row.get::<_, String>(9),
        "expires_at": row.get::<_, Option<String>>(10),
        "revoked_at": row.get::<_, Option<String>>(11),
    });
    validate_decision_owner_integrity(&decision)?;
    Ok(decision)
}

fn load_authorization_sqlite(
    conn: &rusqlite::Connection,
    authorization_id: &str,
) -> Result<Value, String> {
    conn.query_row(
        "SELECT authorization_id, decision_id, tenant_id, principal_kind, principal_id,
                decision_body_sha256, residual_finding_sha256, authorization_sha256,
                scope_json, status, mutation_authority, execution_granted, body_json,
                created_at, updated_at, expires_at, revoked_at
         FROM managed_acceptance_authorizations WHERE authorization_id=?1",
        params![authorization_id],
        |row| {
            let body_s: String = row.get(12)?;
            let body =
                parse_managed_acceptance_sqlite_json(&body_s, "authorization body_json", 12)?;
            let scope = parse_managed_acceptance_sqlite_json(
                &row.get::<_, String>(8)?,
                "authorization scope_json",
                8,
            )?;
            let authorization = json!({
                "schema_version": "managed_acceptance_authorization.v1",
                "authorization_id": row.get::<_, String>(0)?,
                "decision_id": row.get::<_, String>(1)?,
                "tenant_id": row.get::<_, String>(2)?,
                "principal_kind": row.get::<_, String>(3)?,
                "principal_id": row.get::<_, String>(4)?,
                "decision_body_sha256": row.get::<_, String>(5)?,
                "residual_finding_sha256": row.get::<_, String>(6)?,
                "authorization_sha256": row.get::<_, String>(7)?,
                "scope": scope,
                "status": row.get::<_, String>(9)?,
                "mutation_authority": row.get::<_, String>(10)?,
                "execution_granted": strict_managed_acceptance_sqlite_bool(
                    row.get::<_, i64>(11)?,
                    "authorization execution_granted",
                    11,
                )?,
                "body_json": body,
                "fixture_only": body.get("fixture_only").cloned().unwrap_or(Value::Null),
                "created_at": row.get::<_, String>(13)?,
                "updated_at": row.get::<_, String>(14)?,
                "expires_at": row.get::<_, String>(15)?,
                "revoked_at": row.get::<_, Option<String>>(16)?,
            });
            validate_authorization_owner_integrity(&authorization).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    12,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::other(error)),
                )
            })?;
            Ok(authorization)
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn load_authorization_pg(
    client: &mut impl postgres::GenericClient,
    authorization_id: &str,
) -> Result<Value, String> {
    let row = client
        .query_one(
            "SELECT authorization_id, decision_id, tenant_id, principal_kind, principal_id,
                    decision_body_sha256, residual_finding_sha256, authorization_sha256,
                    scope_json, status, mutation_authority, execution_granted, body_json,
                    created_at, updated_at, expires_at, revoked_at
             FROM managed_acceptance_authorizations WHERE authorization_id=$1",
            &[&authorization_id],
        )
        .map_err(|e| e.to_string())?;
    let body_s: String = row.get(12);
    let body = parse_managed_acceptance_json(&body_s, "authorization body_json")?;
    let scope_s: String = row.get(8);
    let scope = parse_managed_acceptance_json(&scope_s, "authorization scope_json")?;
    let exec: i32 = row.get(11);
    let execution_granted =
        strict_managed_acceptance_pg_bool(exec, "authorization execution_granted")?;
    let authorization = json!({
        "schema_version": "managed_acceptance_authorization.v1",
        "authorization_id": row.get::<_, String>(0),
        "decision_id": row.get::<_, String>(1),
        "tenant_id": row.get::<_, String>(2),
        "principal_kind": row.get::<_, String>(3),
        "principal_id": row.get::<_, String>(4),
        "decision_body_sha256": row.get::<_, String>(5),
        "residual_finding_sha256": row.get::<_, String>(6),
        "authorization_sha256": row.get::<_, String>(7),
        "scope": scope,
        "status": row.get::<_, String>(9),
        "mutation_authority": row.get::<_, String>(10),
        "execution_granted": execution_granted,
        "body_json": body,
        "fixture_only": body.get("fixture_only").cloned().unwrap_or(Value::Null),
        "created_at": row.get::<_, String>(13),
        "updated_at": row.get::<_, String>(14),
        "expires_at": row.get::<_, String>(15),
        "revoked_at": row.get::<_, Option<String>>(16),
    });
    validate_authorization_owner_integrity(&authorization)?;
    Ok(authorization)
}

fn load_spend_sqlite(conn: &rusqlite::Connection, spend_id: &str) -> Result<Value, String> {
    conn.query_row(
        "SELECT spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
                principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
                logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                fixture_only, status, body_json,
                created_at, updated_at, expires_at, consumed_at, consumed_by_attempt_id, revoked_at
         FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=?1",
        params![spend_id],
        |row| {
            let body_s: String = row.get(13)?;
            let body = parse_managed_acceptance_sqlite_json(&body_s, "spend body_json", 13)?;
            let spend = json!({
                "schema_version": "managed_acceptance_spend_authorization.v1",
                "spend_authorization_id": row.get::<_, String>(0)?,
                "decision_id": row.get::<_, String>(1)?,
                "risk_authorization_id": row.get::<_, String>(2)?,
                "tenant_id": row.get::<_, String>(3)?,
                "principal_kind": row.get::<_, String>(4)?,
                "principal_id": row.get::<_, String>(5)?,
                "spend_body_sha256": row.get::<_, String>(6)?,
                "risk_authorization_sha256": row.get::<_, String>(7)?,
                "logical_authorization_sha256": row.get::<_, Option<String>>(8)?,
                "decision_body_sha256": row.get::<_, String>(9)?,
                "residual_finding_sha256": row.get::<_, String>(10)?,
                "fixture_only": strict_managed_acceptance_sqlite_bool(
                    row.get::<_, i64>(11)?,
                    "spend fixture_only",
                    11,
                )?,
                "status": row.get::<_, String>(12)?,
                "body_json": body,
                "created_at": row.get::<_, String>(14)?,
                "updated_at": row.get::<_, String>(15)?,
                "expires_at": row.get::<_, String>(16)?,
                "consumed_at": row.get::<_, Option<String>>(17)?,
                "consumed_by_attempt_id": row.get::<_, Option<String>>(18)?,
                "revoked_at": row.get::<_, Option<String>>(19)?,
            });
            validate_spend_owner_integrity(&spend).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    13,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::other(error)),
                )
            })?;
            Ok(spend)
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn load_spend_pg(
    client: &mut impl postgres::GenericClient,
    spend_id: &str,
) -> Result<Value, String> {
    let row = client
        .query_one(
            "SELECT spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
                    principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
                    logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                    fixture_only, status, body_json,
                    created_at, updated_at, expires_at, consumed_at, consumed_by_attempt_id, revoked_at
             FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=$1",
            &[&spend_id],
        )
        .map_err(|e| e.to_string())?;
    let body_s: String = row.get(13);
    let body = parse_managed_acceptance_json(&body_s, "spend body_json")?;
    let fixture_only =
        strict_managed_acceptance_pg_bool(row.get::<_, i32>(11), "spend fixture_only")?;
    let spend = json!({
        "schema_version": "managed_acceptance_spend_authorization.v1",
        "spend_authorization_id": row.get::<_, String>(0),
        "decision_id": row.get::<_, String>(1),
        "risk_authorization_id": row.get::<_, String>(2),
        "tenant_id": row.get::<_, String>(3),
        "principal_kind": row.get::<_, String>(4),
        "principal_id": row.get::<_, String>(5),
        "spend_body_sha256": row.get::<_, String>(6),
        "risk_authorization_sha256": row.get::<_, String>(7),
        "logical_authorization_sha256": row.get::<_, Option<String>>(8),
        "decision_body_sha256": row.get::<_, String>(9),
        "residual_finding_sha256": row.get::<_, String>(10),
        "fixture_only": fixture_only,
        "status": row.get::<_, String>(12),
        "body_json": body,
        "created_at": row.get::<_, String>(14),
        "updated_at": row.get::<_, String>(15),
        "expires_at": row.get::<_, String>(16),
        "consumed_at": row.get::<_, Option<String>>(17),
        "consumed_by_attempt_id": row.get::<_, Option<String>>(18),
        "revoked_at": row.get::<_, Option<String>>(19),
    });
    validate_spend_owner_integrity(&spend)?;
    Ok(spend)
}

fn load_attempt_sqlite(conn: &rusqlite::Connection, attempt_id: &str) -> Result<Value, String> {
    conn.query_row(
        "SELECT attempt_id, tenant_id, product_task_id, workflow_node_id, execution_id,
                decision_id, authorization_id, spend_authorization_id, manifest_sha256, attempt_body_sha256,
                status, terminal_class, body_json, receipt_json, receipt_sha256, lease_token, created_at, updated_at
         FROM managed_acceptance_attempts WHERE attempt_id=?1",
        params![attempt_id],
        |row| {
            let body = parse_managed_acceptance_sqlite_json(
                &row.get::<_, String>(12)?,
                "attempt body_json",
                12,
            )?;
            let receipt = match row.get::<_, Option<String>>(13)? {
                Some(raw) => Some(parse_managed_acceptance_sqlite_json(
                    &raw,
                    "attempt receipt_json",
                    13,
                )?),
                None => None,
            };
            Ok(redact_lease_fields(json!({
                "schema_version": "managed_acceptance_attempt.v1",
                "attempt_id": row.get::<_, String>(0)?,
                "tenant_id": row.get::<_, String>(1)?,
                "product_task_id": row.get::<_, Option<String>>(2)?,
                "workflow_node_id": row.get::<_, Option<String>>(3)?,
                "execution_id": row.get::<_, String>(4)?,
                "decision_id": row.get::<_, String>(5)?,
                "authorization_id": row.get::<_, String>(6)?,
                "spend_authorization_id": row.get::<_, Option<String>>(7)?,
                "manifest_sha256": row.get::<_, String>(8)?,
                "attempt_body_sha256": row.get::<_, String>(9)?,
                "status": row.get::<_, String>(10)?,
                "terminal_class": row.get::<_, Option<String>>(11)?,
                "body_json": body,
                "receipt_json": receipt,
                "receipt_sha256": row.get::<_, Option<String>>(14)?,
                "created_at": row.get::<_, String>(16)?,
                "updated_at": row.get::<_, String>(17)?,
                "idempotent_replay": false,
            })))
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn load_attempt_pg(
    client: &mut impl postgres::GenericClient,
    attempt_id: &str,
) -> Result<Value, String> {
    let row = client
        .query_one(
            "SELECT attempt_id, tenant_id, product_task_id, workflow_node_id, execution_id,
                    decision_id, authorization_id, spend_authorization_id, manifest_sha256, attempt_body_sha256,
                    status, terminal_class, body_json, receipt_json, receipt_sha256, lease_token, created_at, updated_at
             FROM managed_acceptance_attempts WHERE attempt_id=$1",
            &[&attempt_id],
        )
        .map_err(|e| e.to_string())?;
    let body_s: String = row.get(12);
    let receipt_s: Option<String> = row.get(13);
    let body = parse_managed_acceptance_json(&body_s, "attempt body_json")?;
    let receipt = receipt_s
        .as_deref()
        .map(|raw| parse_managed_acceptance_json(raw, "attempt receipt_json"))
        .transpose()?;
    Ok(redact_lease_fields(json!({
        "schema_version": "managed_acceptance_attempt.v1",
        "attempt_id": row.get::<_, String>(0),
        "tenant_id": row.get::<_, String>(1),
        "product_task_id": row.get::<_, Option<String>>(2),
        "workflow_node_id": row.get::<_, Option<String>>(3),
        "execution_id": row.get::<_, String>(4),
        "decision_id": row.get::<_, String>(5),
        "authorization_id": row.get::<_, String>(6),
        "spend_authorization_id": row.get::<_, Option<String>>(7),
        "manifest_sha256": row.get::<_, String>(8),
        "attempt_body_sha256": row.get::<_, String>(9),
        "status": row.get::<_, String>(10),
        "terminal_class": row.get::<_, Option<String>>(11),
        "body_json": body,
        "receipt_json": receipt,
        "receipt_sha256": row.get::<_, Option<String>>(14),
        "created_at": row.get::<_, String>(16),
        "updated_at": row.get::<_, String>(17),
        "idempotent_replay": false,
    })))
}

fn redact_lease_fields(mut value: Value) -> Value {
    match &mut value {
        Value::Object(object) => {
            object.remove("lease_token");
            for child in object.values_mut() {
                *child = redact_lease_fields(std::mem::take(child));
            }
        }
        Value::Array(items) => {
            for child in items {
                *child = redact_lease_fields(std::mem::take(child));
            }
        }
        _ => {}
    }
    value
}

fn open_absolute_path_without_symlinks(
    path: &Path,
    final_access_flags: libc::c_int,
) -> Result<std::fs::File, String> {
    if !path.is_absolute() {
        return Err("managed workspace path must be absolute".into());
    }
    let components = path
        .components()
        .filter_map(|component| match component {
            Component::RootDir => None,
            Component::Normal(value) => Some(Ok(value)),
            _ => Some(Err(
                "managed workspace path contains a non-normal component".to_string(),
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.is_empty() {
        return Err("managed workspace target cannot be the filesystem root".into());
    }
    let mut current = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/")
        .map_err(|error| format!("managed workspace root open failed: {error}"))?;
    for (index, component) in components.iter().enumerate() {
        let component = CString::new(component.as_bytes())
            .map_err(|_| "managed workspace path contains NUL".to_string())?;
        let is_final = index + 1 == components.len();
        let flags = if is_final {
            final_access_flags | libc::O_NOFOLLOW | libc::O_CLOEXEC
        } else {
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC
        };
        let fd = unsafe { libc::openat(current.as_raw_fd(), component.as_ptr(), flags, 0) };
        if fd < 0 {
            return Err(format!(
                "managed workspace descriptor traversal failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        current = unsafe { std::fs::File::from_raw_fd(fd) };
    }
    Ok(current)
}

#[derive(Debug, Clone)]
struct ManagedSchedulerLeaseAuthority {
    run_id: String,
    attempt_count: i64,
    lease_owner_token_sha256: String,
    allowed_paths: Value,
    workspace_path: String,
}

fn managed_scheduler_lease_owner_token_sha256(
    run_id: &str,
    node_id: &str,
    attempt_count: i64,
    leased_at: &str,
) -> String {
    sha256_hex(
        format!("managed_scheduler_node_lease.v1:{run_id}:{node_id}:{attempt_count}:{leased_at}")
            .as_bytes(),
    )
}

fn validate_managed_scheduler_lease_values(
    binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    task_status: &str,
    run_id: Option<String>,
    workspace_binding_json: Option<String>,
    run_status: &str,
    workflow_id: &str,
    node_status: &str,
    attempt_count: i64,
    leased_at: Option<String>,
) -> Result<ManagedSchedulerLeaseAuthority, String> {
    if !matches!(task_status, "graph_ready" | "running") {
        return Err("managed scheduler ProductTask is cancelled, killed, or not executable".into());
    }
    let run_id = run_id.ok_or("managed scheduler ProductTask run is missing")?;
    let workspace_binding: Value = serde_json::from_str(
        workspace_binding_json
            .as_deref()
            .ok_or("managed scheduler ProductTask workspace binding is missing")?,
    )
    .map_err(|_| "managed scheduler ProductTask workspace binding is invalid")?;
    let allowed_paths = workspace_binding
        .get("allowed_paths")
        .cloned()
        .ok_or("managed scheduler ProductTask allowed paths are missing")?;
    let workspace_path = workspace_binding
        .get("workspace_path")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("managed scheduler ProductTask workspace is missing")?
        .to_string();
    let leased_at = leased_at.ok_or("managed scheduler node lease is missing")?;
    if run_status != "running"
        || workflow_id != binding.workflow_id
        || node_status != "running"
        || attempt_count <= 0
    {
        return Err("managed scheduler execution lease is stale, cancelled, or replaced".into());
    }
    Ok(ManagedSchedulerLeaseAuthority {
        lease_owner_token_sha256: managed_scheduler_lease_owner_token_sha256(
            &run_id,
            &binding.node_id,
            attempt_count,
            &leased_at,
        ),
        run_id,
        attempt_count,
        allowed_paths,
        workspace_path,
    })
}

fn load_direct_delegated_workspace_lease_sqlite(
    tx: &rusqlite::Transaction<'_>,
    binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    workspace_binding_json: Option<String>,
) -> Result<ManagedSchedulerLeaseAuthority, String> {
    let (
        delegation_status,
        _expires_at,
        spend_status,
        attempt_status,
        spend_id,
        attempt_lease_token,
        manifest_json,
    ): (String, String, String, String, String, String, String) = tx
        .query_row(
            "SELECT status, expires_at, spend_status, attempt_status,
                    spend_authorization_id, attempt_lease_token, manifest_json
             FROM managed_acceptance_delegations WHERE attempt_id=?1",
            params![binding.attempt_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    if delegation_status != "active"
        || spend_status != "consumed"
        || attempt_status != "admitted"
        || spend_id != binding.spend_authorization_id
        || crate::provider::managed_deepseek::managed_attempt_lease_id(&attempt_lease_token)
            != binding.attempt_lease_id
    {
        return Err("managed delegated attempt lease is stale, cancelled, or replaced".into());
    }
    let manifest: Value = serde_json::from_str(&manifest_json)
        .map_err(|_| "managed delegated manifest is invalid".to_string())?;
    if manifest
        .pointer("/execution/workflow_id")
        .and_then(Value::as_str)
        != Some(binding.workflow_id.as_str())
    {
        return Err("managed delegated attempt workflow_id mismatch".into());
    }
    if !manifest
        .pointer("/execution/workflow_node_ids")
        .and_then(Value::as_array)
        .is_some_and(|nodes| {
            nodes
                .iter()
                .any(|n| n.as_str() == Some(binding.node_id.as_str()))
        })
    {
        return Err("managed delegated attempt node_id mismatch".into());
    }
    let workspace_binding: Value = serde_json::from_str(
        workspace_binding_json
            .as_deref()
            .ok_or("managed scheduler ProductTask workspace binding is missing")?,
    )
    .map_err(|_| "managed scheduler ProductTask workspace binding is invalid")?;
    let allowed_paths = workspace_binding
        .get("allowed_paths")
        .cloned()
        .ok_or("managed scheduler ProductTask allowed paths are missing")?;
    let workspace_path = workspace_binding
        .get("workspace_path")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("managed scheduler ProductTask workspace is missing")?
        .to_string();
    let lease_owner_token_sha256 = sha256_hex(
        format!(
            "managed_scheduler_node_lease.v1:{}:{}:1:{}",
            binding.product_task_id, binding.node_id, binding.attempt_lease_id
        )
        .as_bytes(),
    );
    Ok(ManagedSchedulerLeaseAuthority {
        lease_owner_token_sha256,
        run_id: format!("delegated:{}", binding.product_task_id),
        attempt_count: 1,
        allowed_paths,
        workspace_path,
    })
}

fn load_managed_scheduler_lease_sqlite(
    tx: &rusqlite::Transaction<'_>,
    binding: &crate::provider::managed_deepseek::ManagedCallBinding,
) -> Result<ManagedSchedulerLeaseAuthority, String> {
    let (task_status, run_id, workspace_binding_json): (String, Option<String>, Option<String>) =
        tx.query_row(
            "SELECT status, run_id, workspace_binding_json
             FROM product_tasks WHERE task_id=?1",
            params![binding.product_task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| error.to_string())?;
    if task_status == "workspace_bound" {
        return load_direct_delegated_workspace_lease_sqlite(tx, binding, workspace_binding_json);
    }
    let run_id_ref = run_id
        .as_deref()
        .ok_or("managed scheduler ProductTask run is missing")?;
    let (run_status, workflow_id): (String, String) = tx
        .query_row(
            "SELECT status, workflow_id FROM workflow_runs WHERE run_id=?1",
            params![run_id_ref],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    let (node_status, attempt_count, leased_at): (String, i64, Option<String>) = tx
        .query_row(
            "SELECT status, attempt_count, leased_at
             FROM workflow_run_nodes WHERE run_id=?1 AND node_id=?2",
            params![run_id_ref, binding.node_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| error.to_string())?;
    validate_managed_scheduler_lease_values(
        binding,
        &task_status,
        run_id,
        workspace_binding_json,
        &run_status,
        &workflow_id,
        &node_status,
        attempt_count,
        leased_at,
    )
}

#[cfg(feature = "pg")]
fn load_direct_delegated_workspace_lease_pg(
    tx: &mut postgres::Transaction<'_>,
    binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    workspace_binding_json: Option<String>,
) -> Result<ManagedSchedulerLeaseAuthority, String> {
    let row = tx
        .query_one(
            "SELECT status, expires_at, spend_status, attempt_status,
                    spend_authorization_id, attempt_lease_token, manifest_json
             FROM managed_acceptance_delegations WHERE attempt_id=$1 FOR UPDATE",
            &[&binding.attempt_id],
        )
        .map_err(|error| error.to_string())?;
    let delegation_status: String = row.get(0);
    let _expires_at: String = row.get(1);
    let spend_status: String = row.get(2);
    let attempt_status: String = row.get(3);
    let spend_id: String = row.get(4);
    let attempt_lease_token: String = row.get(5);
    let manifest_json: String = row.get(6);
    if delegation_status != "active"
        || spend_status != "consumed"
        || attempt_status != "admitted"
        || spend_id != binding.spend_authorization_id
        || crate::provider::managed_deepseek::managed_attempt_lease_id(&attempt_lease_token)
            != binding.attempt_lease_id
    {
        return Err("managed delegated attempt lease is stale, cancelled, or replaced".into());
    }
    let manifest: Value = serde_json::from_str(&manifest_json)
        .map_err(|_| "managed delegated manifest is invalid".to_string())?;
    if manifest
        .pointer("/execution/workflow_id")
        .and_then(Value::as_str)
        != Some(binding.workflow_id.as_str())
    {
        return Err("managed delegated attempt workflow_id mismatch".into());
    }
    if !manifest
        .pointer("/execution/workflow_node_ids")
        .and_then(Value::as_array)
        .is_some_and(|nodes| {
            nodes
                .iter()
                .any(|n| n.as_str() == Some(binding.node_id.as_str()))
        })
    {
        return Err("managed delegated attempt node_id mismatch".into());
    }
    let workspace_binding: Value = serde_json::from_str(
        workspace_binding_json
            .as_deref()
            .ok_or("managed scheduler ProductTask workspace binding is missing")?,
    )
    .map_err(|_| "managed scheduler ProductTask workspace binding is invalid")?;
    let allowed_paths = workspace_binding
        .get("allowed_paths")
        .cloned()
        .ok_or("managed scheduler ProductTask allowed paths are missing")?;
    let workspace_path = workspace_binding
        .get("workspace_path")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("managed scheduler ProductTask workspace is missing")?
        .to_string();
    let lease_owner_token_sha256 = sha256_hex(
        format!(
            "managed_scheduler_node_lease.v1:{}:{}:1:{}",
            binding.product_task_id, binding.node_id, binding.attempt_lease_id
        )
        .as_bytes(),
    );
    Ok(ManagedSchedulerLeaseAuthority {
        lease_owner_token_sha256,
        run_id: format!("delegated:{}", binding.product_task_id),
        attempt_count: 1,
        allowed_paths,
        workspace_path,
    })
}

#[cfg(feature = "pg")]
fn load_managed_scheduler_lease_pg(
    tx: &mut postgres::Transaction<'_>,
    binding: &crate::provider::managed_deepseek::ManagedCallBinding,
) -> Result<ManagedSchedulerLeaseAuthority, String> {
    let task = tx
        .query_one(
            "SELECT status, run_id, workspace_binding_json
             FROM product_tasks WHERE task_id=$1 FOR UPDATE",
            &[&binding.product_task_id],
        )
        .map_err(|error| error.to_string())?;
    let task_status: String = task.get(0);
    let run_id: Option<String> = task.get(1);
    let workspace_binding_json: Option<String> = task.get(2);
    if task_status == "workspace_bound" {
        return load_direct_delegated_workspace_lease_pg(tx, binding, workspace_binding_json);
    }
    let run_id_ref = run_id
        .as_deref()
        .ok_or("managed scheduler ProductTask run is missing")?;
    let run = tx
        .query_one(
            "SELECT status, workflow_id FROM workflow_runs WHERE run_id=$1 FOR UPDATE",
            &[&run_id_ref],
        )
        .map_err(|error| error.to_string())?;
    let run_status: String = run.get(0);
    let workflow_id: String = run.get(1);
    let node = tx
        .query_one(
            "SELECT status, attempt_count, leased_at
             FROM workflow_run_nodes WHERE run_id=$1 AND node_id=$2 FOR UPDATE",
            &[&run_id_ref, &binding.node_id],
        )
        .map_err(|error| error.to_string())?;
    let node_status: String = node.get(0);
    let attempt_count = i64::from(node.get::<_, i32>(1));
    let leased_at: Option<String> = node.get(2);
    validate_managed_scheduler_lease_values(
        binding,
        &task_status,
        run_id,
        workspace_binding_json,
        &run_status,
        &workflow_id,
        &node_status,
        attempt_count,
        leased_at,
    )
}

fn validate_workspace_provider_journal(
    journal_json: &str,
    binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    scheduler: &ManagedSchedulerLeaseAuthority,
) -> Result<(), String> {
    let journal: Vec<Value> = serde_json::from_str(journal_json)
        .map_err(|_| "managed workspace provider journal is invalid")?;
    let matching = journal
        .iter()
        .filter(|entry| {
            entry.get("node_id").and_then(Value::as_str) == Some(binding.node_id.as_str())
        })
        .collect::<Vec<_>>();
    let entry = match matching.as_slice() {
        [entry] => *entry,
        [] => return Err("managed workspace provider request claim is missing".into()),
        _ => return Err("managed workspace provider request claim is duplicated".into()),
    };
    if entry.get("status").and_then(Value::as_str) != Some("succeeded")
        || entry.get("role").and_then(Value::as_str) != Some("implementer")
        || entry.get("requested_model").and_then(Value::as_str) != Some("deepseek-v4-flash")
        || entry
            .pointer("/scheduler_lease/product_task_id")
            .and_then(Value::as_str)
            != Some(binding.product_task_id.as_str())
        || entry
            .pointer("/scheduler_lease/run_id")
            .and_then(Value::as_str)
            != Some(scheduler.run_id.as_str())
        || entry
            .pointer("/scheduler_lease/workflow_id")
            .and_then(Value::as_str)
            != Some(binding.workflow_id.as_str())
        || entry
            .pointer("/scheduler_lease/node_id")
            .and_then(Value::as_str)
            != Some(binding.node_id.as_str())
        || entry
            .pointer("/scheduler_lease/attempt_count")
            .and_then(Value::as_i64)
            != Some(scheduler.attempt_count)
        || entry
            .pointer("/scheduler_lease/lease_owner_token_sha256")
            .and_then(Value::as_str)
            != Some(scheduler.lease_owner_token_sha256.as_str())
    {
        return Err(
            "managed workspace provider response belongs to a stale or replaced scheduler lease"
                .into(),
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_delegated_workspace_authority(
    now: &str,
    binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    delegation_status: &str,
    expires_at: &str,
    spend_status: &str,
    attempt_status: &str,
    spend_authorization_id: &str,
    attempt_lease_token: &str,
    manifest_json: &str,
    journal_json: &str,
    scheduler: &ManagedSchedulerLeaseAuthority,
) -> Result<(), String> {
    if delegation_status != "active"
        || is_at_or_before(expires_at, now)?
        || spend_status != "consumed"
        || attempt_status != "admitted"
        || spend_authorization_id != binding.spend_authorization_id
        || crate::provider::managed_deepseek::managed_attempt_lease_id(attempt_lease_token)
            != binding.attempt_lease_id
    {
        return Err("managed workspace delegated attempt lease is stale or closed".into());
    }
    let manifest: Value = serde_json::from_str(manifest_json)
        .map_err(|_| "managed workspace final manifest is invalid")?;
    if manifest.get("manifest_sha256").and_then(Value::as_str)
        != Some(compute_attempt_manifest_sha256(&manifest)?.as_str())
        || manifest
            .pointer("/execution/product_task_id")
            .and_then(Value::as_str)
            != Some(binding.product_task_id.as_str())
        || manifest
            .pointer("/execution/workflow_id")
            .and_then(Value::as_str)
            != Some(binding.workflow_id.as_str())
        || manifest
            .pointer("/execution/attempt_id")
            .and_then(Value::as_str)
            != Some(binding.attempt_id.as_str())
        || !manifest
            .pointer("/execution/workflow_node_ids")
            .and_then(Value::as_array)
            .is_some_and(|nodes| {
                nodes
                    .iter()
                    .any(|node| node.as_str() == Some(binding.node_id.as_str()))
            })
    {
        return Err("managed workspace final manifest binding is stale or mismatched".into());
    }
    validate_workspace_provider_journal(journal_json, binding, scheduler)
}

/// Adapter from the existing store-owned managed-acceptance authority to the
/// protocol-neutral provider-call boundary. It reloads persisted attempt/spend
/// rows on every check; the provider layer never accepts caller assertions as
/// authority and never persists a second lease or budget.
impl LocalProductStore {
    pub(crate) fn apply_managed_workspace_action(
        &self,
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
        node_metadata: &Value,
        model_output: &str,
    ) -> Result<Value, String> {
        if self
            .current_delegated_provider_authority(binding)?
            .is_none()
        {
            return Err("workspace action requires current delegated authority".into());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                let tx = rusqlite::Transaction::new_unchecked(
                    connection,
                    TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let scheduler = load_managed_scheduler_lease_sqlite(&tx, binding)?;
                let row: (
                    String,
                    String,
                    String,
                    String,
                    String,
                    String,
                    String,
                    String,
                ) = tx
                    .query_row(
                        "SELECT status, expires_at, spend_status, attempt_status,
                                spend_authorization_id, attempt_lease_token, manifest_json,
                                provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE attempt_id=?1",
                        params![binding.attempt_id],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                                row.get(6)?,
                                row.get(7)?,
                            ))
                        },
                    )
                    .map_err(|error| error.to_string())?;
                let now = self.now();
                validate_delegated_workspace_authority(
                    &now, binding, &row.0, &row.1, &row.2, &row.3, &row.4, &row.5, &row.6, &row.7,
                    &scheduler,
                )?;
                let receipt = Self::write_managed_workspace_action(
                    binding,
                    node_metadata,
                    model_output,
                    &scheduler.allowed_paths,
                    &scheduler.workspace_path,
                )?;
                let (journal, _receipt_digest) =
                    record_workspace_action_in_journal(&row.7, binding, &receipt)?;
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=?1, updated_at=?2
                         WHERE attempt_id=?3 AND provider_request_journal_json=?4",
                        params![journal, now, binding.attempt_id, row.7],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("workspace action receipt lost its journal authority".into());
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(receipt)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let delegation = tx
                    .query_one(
                        "SELECT status, expires_at, spend_status, attempt_status,
                                spend_authorization_id, attempt_lease_token, manifest_json,
                                provider_request_journal_json
                         FROM managed_acceptance_delegations
                         WHERE attempt_id=$1 FOR UPDATE",
                        &[&binding.attempt_id],
                    )
                    .map_err(|error| error.to_string())?;
                let scheduler = load_managed_scheduler_lease_pg(&mut tx, binding)?;
                let delegation_status: String = delegation.get(0);
                let expires_at: String = delegation.get(1);
                let spend_status: String = delegation.get(2);
                let attempt_status: String = delegation.get(3);
                let spend_authorization_id: String = delegation.get(4);
                let attempt_lease_token: String = delegation.get(5);
                let manifest_json: String = delegation.get(6);
                let journal_json: String = delegation.get(7);
                let now = self.now();
                validate_delegated_workspace_authority(
                    &now,
                    binding,
                    &delegation_status,
                    &expires_at,
                    &spend_status,
                    &attempt_status,
                    &spend_authorization_id,
                    &attempt_lease_token,
                    &manifest_json,
                    &journal_json,
                    &scheduler,
                )?;
                let receipt = Self::write_managed_workspace_action(
                    binding,
                    node_metadata,
                    model_output,
                    &scheduler.allowed_paths,
                    &scheduler.workspace_path,
                )?;
                let (journal, _receipt_digest) =
                    record_workspace_action_in_journal(&journal_json, binding, &receipt)?;
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=$1, updated_at=$2
                         WHERE attempt_id=$3 AND provider_request_journal_json=$4",
                        &[&journal, &now, &binding.attempt_id, &journal_json],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("workspace action receipt lost its journal authority".into());
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(receipt)
            }),
        }
    }

    /// Persist an action failure beside the successful provider journal entry.
    /// The failure value is deliberately hashed: the journal remains a bounded
    /// recovery record and never becomes a raw error/transcript store.
    pub(crate) fn record_managed_workspace_action_failure(
        &self,
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
        failure_class: &str,
    ) -> Result<String, String> {
        if failure_class.trim().is_empty() {
            return Err("workspace action failure class is missing".into());
        }
        let failure_sha256 = sha256_hex(failure_class.as_bytes());
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                let tx = rusqlite::Transaction::new_unchecked(
                    connection,
                    TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let journal_json: String = tx
                    .query_row(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE attempt_id=?1",
                        params![binding.attempt_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let (journal, digest) = record_workspace_action_failure_in_journal(
                    &journal_json,
                    binding,
                    &failure_sha256,
                )?;
                let now = self.now();
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=?1, updated_at=?2
                         WHERE attempt_id=?3 AND provider_request_journal_json=?4",
                        params![journal, now, binding.attempt_id, journal_json],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("workspace action failure lost its journal authority".into());
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(digest)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let journal_json: String = tx
                    .query_one(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE attempt_id=$1 FOR UPDATE",
                        &[&binding.attempt_id],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                let (journal, digest) = record_workspace_action_failure_in_journal(
                    &journal_json,
                    binding,
                    &failure_sha256,
                )?;
                let now = self.now();
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=$1, updated_at=$2
                         WHERE attempt_id=$3 AND provider_request_journal_json=$4",
                        &[&journal, &now, &binding.attempt_id, &journal_json],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("workspace action failure lost its journal authority".into());
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(digest)
            }),
        }
    }

    fn write_managed_workspace_action(
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
        node_metadata: &Value,
        model_output: &str,
        owner_allowed: &Value,
        owner_workspace: &str,
    ) -> Result<Value, String> {
        if node_metadata.get("allowed_paths") != Some(owner_allowed)
            || node_metadata.get("workspace_path").and_then(Value::as_str) != Some(owner_workspace)
            || node_metadata.get("workspace_root").and_then(Value::as_str) != Some(owner_workspace)
        {
            return Err("workspace action metadata does not match persisted ProductTask".into());
        }
        let action = Self::parse_managed_workspace_action(model_output)
            .map_err(|_| "implementer output must be one JSON workspace action".to_string())?;
        if action.get("schema_version").and_then(Value::as_str)
            != Some("managed_workspace_action.v1")
        {
            return Err("workspace action schema is not canonical".to_string());
        }
        let path = action
            .get("path")
            .and_then(Value::as_str)
            .ok_or("workspace action path is required")?;
        let relative = Path::new(path);
        if path.is_empty()
            || relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err("workspace action path is not a clean relative path".to_string());
        }
        let allowed = owner_allowed
            .as_array()
            .ok_or("workspace action allowed_paths are missing")?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect::<Vec<_>>();
        if !crate::rwe::frozen_rwe_bindings::path_under_allowed_paths(path, &allowed) {
            return Err("workspace action path is outside the exact allowed path".to_string());
        }
        let workspace = Path::new(owner_workspace);
        let target = workspace.join(relative);
        let mut file = open_absolute_path_without_symlinks(&target, libc::O_RDWR)
            .map_err(|error| format!("workspace action target open failed: {error}"))?;
        if !file
            .metadata()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("workspace action target is not a regular file".into());
        }
        let mut before = Vec::new();
        file.read_to_end(&mut before)
            .map_err(|error| error.to_string())?;
        let before_sha256 = Some(hex::encode(Sha256::digest(&before)));
        let before_text = Some(
            String::from_utf8(before)
                .map_err(|_| "workspace action target is not UTF-8".to_string())?,
        );
        let operation = action
            .get("action")
            .and_then(Value::as_str)
            .ok_or("workspace action kind is required")?;
        let (after_text, changed_line_budget) = match operation {
            "replace_text" => {
                let current = before_text
                    .as_deref()
                    .ok_or("replace_text target file does not exist")?;
                let old = action
                    .get("old_text")
                    .and_then(Value::as_str)
                    .ok_or("replace_text old_text is required")?;
                let new = action
                    .get("new_text")
                    .and_then(Value::as_str)
                    .ok_or("replace_text new_text is required")?;
                if old.is_empty() || current.matches(old).count() != 1 {
                    return Err("replace_text requires exactly one existing match".to_string());
                }
                (
                    current.replacen(old, new, 1),
                    old.lines().count().max(new.lines().count()),
                )
            }
            _ => return Err("workspace action kind is not permitted".to_string()),
        };
        let lowered = after_text.to_ascii_lowercase();
        for pattern in [
            "-----begin ",
            "api_key=",
            "apikey=",
            "secret=",
            "password=",
            "authorization: bearer ",
            "x-api-key",
        ] {
            if lowered.contains(pattern) {
                return Err("workspace action contains a sensitive literal".to_string());
            }
        }
        // Docs GP: 100 lines. Frozen RWE: schedule patch_max_lines (max of frozen tasks).
        let line_ceiling = crate::rwe::frozen_rwe_bindings::frozen_rwe_max_patch_limits()
            .map(|(_, lines)| lines)
            .unwrap_or(100)
            .max(100);
        if changed_line_budget as u64 > line_ceiling || after_text.len() > 1_048_576 {
            return Err("workspace action exceeds the delegated change bounds".to_string());
        }
        file.rewind().map_err(|error| error.to_string())?;
        file.set_len(0).map_err(|error| error.to_string())?;
        file.write_all(after_text.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        let after_sha256 = hex::encode(Sha256::digest(after_text.as_bytes()));
        Ok(json!({
            "schema_version": "managed_workspace_action_receipt.v1",
            "product_task_id": binding.product_task_id,
            "workflow_id": binding.workflow_id,
            "node_id": binding.node_id,
            "action": operation,
            "path": path,
            "before_sha256": before_sha256,
            "after_sha256": after_sha256,
            "bytes": after_text.len(),
            "changed_line_budget": changed_line_budget,
        }))
    }

    /// Accept the exact action object either bare or inside one complete JSON
    /// markdown fence. No prose extraction is allowed; the downstream owner
    /// still validates the parsed action against every persisted boundary.
    fn parse_managed_workspace_action(model_output: &str) -> Result<Value, serde_json::Error> {
        let trimmed = model_output.trim();
        let candidate = if let Some(body) = trimmed.strip_prefix("```json") {
            body.strip_suffix("```").map(str::trim).unwrap_or("")
        } else {
            trimmed
        };
        serde_json::from_str(candidate)
    }

    pub(crate) fn claim_delegated_provider_request(
        &self,
        request: &crate::provider::managed_deepseek::ManagedProviderCallRequest,
    ) -> Result<(), String> {
        if self
            .current_delegated_provider_authority(&request.binding)?
            .is_none()
        {
            return Err("durable provider request requires delegated authority".into());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let row: (
                    String,
                    String,
                    String,
                    String,
                    String,
                    String,
                    String,
                    String,
                ) = tx
                    .query_row(
                        "SELECT status, expires_at, spend_status, attempt_status,
                                spend_authorization_id, attempt_lease_token, manifest_json,
                                provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE attempt_id=?1",
                        params![request.binding.attempt_id],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                                row.get(6)?,
                                row.get(7)?,
                            ))
                        },
                    )
                    .map_err(|error| error.to_string())?;
                validate_provider_journal_authority(
                    request, &now, &row.0, &row.1, &row.2, &row.3, &row.4, &row.5, &row.6,
                )?;
                let scheduler = load_managed_scheduler_lease_sqlite(&tx, &request.binding)?;
                let journal = claim_provider_journal_entry(&row.7, request, &scheduler, &now)?;
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=?1, updated_at=?2
                         WHERE attempt_id=?3 AND provider_request_journal_json=?4",
                        params![journal, now, request.binding.attempt_id, row.7],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("durable provider request claim lost its authority".into());
                }
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let row = tx
                    .query_one(
                        "SELECT status, expires_at, spend_status, attempt_status,
                                spend_authorization_id, attempt_lease_token, manifest_json,
                                provider_request_journal_json
                         FROM managed_acceptance_delegations
                         WHERE attempt_id=$1 FOR UPDATE",
                        &[&request.binding.attempt_id],
                    )
                    .map_err(|error| error.to_string())?;
                let status: String = row.get(0);
                let expires_at: String = row.get(1);
                let spend_status: String = row.get(2);
                let attempt_status: String = row.get(3);
                let spend_id: String = row.get(4);
                let lease_token: String = row.get(5);
                let manifest_json: String = row.get(6);
                let journal_json: String = row.get(7);
                validate_provider_journal_authority(
                    request,
                    &now,
                    &status,
                    &expires_at,
                    &spend_status,
                    &attempt_status,
                    &spend_id,
                    &lease_token,
                    &manifest_json,
                )?;
                let scheduler = load_managed_scheduler_lease_pg(&mut tx, &request.binding)?;
                let journal =
                    claim_provider_journal_entry(&journal_json, request, &scheduler, &now)?;
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=$1, updated_at=$2
                         WHERE attempt_id=$3 AND provider_request_journal_json=$4",
                        &[&journal, &now, &request.binding.attempt_id, &journal_json],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("durable provider request claim lost its authority".into());
                }
                tx.commit().map_err(|error| error.to_string())
            }),
        }
    }

    pub(crate) fn reconcile_delegated_provider_request(
        &self,
        request: &crate::provider::managed_deepseek::ManagedProviderCallRequest,
        response: Option<&crate::provider::managed_deepseek::ManagedProviderResponse>,
        effect: crate::provider::managed_deepseek::ManagedFailureEffect,
    ) -> Result<(), String> {
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let journal_json: String = tx
                    .query_row(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations
                         WHERE attempt_id=?1 AND spend_authorization_id=?2
                           AND attempt_status='admitted'",
                        params![
                            request.binding.attempt_id,
                            request.binding.spend_authorization_id
                        ],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let journal = reconcile_provider_journal_entry(
                    &journal_json,
                    request,
                    response,
                    effect,
                    &now,
                )?;
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=?1, updated_at=?2
                         WHERE attempt_id=?3 AND provider_request_journal_json=?4",
                        params![journal, now, request.binding.attempt_id, journal_json],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("durable provider reconciliation lost its claim".into());
                }
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let row = tx
                    .query_one(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations
                         WHERE attempt_id=$1 AND spend_authorization_id=$2
                           AND attempt_status='admitted' FOR UPDATE",
                        &[
                            &request.binding.attempt_id,
                            &request.binding.spend_authorization_id,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                let journal_json: String = row.get(0);
                let journal = reconcile_provider_journal_entry(
                    &journal_json,
                    request,
                    response,
                    effect,
                    &now,
                )?;
                let changed = tx
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=$1, updated_at=$2
                         WHERE attempt_id=$3 AND provider_request_journal_json=$4",
                        &[&journal, &now, &request.binding.attempt_id, &journal_json],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("durable provider reconciliation lost its claim".into());
                }
                tx.commit().map_err(|error| error.to_string())
            }),
        }
    }

    pub(crate) fn current_delegated_provider_authority(
        &self,
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    ) -> Result<Option<crate::provider::managed_deepseek::PersistedAuthoritySnapshot>, String> {
        let now = self.now();
        let row = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT delegation_sha256, spend_authorization_id, spend_status, attempt_id, attempt_lease_id, attempt_lease_token, attempt_status, manifest_json, status, expires_at FROM managed_acceptance_delegations WHERE attempt_id=?1",
                    params![binding.attempt_id],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, Option<String>>(6)?, r.get::<_, Option<String>>(7)?, r.get::<_, String>(8)?, r.get::<_, String>(9)?)),
                )
                .optional()
                .map_err(|e| e.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client.query_opt(
                    "SELECT delegation_sha256, spend_authorization_id, spend_status, attempt_id, attempt_lease_id, attempt_lease_token, attempt_status, manifest_json, status, expires_at FROM managed_acceptance_delegations WHERE attempt_id=$1",
                    &[&binding.attempt_id],
                )
                .map(|row| row.map(|r| (r.get::<_, String>(0), r.get::<_, Option<String>>(1), r.get::<_, Option<String>>(2), r.get::<_, Option<String>>(3), r.get::<_, Option<String>>(4), r.get::<_, Option<String>>(5), r.get::<_, Option<String>>(6), r.get::<_, Option<String>>(7), r.get::<_, String>(8), r.get::<_, String>(9))))
                .map_err(|e| e.to_string())
            }),
        }?;
        let Some((
            delegation_sha,
            spend_id,
            spend_status,
            attempt_id,
            lease_id,
            lease_token,
            attempt_status,
            manifest_json,
            delegation_status,
            expires_at,
        )) = row
        else {
            return Ok(None);
        };
        let spend_id = spend_id.ok_or("delegated spend identity is missing")?;
        let spend_status = spend_status.ok_or("delegated spend status is missing")?;
        let attempt_id = attempt_id.ok_or("delegated attempt identity is missing")?;
        let lease_id = lease_id.ok_or("delegated lease identity is missing")?;
        let lease_token = lease_token.ok_or("delegated lease token is missing")?;
        let attempt_status = attempt_status.ok_or("delegated attempt status is missing")?;
        let manifest: Value =
            serde_json::from_str(&manifest_json.ok_or("delegated manifest is missing")?)
                .map_err(|e| format!("delegated manifest JSON is invalid: {e}"))?;
        let delegation_id = manifest
            .pointer("/delegation/delegation_id")
            .and_then(Value::as_str)
            .ok_or("delegated manifest delegation_id is missing")?;
        let manifest_tenant = manifest
            .pointer("/execution/tenant_id")
            .and_then(Value::as_str)
            .ok_or("delegated manifest tenant_id is missing")?;
        self.require_manifest_product_task_binding(delegation_id, manifest_tenant, &manifest)?;
        if delegation_status != "active" || is_at_or_before(&expires_at, &now)? {
            return Err("delegated provider authority is expired or revoked".into());
        }
        let execution = manifest
            .get("execution")
            .ok_or("delegated manifest execution is missing")?;
        let node_ids = execution
            .get("workflow_node_ids")
            .and_then(Value::as_array)
            .ok_or("delegated manifest workflow_node_ids is missing")?;
        if attempt_id != binding.attempt_id
            || spend_id != binding.spend_authorization_id
            || attempt_status != "admitted"
            || spend_status != "consumed"
            || lease_id.trim().is_empty()
            || execution.get("product_task_id").and_then(Value::as_str)
                != Some(binding.product_task_id.as_str())
            || execution.get("workflow_id").and_then(Value::as_str)
                != Some(binding.workflow_id.as_str())
            || execution.get("attempt_id").and_then(Value::as_str)
                != Some(binding.attempt_id.as_str())
            || !node_ids
                .iter()
                .any(|node| node.as_str() == Some(binding.node_id.as_str()))
            || manifest.get("delegation_sha256").and_then(Value::as_str)
                != Some(delegation_sha.as_str())
            || manifest.get("manifest_sha256").and_then(Value::as_str)
                != Some(compute_attempt_manifest_sha256(&manifest)?.as_str())
            || crate::provider::managed_deepseek::managed_attempt_lease_id(&lease_token)
                != binding.attempt_lease_id
        {
            return Err("delegated provider authority is stale or mismatched".into());
        }
        Ok(Some(
            crate::provider::managed_deepseek::PersistedAuthoritySnapshot {
                product_task_id: binding.product_task_id.clone(),
                workflow_id: binding.workflow_id.clone(),
                node_id: binding.node_id.clone(),
                attempt_id: binding.attempt_id.clone(),
                spend_authorization_id: binding.spend_authorization_id.clone(),
                attempt_lease_id: binding.attempt_lease_id.clone(),
                spend_status,
                consumed_by_attempt_id: Some(binding.attempt_id.clone()),
                lease_status: "current".into(),
                execution_contract: Some(delegated_execution_contract(
                    &manifest,
                    &binding.node_id,
                )?),
            },
        ))
    }
}

fn managed_provider_request_sha256(
    request: &crate::provider::managed_deepseek::ManagedProviderCallRequest,
) -> Result<String, String> {
    let value = serde_json::to_value(request)
        .map_err(|error| format!("managed provider request cannot be fingerprinted: {error}"))?;
    Ok(sha256_hex(canonical_json(&sort_value(&value))?.as_bytes()))
}

#[allow(clippy::too_many_arguments)]
fn validate_provider_journal_authority(
    request: &crate::provider::managed_deepseek::ManagedProviderCallRequest,
    now: &str,
    status: &str,
    expires_at: &str,
    spend_status: &str,
    attempt_status: &str,
    spend_id: &str,
    lease_token: &str,
    manifest_json: &str,
) -> Result<(), String> {
    if status != "active"
        || spend_status != "consumed"
        || attempt_status != "admitted"
        || spend_id != request.binding.spend_authorization_id
        || crate::provider::managed_deepseek::managed_attempt_lease_id(lease_token)
            != request.binding.attempt_lease_id
        || is_at_or_before(expires_at, now)?
    {
        return Err("durable provider request authority is stale or closed".into());
    }
    let manifest: Value = serde_json::from_str(manifest_json)
        .map_err(|_| "durable provider request manifest is invalid")?;
    let contract = delegated_execution_contract(&manifest, &request.binding.node_id)?;
    if contract.provider_identity != request.provider_identity
        || contract.provider_kind != request.provider_kind
        || contract.protocol != request.protocol
        || contract.host != request.host
        || contract.base_url != request.base_url
        || contract.endpoint_path != request.endpoint_path
        || contract.credential_reference != request.credential_reference
        || contract.requested_model != request.requested_model
        || contract.request_schema_version != request.schema_version
        || contract.limits != request.limits
        || contract.price_profile != request.price_profile
    {
        return Err(
            "durable provider request binding is not the persisted execution contract".into(),
        );
    }
    if manifest.get("manifest_sha256").and_then(Value::as_str)
        != Some(compute_attempt_manifest_sha256(&manifest)?.as_str())
        || manifest
            .pointer("/execution/product_task_id")
            .and_then(Value::as_str)
            != Some(request.binding.product_task_id.as_str())
        || manifest
            .pointer("/execution/workflow_id")
            .and_then(Value::as_str)
            != Some(request.binding.workflow_id.as_str())
        || manifest
            .pointer("/execution/attempt_id")
            .and_then(Value::as_str)
            != Some(request.binding.attempt_id.as_str())
        || !manifest
            .pointer("/execution/workflow_node_ids")
            .and_then(Value::as_array)
            .is_some_and(|nodes| {
                nodes
                    .iter()
                    .any(|node| node.as_str() == Some(request.binding.node_id.as_str()))
            })
    {
        return Err("durable provider request manifest binding is stale or mismatched".into());
    }
    Ok(())
}

fn claim_provider_journal_entry(
    journal_json: &str,
    request: &crate::provider::managed_deepseek::ManagedProviderCallRequest,
    scheduler: &ManagedSchedulerLeaseAuthority,
    now: &str,
) -> Result<String, String> {
    let mut journal: Vec<Value> = serde_json::from_str(journal_json)
        .map_err(|_| "durable provider request journal is invalid")?;
    let request_sha256 = managed_provider_request_sha256(request)?;
    if journal.iter().any(|entry| {
        entry.get("node_id").and_then(Value::as_str) == Some(request.binding.node_id.as_str())
            || entry.get("request_sha256").and_then(Value::as_str) == Some(request_sha256.as_str())
    }) {
        return Err("durable provider request was already claimed; replay is forbidden".into());
    }
    if journal.len() as u64 >= request.limits.max_requests {
        return Err("durable provider request ceiling exhausted".into());
    }
    let reserved_input_tokens = request.estimated_input_tokens();
    let reserved_output_tokens = request.max_output_tokens;
    let reserved_tokens = reserved_input_tokens.saturating_add(reserved_output_tokens);
    let prior_tokens = journal.iter().try_fold(0_u64, |total, entry| {
        let value = entry
            .get("effective_tokens")
            .and_then(Value::as_u64)
            .ok_or("durable provider journal token reservation is missing")?;
        Ok::<u64, String>(total.saturating_add(value))
    })?;
    if reserved_input_tokens == 0
        || reserved_input_tokens > request.limits.max_input_tokens
        || reserved_output_tokens == 0
        || reserved_output_tokens > request.limits.max_output_tokens
        || prior_tokens.saturating_add(reserved_tokens) > request.limits.max_cumulative_tokens
    {
        return Err("durable provider token reservation exceeds the execution manifest".into());
    }
    let reserved_cost_usd = request.conservative_reserved_cost_usd()?;
    let prior_cost_usd = journal.iter().try_fold(0.0_f64, |total, entry| {
        let value = match entry.get("effective_cost_usd") {
            Some(Value::Null) | None => 0.0,
            Some(Value::Number(num)) => num.as_f64().unwrap_or(0.0),
            _ => return Err("durable provider journal cost reservation is invalid".to_string()),
        };
        if !value.is_finite() || value < 0.0 {
            return Err("durable provider journal cost reservation is invalid".to_string());
        }
        Ok::<f64, String>(total + value)
    })?;
    if request
        .limits
        .max_cost_usd
        .is_some_and(|maximum| prior_cost_usd + reserved_cost_usd > maximum + 1e-12)
    {
        return Err("durable provider cost reservation exceeds the execution manifest".into());
    }
    journal.push(sort_value(&json!({
        "schema_version": "managed_provider_request_claim.v1",
        "ordinal": journal.len() + 1,
        "node_id": request.binding.node_id,
        "provider_identity": request.provider_identity,
        "provider_kind": request.provider_kind,
        "role": request.role,
        "protocol": request.protocol,
        "requested_model": request.requested_model,
        "transport_provenance": request.transport_provenance.as_str(),
        "request_sha256": request_sha256,
        "status": "sending",
        "scheduler_lease": {
            "product_task_id": request.binding.product_task_id,
            "run_id": scheduler.run_id,
            "workflow_id": request.binding.workflow_id,
            "node_id": request.binding.node_id,
            "attempt_count": scheduler.attempt_count,
            "lease_owner_token_sha256": scheduler.lease_owner_token_sha256,
        },
        "reserved_input_tokens": reserved_input_tokens,
        "reserved_output_tokens": reserved_output_tokens,
        "conservative_reserved_cost_usd": if request.limits.max_cost_usd.is_none() {
            Value::Null
        } else {
            json!(reserved_cost_usd)
        },
        "effective_tokens": reserved_tokens,
        "effective_cost_usd": if request.limits.max_cost_usd.is_none() {
            Value::Null
        } else {
            json!(reserved_cost_usd)
        },
        "claimed_at": now,
    })));
    Ok(sort_value(&Value::Array(journal)).to_string())
}

fn reconcile_provider_journal_entry(
    journal_json: &str,
    request: &crate::provider::managed_deepseek::ManagedProviderCallRequest,
    response: Option<&crate::provider::managed_deepseek::ManagedProviderResponse>,
    effect: crate::provider::managed_deepseek::ManagedFailureEffect,
    now: &str,
) -> Result<String, String> {
    let mut journal: Vec<Value> = serde_json::from_str(journal_json)
        .map_err(|_| "durable provider request journal is invalid")?;
    let request_sha256 = managed_provider_request_sha256(request)?;
    let entry = journal
        .iter_mut()
        .find(|entry| {
            entry.get("node_id").and_then(Value::as_str) == Some(request.binding.node_id.as_str())
                && entry.get("request_sha256").and_then(Value::as_str)
                    == Some(request_sha256.as_str())
        })
        .ok_or("durable provider request claim is missing")?;
    if entry.get("status").and_then(Value::as_str) != Some("sending") {
        return Err("durable provider request claim is already reconciled".into());
    }
    // Provenance is immutable after claim: a replay with a different transport
    // cannot upgrade an injected claim into an external one.
    if entry.get("transport_provenance").and_then(Value::as_str)
        != Some(request.transport_provenance.as_str())
        || entry.get("provider_identity").and_then(Value::as_str)
            != Some(request.provider_identity.as_str())
        || entry.get("provider_kind").and_then(Value::as_str)
            != Some(request.provider_kind.as_str())
    {
        return Err("durable provider request transport provenance is mismatched".into());
    }
    let object = entry
        .as_object_mut()
        .ok_or("durable provider request claim is not an object")?;
    object.insert("reconciled_at".into(), json!(now));
    if let Some(response) = response {
        if effect != crate::provider::managed_deepseek::ManagedFailureEffect::NoExternalEffect
            || response.requested_model != request.requested_model
            || response.resolved_model != request.requested_model
            || response.provider_identity != request.provider_identity
            || response.provider_kind != request.provider_kind
            || response.protocol != request.protocol
            || response.usage.model != request.requested_model
            || response.usage.request_id != response.request_id
        {
            return Err("durable provider response identity is mismatched".into());
        }
        let actual_cost = match response.estimated_cost_usd {
            Some(cost) => {
                if !cost.is_finite() || cost < 0.0 {
                    return Err("durable provider response cost is invalid".into());
                }
                Some(cost)
            }
            None => {
                if request.limits.max_cost_usd.is_some() {
                    return Err("durable provider response cost is missing".into());
                }
                None
            }
        };
        object.insert("status".into(), json!("succeeded"));
        object.insert("provider_identity".into(), json!(request.provider_identity));
        object.insert("provider_kind".into(), json!(request.provider_kind));
        object.insert("request_id".into(), json!(response.request_id));
        object.insert("resolved_model".into(), json!(response.resolved_model));
        object.insert(
            "usage".into(),
            serde_json::to_value(&response.usage)
                .map_err(|error| format!("durable provider usage cannot be persisted: {error}"))?,
        );
        object.insert(
            "effective_tokens".into(),
            json!(response.usage.cumulative_tokens),
        );
        object.insert(
            "effective_cost_usd".into(),
            match actual_cost {
                Some(c) => json!(c),
                None => Value::Null,
            },
        );
    } else {
        let (status, retain_reservation) = match effect {
            crate::provider::managed_deepseek::ManagedFailureEffect::PreSend => {
                ("failed_before_send", false)
            }
            crate::provider::managed_deepseek::ManagedFailureEffect::NoExternalEffect => {
                ("failed_known_outcome", false)
            }
            crate::provider::managed_deepseek::ManagedFailureEffect::OutcomeUnknown => {
                ("outcome_unknown", true)
            }
        };
        object.insert("status".into(), json!(status));
        object.insert("provider_identity".into(), json!(request.provider_identity));
        object.insert("provider_kind".into(), json!(request.provider_kind));
        if !retain_reservation {
            object.insert("effective_tokens".into(), json!(0));
            object.insert(
                "effective_cost_usd".into(),
                if request.limits.max_cost_usd.is_none() {
                    Value::Null
                } else {
                    json!(0.0)
                },
            );
        }
    }
    Ok(sort_value(&Value::Array(journal)).to_string())
}

impl crate::provider::managed_deepseek::ManagedAuthoritySource for LocalProductStore {
    fn claim_provider_request(
        &self,
        request: &crate::provider::managed_deepseek::ManagedProviderCallRequest,
    ) -> Result<(), String> {
        self.claim_delegated_provider_request(request)
    }
    fn reconcile_provider_request(
        &self,
        request: &crate::provider::managed_deepseek::ManagedProviderCallRequest,
        response: Option<&crate::provider::managed_deepseek::ManagedProviderResponse>,
        effect: crate::provider::managed_deepseek::ManagedFailureEffect,
    ) -> Result<(), String> {
        self.reconcile_delegated_provider_request(request, response, effect)
    }
    fn stage_context(
        &self,
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
        node_metadata: &Value,
    ) -> Result<Option<Value>, String> {
        self.store_managed_stage_context(binding, node_metadata)
    }
    fn current_authority(
        &self,
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    ) -> Result<crate::provider::managed_deepseek::PersistedAuthoritySnapshot, String> {
        self.store_current_managed_authority(binding)
    }

    fn apply_workspace_action(
        &self,
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
        node_metadata: &Value,
        model_output: &str,
    ) -> Result<Value, String> {
        if self
            .current_delegated_provider_authority(binding)?
            .is_none()
        {
            return Err("managed workspace action requires a current delegated authority".into());
        }
        self.apply_managed_workspace_action(binding, node_metadata, model_output)
    }
}
impl LocalProductStore {
    pub(crate) fn store_managed_stage_context(
        &self,
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
        node_metadata: &Value,
    ) -> Result<Option<Value>, String> {
        if self
            .current_delegated_provider_authority(binding)?
            .is_none()
        {
            return Err("managed stage context requires a current delegated authority".into());
        }
        let stage = node_metadata
            .pointer("/managed_deepseek/stage")
            .and_then(Value::as_str)
            .ok_or("managed stage context is missing its route stage")?;
        if !matches!(stage, "planning" | "implementation" | "review") {
            return Err("managed stage context is not a provider route stage".into());
        }
        let task = self
            .get_product_task(&binding.product_task_id)?
            .ok_or("managed stage context ProductTask is missing")?;
        let allowed_paths = task
            .pointer("/workspace_binding/allowed_paths")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let docs_paths = allowed_paths == ["docs/USER_GUIDE.md"];
        let rwe_paths =
            crate::rwe::frozen_rwe_bindings::is_exact_frozen_rwe_allowed_paths(&allowed_paths);
        if !docs_paths && !rwe_paths {
            return Err("managed stage context allowed path binding changed".into());
        }
        let workspace_path = task
            .pointer("/workspace_binding/workspace_path")
            .and_then(Value::as_str)
            .ok_or("managed stage context workspace path is missing")?;
        let allowed_file_paths = if rwe_paths {
            bounded_workspace_file_paths(workspace_path, &allowed_paths)?
        } else {
            Vec::new()
        };
        // Docs GP stages the single USER_GUIDE file; frozen RWE selects a
        // deterministic regular file from the Store-owned bounded index.
        let stage_path = if docs_paths {
            "docs/USER_GUIDE.md".to_string()
        } else {
            node_metadata
                .pointer("/managed_deepseek/prompt_path")
                .and_then(Value::as_str)
                .filter(|path| allowed_file_paths.iter().any(|candidate| candidate == path))
                .map(str::to_string)
                .or_else(|| allowed_file_paths.first().cloned())
                .ok_or("managed stage context frozen RWE paths contain no regular file")?
        };
        let file_path = Path::new(workspace_path).join(&stage_path);
        let (content, staged_path) = if file_path.is_file() {
            let mut file = open_absolute_path_without_symlinks(&file_path, libc::O_RDONLY)
                .map_err(|_| "managed stage context allowed file is unavailable")?;
            let metadata = file
                .metadata()
                .map_err(|_| "managed stage context allowed file metadata is unavailable")?;
            if !metadata.is_file() || metadata.len() > 64 * 1024 {
                return Err("managed stage context allowed file exceeds its bounded input".into());
            }
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|_| "managed stage context allowed file is not UTF-8")?;
            if crate::provider::redaction::contains_sensitive_patterns(&content) {
                return Err(
                    "managed stage context secret scan failed before provider request".into(),
                );
            }
            // Keep the request-time workspace context inside the frozen
            // provider envelope. The action sink still reads the complete
            // target file before applying the model's exact replacement.
            (content.chars().take(4096).collect::<String>(), stage_path)
        } else {
            return Err("managed stage context allowed file is unavailable".into());
        };
        let run_id = task
            .get("run_id")
            .and_then(Value::as_str)
            .ok_or("managed stage context ProductTask run is missing")?;
        let run = self.get_workflow_run(run_id)?;
        let planner_receipt = match (stage, run.as_ref()) {
            ("implementation", Some(run)) | ("review", Some(run)) => {
                Some(managed_deepseek_stage_receipt(
                    run,
                    &format!("{}-planning", binding.workflow_id),
                    "managed_deepseek_plan.v1",
                )?)
            }
            ("review", None) => {
                return Err("managed stage context planner workflow run is missing".into())
            }
            _ => None,
        };
        let verification = if stage == "review" {
            let run = run
                .as_ref()
                .ok_or("managed stage context verifier workflow run is missing")?;
            let node = managed_deepseek_run_node(
                run,
                &format!("{}-deterministic_verification", binding.workflow_id),
            )?;
            let result = node
                .get("result")
                .ok_or("managed reviewer deterministic verifier result is missing")?;
            let process_outcome = result
                .get("process_outcome")
                .filter(|value| value.is_object())
                .ok_or("managed reviewer deterministic verifier process outcome is missing")
                .and_then(|value| {
                    serde_json::from_value::<ProcessOutcome>(value.clone()).map_err(|_| {
                        "managed reviewer deterministic verifier process outcome is invalid"
                    })
                })?;
            if node.get("status").and_then(Value::as_str) != Some("completed")
                || result.get("status").and_then(Value::as_str) != Some("completed")
                || !process_outcome.boundary_mapping().is_known_success()
            {
                return Err(
                    "managed reviewer requires successful deterministic verification".into(),
                );
            }
            Some(json!({
                "schema_version": "managed_deterministic_verification_receipt.v1",
                "status": "succeeded",
                "node_id": node.get("node_id"),
                "result_sha256": sha256_hex(canonical_json(result)?.as_bytes())
            }))
        } else {
            None
        };
        Ok(Some(sort_value(&json!({
            "schema_version": "managed_deepseek_stage_context.v1",
            "stage": stage,
            "planner_receipt": planner_receipt,
            "deterministic_verification": verification,
            "allowed_file": {
                "path": staged_path,
                "content": content
            },
            "allowed_paths": allowed_paths,
            "allowed_file_paths": allowed_file_paths
        }))))
    }
    pub(crate) fn store_current_managed_authority(
        &self,
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    ) -> Result<crate::provider::managed_deepseek::PersistedAuthoritySnapshot, String> {
        if let Some(authority) = self.current_delegated_provider_authority(binding)? {
            return Ok(authority);
        }
        let attempt = self
            .get_managed_acceptance_attempt(&binding.attempt_id)?
            .ok_or_else(|| "managed provider attempt is missing".to_string())?;
        if attempt.get("status").and_then(Value::as_str) != Some("admitted")
            || attempt.get("product_task_id").and_then(Value::as_str)
                != Some(binding.product_task_id.as_str())
            || attempt.get("workflow_node_id").and_then(Value::as_str)
                != Some(binding.node_id.as_str())
        {
            return Err("managed provider attempt lease is stale or mismatched".to_string());
        }
        let current_lease_token = self.current_attempt_lease_token(&binding.attempt_id)?;
        if current_lease_token.is_empty()
            || crate::provider::managed_deepseek::managed_attempt_lease_id(&current_lease_token)
                != binding.attempt_lease_id
        {
            return Err("managed provider attempt lease is stale or mismatched".to_string());
        }
        let persisted_workflow_id = attempt
            .pointer("/body_json/workflow_id")
            .and_then(Value::as_str)
            .or_else(|| {
                attempt
                    .pointer("/body_json/workflow/workflow_id")
                    .and_then(Value::as_str)
            })
            .ok_or_else(|| {
                "managed provider attempt lacks persisted workflow identity".to_string()
            })?;
        if persisted_workflow_id != binding.workflow_id {
            return Err("managed provider workflow identity is stale or mismatched".to_string());
        }
        let spend_id = attempt
            .get("spend_authorization_id")
            .and_then(Value::as_str)
            .filter(|value| *value == binding.spend_authorization_id)
            .ok_or_else(|| "managed provider spend identity is stale or mismatched".to_string())?;
        let spend = self
            .get_managed_acceptance_spend_authorization(spend_id)?
            .ok_or_else(|| "managed provider spend authorization is missing".to_string())?;
        let spend_status = spend
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(|| "managed provider spend status is missing".to_string())?
            .to_string();
        let consumed_by_attempt_id = spend
            .get("consumed_by_attempt_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        if spend_status == "consumed"
            && consumed_by_attempt_id.as_deref() != Some(binding.attempt_id.as_str())
        {
            return Err("managed provider spend was consumed by another attempt".to_string());
        }
        if spend_status != "active" && spend_status != "consumed" {
            return Err("managed provider spend is not current".to_string());
        }
        Ok(
            crate::provider::managed_deepseek::PersistedAuthoritySnapshot {
                product_task_id: binding.product_task_id.clone(),
                workflow_id: binding.workflow_id.clone(),
                node_id: binding.node_id.clone(),
                attempt_id: binding.attempt_id.clone(),
                spend_authorization_id: binding.spend_authorization_id.clone(),
                attempt_lease_id: binding.attempt_lease_id.clone(),
                spend_status,
                consumed_by_attempt_id,
                lease_status: "current".to_string(),
                execution_contract: None,
            },
        )
    }
}

/// Return a deterministic, bounded index of regular files under frozen RWE
/// directory prefixes. It exposes names only; the workspace action sink still
/// revalidates the chosen path and opens it without symlink traversal.
fn bounded_workspace_file_paths(
    workspace_path: &str,
    allowed_paths: &[String],
) -> Result<Vec<String>, String> {
    let workspace = Path::new(workspace_path);
    let mut pending = Vec::new();
    let mut files = Vec::new();
    for allowed in allowed_paths {
        let relative = Path::new(allowed);
        let absolute = workspace.join(relative);
        let metadata = std::fs::symlink_metadata(&absolute)
            .map_err(|_| "managed stage context allowed path is unavailable")?;
        if metadata.file_type().is_symlink() {
            return Err("managed stage context allowed path is a symlink".into());
        }
        if metadata.is_file() {
            files.push(allowed.clone());
        } else if metadata.is_dir() {
            pending.push((absolute, relative.to_path_buf()));
        }
    }
    while let Some((directory, relative_directory)) = pending.pop() {
        let mut entries = std::fs::read_dir(&directory)
            .map_err(|_| "managed stage context allowed directory is unavailable")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "managed stage context allowed directory is unavailable")?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let entry_path = entry.path();
            let metadata = std::fs::symlink_metadata(&entry_path)
                .map_err(|_| "managed stage context directory entry is unavailable")?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            let relative = relative_directory.join(entry.file_name());
            if metadata.is_dir() {
                pending.push((entry_path, relative));
            } else if metadata.is_file() {
                if let Some(path) = relative.to_str() {
                    files.push(path.to_string());
                }
            }
            if files.len() >= 128 {
                break;
            }
        }
        if files.len() >= 128 {
            break;
        }
    }
    files.sort();
    files.dedup();
    files.truncate(64);
    Ok(files)
}

fn managed_deepseek_run_node<'a>(run: &'a Value, node_id: &str) -> Result<&'a Value, String> {
    run.get("nodes")
        .and_then(Value::as_array)
        .and_then(|nodes| {
            nodes
                .iter()
                .find(|node| node.get("node_id").and_then(Value::as_str) == Some(node_id))
        })
        .ok_or_else(|| format!("managed route predecessor node is missing: {node_id}"))
}

pub(super) fn managed_deepseek_stage_receipt(
    run: &Value,
    node_id: &str,
    schema_version: &str,
) -> Result<Value, String> {
    let node = managed_deepseek_run_node(run, node_id)?;
    if node.get("status").and_then(Value::as_str) != Some("completed") {
        return Err(format!(
            "managed route predecessor is incomplete: {node_id}"
        ));
    }
    let output = node
        .pointer("/result/output")
        .and_then(Value::as_str)
        .ok_or("managed route predecessor output receipt is missing")?;
    let output: Value = serde_json::from_str(output)
        .map_err(|_| "managed route predecessor output receipt is invalid JSON")?;
    let receipt = output
        .get("stage_receipt")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or("managed route predecessor typed receipt is missing")?;
    if receipt.get("schema_version").and_then(Value::as_str) != Some(schema_version) {
        return Err("managed route predecessor typed receipt schema is invalid".into());
    }
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::codex_partial_mediation_authority_decision::OPERATOR_RISK_ACCEPTANCE_PHRASE;
    use crate::product_golden_path::{
        validate_intake, ProductExecutorPolicy, ProductTaskIntakeRequest,
        ProductVerificationCommand, PRODUCT_TASK_GATE,
    };
    use crate::provider::config::{CredentialRef, ProviderConfig};
    use crate::provider::credential::CredentialBoundary;
    use crate::provider::managed_deepseek::{
        DeepSeekPriceProfile, DeepSeekProtocol, ManagedCallLimits, ManagedDeepSeekProvider,
        DEEPSEEK_CREDENTIAL_REFERENCE,
    };
    use crate::provider::managed_deepseek_executor::{
        ManagedDeepSeekExecutorConfig, ManagedDeepSeekNodeExecutor,
    };
    use crate::provider::transport::{HttpError, HttpRequest, HttpResponse, HttpTransport};
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;
    use tempfile::tempdir;

    fn store() -> (tempfile::TempDir, LocalProductStore) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ma.db");
        let store = LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
        (dir, store)
    }

    struct DelegatedRouteMockTransport {
        responses: Mutex<Vec<HttpResponse>>,
        sends: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl HttpTransport for DelegatedRouteMockTransport {
        async fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, HttpError> {
            self.sends.fetch_add(1, Ordering::SeqCst);
            let mut responses = self
                .responses
                .lock()
                .map_err(|_| HttpError::Connection("mock transport poisoned".into()))?;
            if responses.is_empty() {
                Err(HttpError::Connection(
                    "unexpected fourth provider request".into(),
                ))
            } else {
                Ok(responses.remove(0))
            }
        }
    }

    struct DelegatedOutcomeUnknownTransport {
        sends: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl HttpTransport for DelegatedOutcomeUnknownTransport {
        async fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, HttpError> {
            self.sends.fetch_add(1, Ordering::SeqCst);
            Err(HttpError::Connection(
                "mock post-send connection outcome is unknown".into(),
            ))
        }
    }

    struct DelegatedBlockedImplementationTransport {
        responses: Mutex<Vec<HttpResponse>>,
        sends: AtomicUsize,
        implementation_entered: Arc<Barrier>,
        implementation_release: Arc<Barrier>,
    }

    #[async_trait::async_trait]
    impl HttpTransport for DelegatedBlockedImplementationTransport {
        async fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, HttpError> {
            let ordinal = self.sends.fetch_add(1, Ordering::SeqCst);
            if ordinal == 1 {
                self.implementation_entered.wait();
                self.implementation_release.wait();
            }
            let mut responses = self
                .responses
                .lock()
                .map_err(|_| HttpError::Connection("blocked transport poisoned".into()))?;
            if responses.is_empty() {
                Err(HttpError::Connection(
                    "unexpected blocked transport provider request".into(),
                ))
            } else {
                Ok(responses.remove(0))
            }
        }
    }

    fn delegated_openai_response(id: &str, model: &str, content: Value) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: json!({
                "id": id,
                "model": model,
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": content.to_string()
                    },
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 20, "completion_tokens": 10}
            })
            .to_string()
            .into_bytes(),
        }
    }

    fn delegated_mock_provider(
        model: &str,
        transport: Arc<dyn HttpTransport>,
    ) -> Arc<ManagedDeepSeekProvider> {
        let config = ProviderConfig::new(
            "deepseek-managed-test",
            "openai_compatible",
            "https://api.deepseek.com",
            model,
            DEEPSEEK_CREDENTIAL_REFERENCE,
            "2026-07-30T00:00:00Z",
        );
        let credential = CredentialRef::new(
            DEEPSEEK_CREDENTIAL_REFERENCE,
            "env",
            "***",
            "provider:deepseek",
            "2026-07-30T00:00:00Z",
        );
        Arc::new(ManagedDeepSeekProvider::new_openai(
            config,
            CredentialBoundary::for_test(),
            credential,
            transport,
        ))
    }

    fn decision_body(decision_id: &str) -> Value {
        decision_body_for_attempt(decision_id, "attempt-1")
    }

    fn decision_body_for_attempt(decision_id: &str, attempt_id: &str) -> Value {
        json!({
            "decision_id": decision_id,
            "schema_version": "codex_partial_mediation_authority_decision.v2",
            "status": "draft_pending_operator",
            "invalidation_state": "none",
            "acknowledgement": {
                "required_phrase": OPERATOR_RISK_ACCEPTANCE_PHRASE,
            },
            "trial_envelope": {
                "max_retries": 0,
                "max_provider_requests": 1,
                "draft_pr_only": true,
                "max_input_tokens": 8000,
                "max_output_tokens": 4000,
                "max_total_tokens": 12000,
                "max_wall_time_ms": 300000,
                "exact_codex_version": "0.145.0",
                "exact_codex_sha_required": true,
                "provider_kind": "openai_compatible",
                "provider_host": "api.openai.com",
                "provider_base_url": "https://api.openai.com/v1",
                "admitted_endpoint_paths": vec!["/v1/responses"],
                "model": "gpt-5.6-luna",
                "product_task_id": "ptask-1",
                "workflow_id": "wf-1",
                "workflow_node_id": "node-1",
                "execution_id": format!("codex-attempt-{attempt_id}"),
                "attempt_id": attempt_id,
                "target_repo": "org/disposable-trial",
                "target_main_sha": "a".repeat(40),
                "exact_codex_path": "/usr/bin/codex",
                "exact_codex_sha256": "ab".repeat(32),
                "cancellation_identity": "cancel-1",
                "rollback_identity": "rollback-1",
                "output_branch_prefix": "acp/",
                "cost_authority": {
                    "kind": "cost_unavailable",
                    "monetary_ceiling_enforced": false,
                    "note": "rely on request/token/time caps; no monetary ceiling claimed",
                },
                "auto_merge_disabled": true,
            },
        })
    }

    fn seed_key(store: &LocalProductStore, tenant_id: &str, key_id: &str) {
        store
            .record_api_key_metadata_for_tenant(
                tenant_id,
                key_id,
                "user-operator-1",
                "operator",
                &ALL_MANAGED_ACCEPTANCE_SCOPES
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect::<Vec<_>>(),
                "test-setup",
            )
            .unwrap();
    }

    fn spend_req(risk_id: &str, attempt_id: &str) -> SpendAuthorizationRequest {
        SpendAuthorizationRequest {
            risk_authorization_id: risk_id.into(),
            product_task_id: "ptask-1".into(),
            workflow_id: Some("wf-1".into()),
            workflow_node_id: Some("node-1".into()),
            execution_id: format!("codex-attempt-{attempt_id}"),
            attempt_id: attempt_id.into(),
            binary_path: "/usr/bin/codex".into(),
            binary_version: "0.145.0".into(),
            binary_sha256: "ab".repeat(32),
            runtime_profile_sha256: None,
            capability_probe_sha256: None,
            provider_kind: "openai_compatible".into(),
            provider_host: "api.openai.com".into(),
            provider_base_url: "https://api.openai.com/v1".into(),
            admitted_endpoint_paths: vec!["/v1/responses".into()],
            model: "gpt-5.6-luna".into(),
            target_repo: "org/disposable-trial".into(),
            target_main_sha: "a".repeat(40),
            output_branch_prefix: "acp/".into(),
            draft_pr_only: true,
            max_provider_requests: 1,
            max_retries: 0,
            max_input_tokens: 8000,
            max_output_tokens: 4000,
            max_total_tokens: 12000,
            max_wall_time_ms: 300000,
            cost_authority: CostAuthority::CostUnavailable,
            cancellation_identity: "cancel-1".into(),
            rollback_identity: "rollback-1".into(),
        }
    }

    fn attempt_body_for(req: &SpendAuthorizationRequest) -> Value {
        // Mirror spend body fields used by build_attempt_authority_manifest.
        let spend_like = json!({
            "product_task_id": req.product_task_id,
            "workflow_id": req.workflow_id,
            "workflow_node_id": req.workflow_node_id,
            "execution_id": req.execution_id,
            "attempt_id": req.attempt_id,
            "binary_path": req.binary_path,
            "binary_version": req.binary_version,
            "binary_sha256": req.binary_sha256,
            "runtime_profile_sha256": req.runtime_profile_sha256,
            "capability_probe_sha256": req.capability_probe_sha256,
            "provider_kind": req.provider_kind,
            "provider_host": req.provider_host,
            "provider_base_url": req.provider_base_url,
            "admitted_endpoint_paths": req.admitted_endpoint_paths,
            "model": req.model,
            "target_repo": req.target_repo,
            "target_main_sha": req.target_main_sha,
            "output_branch_prefix": req.output_branch_prefix,
            "draft_pr_only": req.draft_pr_only,
            "max_provider_requests": req.max_provider_requests,
            "max_retries": req.max_retries,
            "max_input_tokens": req.max_input_tokens,
            "max_output_tokens": req.max_output_tokens,
            "max_total_tokens": req.max_total_tokens,
            "max_wall_time_ms": req.max_wall_time_ms,
            "cost_authority": req.cost_authority.to_json(),
            "cancellation_identity": req.cancellation_identity,
            "rollback_identity": req.rollback_identity,
            "decision_body_sha256": Value::Null,
            "spend_authorization_id": Value::Null,
        });
        let manifest = build_attempt_authority_manifest(&spend_like).unwrap();
        let mut body = spend_like;
        if let Value::Object(ref mut map) = body {
            map.insert(
                "manifest_sha256".into(),
                manifest
                    .get("manifest_sha256")
                    .cloned()
                    .unwrap_or(Value::Null),
            );
            map.insert("manifest".into(), manifest);
        }
        body
    }

    #[test]
    fn free_form_from_api_key_removed_store_auth_required() {
        let (_dir, store) = store();
        let err = store
            .authenticate_managed_acceptance_principal("t1", "agent", None)
            .unwrap_err();
        assert!(
            err.contains("not admitted") || err.contains("not found"),
            "{err}"
        );
        seed_key(&store, "tenant-a", "key_operator_ok");
        let p = store
            .authenticate_managed_acceptance_principal("tenant-a", "key_operator_ok", Some(1.0))
            .unwrap();
        assert_eq!(p.principal_id(), "key_operator_ok");
        assert!(p.has_scope(SCOPE_RISK_ACKNOWLEDGE));
    }

    #[test]
    fn managed_principal_auth_rejects_unbound_metadata() {
        let (_dir, store) = store();
        store
            .record_api_key_metadata(
                "unbound-operator",
                "operator-user",
                "operator",
                &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
                "test-setup",
            )
            .unwrap();
        let error = store
            .authenticate_managed_acceptance_principal("tenant-a", "unbound-operator", Some(1.0))
            .unwrap_err();
        assert!(error.contains("tenant binding"), "{error}");
    }

    #[test]
    fn persisted_key_metadata_reissues_principal_after_restart_without_secret() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("operator-key-reissue.db");
        let scopes = vec![
            SCOPE_RISK_ACKNOWLEDGE.to_string(),
            SCOPE_DELEGATED_EXECUTE.to_string(),
        ];
        let store = LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
        store
            .record_api_key_metadata_for_tenant(
                "tenant-a",
                "restart-operator-key",
                "operator-user",
                "operator",
                &scopes,
                "canonical-key-issuer",
            )
            .unwrap();
        let metadata = store
            .get_api_key_metadata("restart-operator-key")
            .unwrap()
            .unwrap();
        assert_eq!(metadata["scopes"], json!(scopes));
        assert!(metadata.get("raw_key").is_none());
        drop(store);

        let restarted =
            LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:01Z".to_string())
                .unwrap();
        let principal = restarted
            .authenticate_managed_acceptance_principal(
                "tenant-a",
                "restart-operator-key",
                Some(1.0),
            )
            .unwrap();
        assert_eq!(principal.principal_id(), "restart-operator-key");
        assert!(principal.has_scope(SCOPE_RISK_ACKNOWLEDGE));
        assert!(principal.has_scope(SCOPE_DELEGATED_EXECUTE));
        assert!(!principal.has_scope(SCOPE_DELEGATED_ARTIFACT_CONFIRM));
    }

    #[test]
    fn expired_api_key_principal_is_rejected_against_wall_clock() {
        let (_dir, store) = store();
        // Key expired at unix 100. A mistaken cost-cap "now" of 0.50 never exceeds
        // real production expiry timestamps and would keep keys alive forever.
        store
            .record_api_key_metadata_with_expiry_for_tenant(
                "tenant-a",
                "key_expired",
                "operator",
                "operator",
                &ALL_MANAGED_ACCEPTANCE_SCOPES
                    .iter()
                    .map(|scope| (*scope).to_string())
                    .collect::<Vec<_>>(),
                Some(100.0),
                "test",
            )
            .unwrap();
        // Mistaken cost-as-clock still sees the key as live (0.50 < 100).
        store
            .authenticate_managed_acceptance_principal("tenant-a", "key_expired", Some(0.50))
            .expect("cost-as-clock incorrectly keeps the key live");
        // Production callers must use wall clock (None) and reject after expiry.
        let wall_clock = store
            .authenticate_managed_acceptance_principal("tenant-a", "key_expired", None)
            .unwrap_err();
        assert!(wall_clock.contains("expired"), "{wall_clock}");
        // Explicit frozen "now" after expiry fails; before expiry still works.
        let after = store
            .authenticate_managed_acceptance_principal("tenant-a", "key_expired", Some(100.5))
            .unwrap_err();
        assert!(after.contains("expired"), "{after}");
        store
            .authenticate_managed_acceptance_principal("tenant-a", "key_expired", Some(50.0))
            .unwrap();
    }

    #[test]
    fn decision_creation_rejects_non_draft_status_without_transition_receipt() {
        let (_dir, store) = store();
        let residual = "a9".repeat(32);
        let parameter_error = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body("mad-non-draft-parameter"),
                &residual,
                "operator_accepted",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .expect_err("a decision cannot be created in an accepted state");
        assert!(parameter_error.contains("transitions are receipt-owned"));

        let mut self_declared = decision_body("mad-non-draft-body");
        self_declared["status"] = json!("operator_accepted");
        let body_error = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &self_declared,
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .expect_err("a decision body cannot self-declare accepted status");
        assert!(body_error.contains("is not transition evidence"));
    }

    #[test]
    fn risk_ack_store_derived_spend_admit_idempotent_and_conflict() {
        let (dir, store) = store();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-alice")
                .unwrap();
        let residual = "ab".repeat(32);
        let body = decision_body("mad-test-1");
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &body,
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let dsha = decision["decision_body_sha256"]
            .as_str()
            .unwrap()
            .to_string();
        let auth = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-test-1".into(),
                    expected_decision_body_sha256: dsha.clone(),
                    expected_residual_finding_sha256: residual.clone(),
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        assert_eq!(auth["execution_granted"], false);
        assert!(auth["fixture_only"].as_bool().unwrap_or(false));
        let auth_id = auth["authorization_id"].as_str().unwrap().to_string();

        // exact risk replay
        let auth2 = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-test-1".into(),
                    expected_decision_body_sha256: dsha.clone(),
                    expected_residual_finding_sha256: residual.clone(),
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        assert_eq!(auth2["authorization_id"], auth_id);

        let spend_request = spend_req(&auth_id, "attempt-1");
        let spend = store
            .issue_managed_acceptance_spend_authorization(&principal, &spend_request)
            .unwrap();
        let spend_id = spend["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(spend["status"], "active");
        let attempt_body = attempt_body_for(&spend_request);
        let unauthorized =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-mallory")
                .unwrap();
        let unauthorized_error = store
            .admit_managed_acceptance_attempt_for_test(
                &unauthorized,
                "attempt-1",
                &attempt_body,
                &spend_id,
                true,
            )
            .expect_err("another principal must not consume this spend");
        assert!(
            unauthorized_error.contains("spend principal mismatch"),
            "{unauthorized_error}"
        );
        assert_eq!(
            store
                .get_managed_acceptance_spend_authorization(&spend_id)
                .unwrap()
                .unwrap()["status"],
            "active"
        );
        let production_fixture_error = store
            .admit_managed_acceptance_attempt(&principal, "attempt-1", &attempt_body, &spend_id)
            .expect_err("production admission must reject fixture authority");
        assert!(production_fixture_error.contains("fixture principal"));
        assert_eq!(
            store
                .get_managed_acceptance_spend_authorization(&spend_id)
                .unwrap()
                .unwrap()["status"],
            "active"
        );
        let a1 = store
            .admit_managed_acceptance_attempt_for_test(
                &principal,
                "attempt-1",
                &attempt_body,
                &spend_id,
                true,
            )
            .unwrap();
        assert_eq!(a1["status"], "admitted");
        assert!(a1["lease_token"].as_str().unwrap().starts_with("lease-"));
        let lease = a1["lease_token"].as_str().unwrap().to_string();

        let a2 = store
            .admit_managed_acceptance_attempt_for_test(
                &principal,
                "attempt-1",
                &attempt_body,
                &spend_id,
                true,
            )
            .unwrap();
        assert_eq!(a2["idempotent_replay"], true);
        let restarted = LocalProductStore::new_with_clock(dir.path().join("ma.db"), || {
            "2026-07-25T12:00:00Z".to_string()
        })
        .unwrap();
        let restarted_replay = restarted
            .admit_managed_acceptance_attempt_for_test(
                &principal,
                "attempt-1",
                &attempt_body,
                &spend_id,
                true,
            )
            .unwrap();
        assert_eq!(restarted_replay["idempotent_replay"], true);

        // spend consumed — cannot re-admit different attempt
        let spend_row = store
            .get_managed_acceptance_spend_authorization(&spend_id)
            .unwrap()
            .unwrap();
        assert_eq!(spend_row["status"], "consumed");

        let mut conflict_body = attempt_body.clone();
        conflict_body["model"] = json!("conflict-model");
        if let Some(manifest) = conflict_body.get_mut("manifest") {
            manifest["model"] = json!("conflict-model");
        }
        let conflict = store.admit_managed_acceptance_attempt_for_test(
            &principal,
            "attempt-1",
            &conflict_body,
            &spend_id,
            true,
        );
        assert!(conflict.unwrap_err().contains("conflict"));

        store
            .complete_managed_acceptance_attempt(
                "attempt-1",
                &lease,
                "succeeded",
                "succeeded_fixture",
                &json!({"ok": true}),
            )
            .unwrap();
        let late = store.complete_managed_acceptance_attempt(
            "attempt-1",
            &lease,
            "failed",
            "failed_provider",
            &json!({"ok": false}),
        );
        assert!(late.unwrap_err().contains("late terminal"));
        // Exact terminal replay requires the current lease; a reconstructed
        // receipt with an arbitrary lease is not an authorized replay.
        let exact = store
            .complete_managed_acceptance_attempt(
                "attempt-1",
                &lease,
                "succeeded",
                "succeeded_fixture",
                &json!({"ok": true}),
            )
            .unwrap();
        assert_eq!(exact["status"], "succeeded");
        let conflict_receipt = store.complete_managed_acceptance_attempt(
            "attempt-1",
            "wrong-lease",
            "succeeded",
            "succeeded_fixture",
            &json!({"ok": true, "extra": 1}),
        );
        assert!(conflict_receipt
            .unwrap_err()
            .contains("lease_token mismatch"));
        let restarted_terminal = restarted
            .get_managed_acceptance_attempt("attempt-1")
            .unwrap()
            .expect("terminal attempt must survive a separate store restart");
        assert_eq!(restarted_terminal["status"], "succeeded");
        assert_eq!(
            restarted
                .get_managed_acceptance_spend_authorization(&spend_id)
                .unwrap()
                .unwrap()["status"],
            "consumed",
            "terminalization must never reactivate a consumed spend"
        );
    }

    #[test]
    fn spend_issue_reuses_one_active_receipt_for_the_same_logical_attempt() {
        let (_dir, store) = store();
        let principal = AuthenticatedPrincipal::fixture_for_tests(
            "tenant-a",
            "fixture-principal-spend-idempotent",
        )
        .unwrap();
        let residual = "77".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body_for_attempt("mad-spend-idempotent", "same-attempt"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let risk = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-spend-idempotent".into(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .into(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let request = spend_req(risk["authorization_id"].as_str().unwrap(), "same-attempt");

        let first = store
            .issue_managed_acceptance_spend_authorization(&principal, &request)
            .unwrap();
        let second = store
            .issue_managed_acceptance_spend_authorization(&principal, &request)
            .unwrap();

        assert_eq!(first["status"], "active");
        assert_eq!(
            first["spend_authorization_id"], second["spend_authorization_id"],
            "replay must reuse the one active logical spend receipt"
        );
        assert_eq!(
            first["logical_authorization_sha256"],
            stable_spend_authorization_identity(&first["body_json"]).unwrap(),
            "persisted logical spend identity must match its canonical body"
        );
    }

    #[test]
    fn logical_spend_identity_is_stable_before_receipt_id_and_timestamp_generation() {
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-logical")
                .unwrap();
        let request = spend_req("maa-logical", "logical-attempt");
        let risk = json!({
            "authorization_sha256": "a1".repeat(32),
            "decision_body_sha256": "b2".repeat(32),
            "residual_finding_sha256": "c3".repeat(32),
            "expires_at": "2026-07-26T00:00:00Z",
        });
        let first = build_spend_body(
            "mas-first-receipt",
            &principal,
            &request,
            &risk,
            "mad-logical",
            true,
            "2026-07-25T12:00:00Z",
            None,
        )
        .unwrap();
        let second = build_spend_body(
            "mas-second-receipt",
            &principal,
            &request,
            &risk,
            "mad-logical",
            true,
            "2026-07-25T12:01:00Z",
            None,
        )
        .unwrap();
        assert_eq!(
            stable_spend_authorization_identity(&first).unwrap(),
            stable_spend_authorization_identity(&second).unwrap(),
            "logical identity must be derived before UUID/timestamp receipt generation"
        );
        let mut different_expiry = second;
        different_expiry["expires_at"] = json!("2026-07-26T01:00:00Z");
        assert_ne!(
            stable_spend_authorization_identity(&first).unwrap(),
            stable_spend_authorization_identity(&different_expiry).unwrap(),
            "expiry is authorization scope and must remain in the logical identity"
        );
    }

    #[test]
    fn attempt_admission_rejects_non_boolean_persisted_spend_fixture_flag() {
        let (_dir, store) = store();
        let principal = AuthenticatedPrincipal::fixture_for_tests(
            "tenant-a",
            "fixture-principal-spend-boolean",
        )
        .unwrap();
        let decision_id = "mad-spend-boolean";
        let residual = "7d".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body_for_attempt(decision_id, "spend-boolean-attempt"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let risk = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: decision_id.into(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .into(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let request = spend_req(
            risk["authorization_id"].as_str().unwrap(),
            "spend-boolean-attempt",
        );
        let spend = store
            .issue_managed_acceptance_spend_authorization(&principal, &request)
            .unwrap();
        let spend_id = spend["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        store
            .with_conn(|conn| {
                // The production schema already constrains this column. Use
                // SQLite's isolated test-only check bypass to prove the
                // loader remains fail-closed even if storage is corrupted
                // underneath that first line of defense.
                conn.execute_batch("PRAGMA ignore_check_constraints=ON")
                    .map_err(|error| error.to_string())?;
                conn.execute(
                    "UPDATE managed_acceptance_spend_authorizations
                     SET fixture_only=-1 WHERE spend_authorization_id=?1",
                    [&spend_id],
                )
                .map_err(|error| error.to_string())?;
                conn.execute_batch("PRAGMA ignore_check_constraints=OFF")
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();

        let error = store
            .admit_managed_acceptance_attempt_for_test(
                &principal,
                "spend-boolean-attempt",
                &attempt_body_for(&request),
                &spend_id,
                true,
            )
            .expect_err("non-boolean fixture flag must not consume a spend");
        assert!(
            error.contains("spend fixture_only is not a persisted boolean"),
            "{error}"
        );
        let active: i64 = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM managed_acceptance_spend_authorizations
                     WHERE spend_authorization_id=?1 AND status='active'",
                    [&spend_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(active, 1, "rejected spend must never be consumed");
    }

    #[test]
    fn attempt_admission_rejects_persisted_spend_principal_kind_tampering() {
        let (_dir, store) = store();
        let principal = AuthenticatedPrincipal::fixture_for_tests(
            "tenant-a",
            "fixture-principal-spend-principal-kind",
        )
        .unwrap();
        let decision_id = "mad-spend-principal-kind";
        let residual = "7f".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body_for_attempt(decision_id, "spend-principal-kind-attempt"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let risk = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: decision_id.into(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .into(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let request = spend_req(
            risk["authorization_id"].as_str().unwrap(),
            "spend-principal-kind-attempt",
        );
        let spend = store
            .issue_managed_acceptance_spend_authorization(&principal, &request)
            .unwrap();
        let spend_id = spend["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_spend_authorizations
                     SET principal_kind='operator_api_key' WHERE spend_authorization_id=?1",
                    [&spend_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();

        let error = store
            .admit_managed_acceptance_attempt_for_test(
                &principal,
                "spend-principal-kind-attempt",
                &attempt_body_for(&request),
                &spend_id,
                true,
            )
            .expect_err("principal-kind tampering must not consume a spend");
        assert!(
            error.contains("spend principal kind mismatch")
                || error.contains("body_json principal_kind"),
            "{error}"
        );
        assert!(
            store
                .get_managed_acceptance_spend_authorization(&spend_id)
                .is_err(),
            "tampered spend owner reads must fail closed"
        );
    }

    #[test]
    fn managed_acceptance_owner_json_read_errors_fail_closed() {
        let (_dir, store) = store();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-owner-json")
                .unwrap();
        let decision_id = "mad-owner-json";
        let residual = "7e".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body_for_attempt(decision_id, "owner-json-attempt"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let risk = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: decision_id.into(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .into(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let risk_id = risk["authorization_id"].as_str().unwrap().to_string();
        let request = spend_req(&risk_id, "owner-json-attempt");
        let spend = store
            .issue_managed_acceptance_spend_authorization(&principal, &request)
            .unwrap();
        let spend_id = spend["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        store
            .admit_managed_acceptance_attempt_for_test(
                &principal,
                "owner-json-attempt",
                &attempt_body_for(&request),
                &spend_id,
                true,
            )
            .unwrap();

        let original_decision: String = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT body_json FROM managed_acceptance_decisions WHERE decision_id=?1",
                    [decision_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_decisions SET body_json='not-json' WHERE decision_id=?1",
                    [decision_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        assert!(store.get_managed_acceptance_decision(decision_id).is_err());
        let mut tampered_decision: Value = serde_json::from_str(&original_decision).unwrap();
        tampered_decision["trial_envelope"]["max_retries"] = json!(99);
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_decisions SET body_json=?1 WHERE decision_id=?2",
                    params![tampered_decision.to_string(), decision_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        assert!(
            store.get_managed_acceptance_decision(decision_id).is_err(),
            "a valid but hash-inconsistent decision body must fail closed"
        );
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_decisions SET body_json=?1 WHERE decision_id=?2",
                    params![original_decision, decision_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();

        let original_risk: String = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT body_json FROM managed_acceptance_authorizations WHERE authorization_id=?1",
                    [&risk_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_authorizations SET body_json='not-json' WHERE authorization_id=?1",
                    [&risk_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        assert!(store
            .get_active_managed_acceptance_authorization(&risk_id)
            .is_err());
        let mut tampered_risk: Value = serde_json::from_str(&original_risk).unwrap();
        tampered_risk["scope"]["decision_id"] = json!("other-decision");
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_authorizations SET body_json=?1 WHERE authorization_id=?2",
                    params![tampered_risk.to_string(), risk_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        assert!(
            store
                .get_active_managed_acceptance_authorization(&risk_id)
                .is_err(),
            "a valid but hash-inconsistent risk body must fail closed"
        );
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_authorizations SET body_json=?1 WHERE authorization_id=?2",
                    params![original_risk, risk_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();

        let original_spend: String = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT body_json FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=?1",
                    [&spend_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_spend_authorizations SET body_json='not-json' WHERE spend_authorization_id=?1",
                    [&spend_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        assert!(store
            .get_managed_acceptance_spend_authorization(&spend_id)
            .is_err());
        let mut tampered_spend: Value = serde_json::from_str(&original_spend).unwrap();
        tampered_spend["model"] = json!("different-model");
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_spend_authorizations SET body_json=?1 WHERE spend_authorization_id=?2",
                    params![tampered_spend.to_string(), spend_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        assert!(
            store
                .get_managed_acceptance_spend_authorization(&spend_id)
                .is_err(),
            "a valid but hash-inconsistent spend body must fail closed"
        );
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_spend_authorizations SET body_json=?1 WHERE spend_authorization_id=?2",
                    params![original_spend, spend_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();

        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_attempts SET receipt_json='not-json' WHERE attempt_id='owner-json-attempt'",
                    [],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        assert!(store
            .get_managed_acceptance_attempt("owner-json-attempt")
            .is_err());
    }

    #[test]
    fn concurrent_spend_issue_reuses_one_active_logical_receipt() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("spend-race.db");
        let store = LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-spend-race")
                .unwrap();
        let residual = "79".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body_for_attempt("mad-spend-race", "same-logical-attempt"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let risk = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-spend-race".into(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .into(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let risk_id = risk["authorization_id"].as_str().unwrap().to_string();
        let request = spend_req(&risk_id, "same-logical-attempt");
        let barrier = Arc::new(Barrier::new(3));
        let mut joins = Vec::new();
        for _ in 0..2 {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            let request = request.clone();
            joins.push(thread::spawn(move || {
                let concurrent =
                    LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string())
                        .unwrap();
                let principal = AuthenticatedPrincipal::fixture_for_tests(
                    "tenant-a",
                    "fixture-principal-spend-race",
                )
                .unwrap();
                barrier.wait();
                concurrent.issue_managed_acceptance_spend_authorization(&principal, &request)
            }));
        }
        barrier.wait();
        let spends = joins
            .into_iter()
            .map(|join| {
                join.join()
                    .unwrap()
                    .expect("concurrent logical spend issuance")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            spends[0]["spend_authorization_id"], spends[1]["spend_authorization_id"],
            "concurrent retries must collapse before UUID/timestamp receipt creation"
        );
        let connection = rusqlite::Connection::open(&path).unwrap();
        let active: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM managed_acceptance_spend_authorizations
                 WHERE tenant_id='tenant-a' AND status='active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active, 1, "only one active logical spend may persist");
    }

    #[test]
    fn active_spend_logical_identity_constraints_reject_null_and_duplicate_writes() {
        let (_dir, store) = store();
        let principal = AuthenticatedPrincipal::fixture_for_tests(
            "tenant-a",
            "fixture-principal-logical-constraint",
        )
        .unwrap();
        let residual = "7c".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body_for_attempt("mad-logical-constraint", "logical-attempt"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let risk = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-logical-constraint".into(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .into(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let request = spend_req(
            risk["authorization_id"].as_str().unwrap(),
            "logical-attempt",
        );
        let first = store
            .issue_managed_acceptance_spend_authorization(&principal, &request)
            .unwrap();
        let first_id = first["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        let mut second_body = first["body_json"].clone();
        second_body["spend_authorization_id"] = json!("mas-logical-duplicate-second");
        second_body["created_at"] = json!("2026-07-25T12:00:01Z");
        let second_sha = sha256_hex(canonical_json(&second_body).unwrap().as_bytes());
        let null_error = store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_spend_authorizations
                     SET logical_authorization_sha256=NULL WHERE spend_authorization_id=?1",
                    [&first_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .expect_err("V33 must reject an active spend with no logical identity");
        assert!(
            null_error.contains("CHECK constraint failed"),
            "unexpected active-null error: {null_error}"
        );
        let duplicate_error = store
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO managed_acceptance_spend_authorizations (
                        spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
                        principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
                        logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                        fixture_only, status, body_json, created_at, updated_at, expires_at,
                        consumed_at, consumed_by_attempt_id, revoked_at
                     ) SELECT ?1, decision_id, risk_authorization_id, tenant_id,
                        principal_kind, principal_id, ?2, risk_authorization_sha256,
                        logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                        fixture_only, 'active', ?3, created_at, updated_at, expires_at,
                        NULL, NULL, NULL
                     FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=?4",
                    rusqlite::params![
                        "mas-logical-duplicate-second",
                        second_sha,
                        second_body.to_string(),
                        first_id,
                    ],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .expect_err("V33 must reject a second active spend for the same logical identity");
        assert!(
            duplicate_error.contains("UNIQUE constraint failed"),
            "unexpected active-duplicate error: {duplicate_error}"
        );
        let replay = store
            .issue_managed_acceptance_spend_authorization(&principal, &request)
            .expect("the rejected raw writes must leave the original spend reusable");
        assert_eq!(
            replay["spend_authorization_id"],
            first["spend_authorization_id"]
        );
    }

    #[test]
    fn spend_envelope_requires_exact_attempt_target_binary_and_recovery_bindings() {
        let (_dir, store) = store();
        let principal = AuthenticatedPrincipal::fixture_for_tests(
            "tenant-a",
            "fixture-principal-envelope-bindings",
        )
        .unwrap();
        let residual = "7a".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body_for_attempt("mad-envelope-bindings", "bound-attempt"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let risk = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-envelope-bindings".into(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .into(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let base = spend_req(risk["authorization_id"].as_str().unwrap(), "bound-attempt");
        let mut cases = Vec::new();
        let mut changed = base.clone();
        changed.product_task_id = "other-product-task".into();
        cases.push(("product_task_id", changed));
        let mut changed = base.clone();
        changed.workflow_id = Some("other-workflow".into());
        cases.push(("workflow_id", changed));
        let mut changed = base.clone();
        changed.workflow_node_id = Some("other-node".into());
        cases.push(("workflow_node_id", changed));
        let mut changed = base.clone();
        changed.execution_id = "other-execution".into();
        cases.push(("execution_id", changed));
        let mut changed = base.clone();
        changed.attempt_id = "other-attempt".into();
        cases.push(("attempt_id", changed));
        let mut changed = base.clone();
        changed.target_repo = "org/other-target".into();
        cases.push(("target_repo", changed));
        let mut changed = base.clone();
        changed.target_main_sha = "b".repeat(40);
        cases.push(("target_main_sha", changed));
        let mut changed = base.clone();
        changed.binary_path = "/opt/other-codex".into();
        cases.push(("binary_path", changed));
        let mut changed = base.clone();
        changed.binary_sha256 = "cd".repeat(32);
        cases.push(("binary_sha256", changed));
        let mut changed = base.clone();
        changed.cancellation_identity = "other-cancel".into();
        cases.push(("cancellation_identity", changed));
        let mut changed = base.clone();
        changed.rollback_identity = "other-rollback".into();
        cases.push(("rollback_identity", changed));
        let mut changed = base.clone();
        changed.output_branch_prefix = "other/".into();
        cases.push(("output_branch_prefix", changed));
        let mut changed = base.clone();
        changed.draft_pr_only = false;
        cases.push(("draft_pr_only", changed));
        let mut changed = base.clone();
        changed.cost_authority = CostAuthority::ProviderReported {
            max_cost: 1.0,
            currency: "USD".into(),
        };
        cases.push(("cost_authority", changed));

        for (field, request) in cases {
            let error = store
                .issue_managed_acceptance_spend_authorization(&principal, &request)
                .expect_err(&format!("{field} mutation must not issue spend"));
            assert!(
                error.contains("mismatches decision trial envelope"),
                "{field}: unexpected error {error}"
            );
        }
    }

    #[test]
    fn spend_issuance_rejects_missing_persisted_risk_fixture_boolean() {
        let (_dir, store) = store();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-risk-boolean")
                .unwrap();
        let residual = "7d".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body_for_attempt("mad-risk-boolean", "risk-boolean-attempt"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let risk = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-risk-boolean".into(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .into(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let risk_id = risk["authorization_id"].as_str().unwrap().to_string();
        store
            .with_conn(|conn| {
                let raw: String = conn
                    .query_row(
                        "SELECT body_json FROM managed_acceptance_authorizations
                         WHERE authorization_id=?1",
                        [&risk_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let mut body: Value =
                    serde_json::from_str(&raw).map_err(|error| error.to_string())?;
                body.as_object_mut()
                    .ok_or("risk authorization body must be an object")?
                    .remove("fixture_only");
                conn.execute(
                    "UPDATE managed_acceptance_authorizations SET body_json=?1
                     WHERE authorization_id=?2",
                    params![body.to_string(), risk_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        let error = store
            .issue_managed_acceptance_spend_authorization(
                &principal,
                &spend_req(&risk_id, "risk-boolean-attempt"),
            )
            .expect_err("missing persisted risk boolean must fail closed");
        assert!(
            error.contains("fixture_only must be a persisted boolean")
                || error.contains("body fixture_only must be a boolean")
                || error.contains("body_json missing fixture_only"),
            "{error}"
        );
    }

    #[test]
    fn decision_status_changes_persist_hash_linked_transition_receipts() {
        let (_dir, store) = store();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-transition")
                .unwrap();
        let residual = "88".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body("mad-transition-receipts"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let decision_sha = decision["decision_body_sha256"]
            .as_str()
            .unwrap()
            .to_string();
        let authorization = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-transition-receipts".into(),
                    expected_decision_body_sha256: decision_sha.clone(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        store
            .revoke_managed_acceptance_authorization(
                &principal,
                authorization["authorization_id"].as_str().unwrap(),
            )
            .unwrap();

        let receipts = store
            .list_managed_acceptance_decision_transition_receipts("mad-transition-receipts")
            .unwrap();
        assert_eq!(receipts.len(), 2);
        let accepted = receipts
            .iter()
            .find(|receipt| receipt["to_status"] == "operator_accepted")
            .unwrap();
        let revoked = receipts
            .iter()
            .find(|receipt| receipt["to_status"] == "revoked")
            .unwrap();
        assert_eq!(accepted["decision_body_sha256"], decision_sha);
        assert_eq!(accepted["from_status"], "draft_pending_operator");
        assert_eq!(revoked["from_status"], "operator_accepted");
        assert_eq!(
            revoked["previous_transition_sha256"],
            accepted["transition_sha256"]
        );
        assert_eq!(accepted["transition_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(revoked["transition_sha256"].as_str().unwrap().len(), 64);
    }

    #[test]
    fn decision_transition_receipts_order_by_sequence_for_equal_timestamps() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("transition-order.db");
        let store = LocalProductStore::new_with_clock(&path, || "2026-07-26T00:00:00Z".to_string())
            .unwrap();
        let principal = AuthenticatedPrincipal::fixture_for_tests(
            "tenant-a",
            "fixture-principal-transition-order",
        )
        .unwrap();
        let decision_id = "mad-transition-sequence-order-02";
        let residual = "8b".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body(decision_id),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-27T00:00:00Z"),
            )
            .unwrap();
        let authorization = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: decision_id.to_string(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .into(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        store
            .revoke_managed_acceptance_authorization(
                &principal,
                authorization["authorization_id"].as_str().unwrap(),
            )
            .unwrap();

        let before = store
            .list_managed_acceptance_decision_transition_receipts(decision_id)
            .unwrap();
        assert_eq!(before.len(), 2);
        assert_eq!(before[0]["sequence"], 1);
        assert_eq!(before[1]["sequence"], 2);
        assert_eq!(before[1]["previous_transition_sequence"], 1);
    }

    #[test]
    fn decision_transition_receipt_content_tampering_fails_closed_on_read() {
        let (_dir, store) = store();
        let principal = AuthenticatedPrincipal::fixture_for_tests(
            "tenant-a",
            "fixture-principal-transition-tamper",
        )
        .unwrap();
        let decision_id = "mad-transition-tamper";
        let residual = "89".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body(decision_id),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: decision_id.into(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .into(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        store
            .with_conn(|conn| {
                let (receipt_id, encoded): (String, String) = conn
                    .query_row(
                        "SELECT transition_receipt_id, receipt_json
                         FROM managed_acceptance_decision_transition_receipts
                         WHERE decision_id=?1",
                        [decision_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(|error| error.to_string())?;
                let mut receipt: Value =
                    serde_json::from_str(&encoded).map_err(|error| error.to_string())?;
                receipt["reason"] = json!("tampered_after_persistence");
                conn.execute(
                    "UPDATE managed_acceptance_decision_transition_receipts
                     SET receipt_json=?1 WHERE transition_receipt_id=?2",
                    params![receipt.to_string(), receipt_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();

        let error = store
            .list_managed_acceptance_decision_transition_receipts(decision_id)
            .expect_err("tampered transition content must not be readable as evidence");
        assert!(error.contains("hash does not match content"), "{error}");
    }

    #[test]
    fn decision_transition_receipt_owner_constraints_reject_forked_writes() {
        let (_dir, store) = store();
        let principal = AuthenticatedPrincipal::fixture_for_tests(
            "tenant-a",
            "fixture-principal-transition-fork",
        )
        .unwrap();
        let decision_id = "mad-transition-fork";
        let residual = "8a".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body(decision_id),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let authorization = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: decision_id.into(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .into(),
                    expected_residual_finding_sha256: residual.clone(),
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        store
            .revoke_managed_acceptance_authorization(
                &principal,
                authorization["authorization_id"].as_str().unwrap(),
            )
            .unwrap();
        let accepted_sha = store
            .list_managed_acceptance_decision_transition_receipts(decision_id)
            .unwrap()
            .into_iter()
            .find(|receipt| receipt["to_status"] == "operator_accepted")
            .unwrap()["transition_sha256"]
            .as_str()
            .unwrap()
            .to_string();
        let forged = decision_status_transition_receipt(
            decision_id,
            "tenant-a",
            decision["decision_body_sha256"].as_str().unwrap(),
            &residual,
            2,
            Some(1),
            Some(&accepted_sha),
            "operator_accepted",
            "revoked",
            &principal,
            "2026-07-25T12:00:01Z",
            "forged_parallel_revocation",
        )
        .unwrap();
        let child_error = store
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO managed_acceptance_decision_transition_receipts (
                        transition_receipt_id, decision_id, tenant_id, decision_body_sha256,
                        previous_transition_sha256, transition_sha256, from_status, to_status,
                        actor_principal_kind, actor_principal_id, receipt_json, created_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                    params![
                        forged["transition_receipt_id"].as_str().unwrap(),
                        decision_id,
                        "tenant-a",
                        decision["decision_body_sha256"].as_str().unwrap(),
                        forged["previous_transition_sha256"].as_str().unwrap(),
                        forged["transition_sha256"].as_str().unwrap(),
                        forged["from_status"].as_str().unwrap(),
                        forged["to_status"].as_str().unwrap(),
                        forged["actor_principal_kind"].as_str().unwrap(),
                        forged["actor_principal_id"].as_str().unwrap(),
                        forged.to_string(),
                        forged["at"].as_str().unwrap(),
                    ],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .expect_err("V32 must reject a second child transition from one predecessor");
        assert!(
            child_error.contains("UNIQUE constraint failed"),
            "unexpected child fork error: {child_error}"
        );

        let forged_genesis = decision_status_transition_receipt(
            decision_id,
            "tenant-a",
            decision["decision_body_sha256"].as_str().unwrap(),
            &residual,
            1,
            None,
            None,
            "draft_pending_operator",
            "operator_accepted",
            &principal,
            "2026-07-25T12:00:02Z",
            "forged_second_genesis",
        )
        .unwrap();
        let genesis_error = store
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO managed_acceptance_decision_transition_receipts (
                        transition_receipt_id, decision_id, tenant_id, decision_body_sha256,
                        previous_transition_sha256, transition_sha256, from_status, to_status,
                        actor_principal_kind, actor_principal_id, receipt_json, created_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                    params![
                        forged_genesis["transition_receipt_id"].as_str().unwrap(),
                        decision_id,
                        "tenant-a",
                        decision["decision_body_sha256"].as_str().unwrap(),
                        Option::<String>::None,
                        forged_genesis["transition_sha256"].as_str().unwrap(),
                        forged_genesis["from_status"].as_str().unwrap(),
                        forged_genesis["to_status"].as_str().unwrap(),
                        forged_genesis["actor_principal_kind"].as_str().unwrap(),
                        forged_genesis["actor_principal_id"].as_str().unwrap(),
                        forged_genesis.to_string(),
                        forged_genesis["at"].as_str().unwrap(),
                    ],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .expect_err("V32 must reject a second genesis transition");
        assert!(
            genesis_error.contains("UNIQUE constraint failed"),
            "unexpected genesis fork error: {genesis_error}"
        );

        let receipts = store
            .list_managed_acceptance_decision_transition_receipts(decision_id)
            .expect("rejected writes must leave the persisted transition chain valid");
        assert_eq!(receipts.len(), 2);
    }

    #[test]
    fn concurrent_admission_single_winner() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("race.db");
        let store = LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-race")
                .unwrap();
        let residual = "11".repeat(32);
        let body = decision_body_for_attempt("mad-race", "race-1");
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &body,
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let dsha = decision["decision_body_sha256"]
            .as_str()
            .unwrap()
            .to_string();
        let auth = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-race".into(),
                    expected_decision_body_sha256: dsha,
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let auth_id = auth["authorization_id"].as_str().unwrap().to_string();
        let spend_request = spend_req(&auth_id, "race-1");
        let spend = store
            .issue_managed_acceptance_spend_authorization(&principal, &spend_request)
            .unwrap();
        let spend_id = spend["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        let attempt_body = attempt_body_for(&spend_request);
        let barrier = Arc::new(Barrier::new(2));
        let path2 = path.clone();
        let spend_a = spend_id.clone();
        let spend_b = spend_id.clone();
        let body_a = attempt_body.clone();
        let body_b = attempt_body.clone();
        let b1 = Arc::clone(&barrier);
        let b2 = Arc::clone(&barrier);
        let h1 = thread::spawn(move || {
            let s =
                LocalProductStore::new_with_clock(&path2, || "2026-07-25T12:00:00Z".to_string())
                    .unwrap();
            let p = AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-race")
                .unwrap();
            b1.wait();
            s.admit_managed_acceptance_attempt_for_test(&p, "race-1", &body_a, &spend_a, true)
        });
        let path3 = path.clone();
        let h2 = thread::spawn(move || {
            let s =
                LocalProductStore::new_with_clock(&path3, || "2026-07-25T12:00:00Z".to_string())
                    .unwrap();
            let p = AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-race")
                .unwrap();
            b2.wait();
            s.admit_managed_acceptance_attempt_for_test(&p, "race-1", &body_b, &spend_b, true)
        });
        let r1 = h1.join().unwrap();
        let r2 = h2.join().unwrap();
        let ok = [r1.is_ok(), r2.is_ok()].iter().filter(|x| **x).count();
        // One admits, the other may idempotent-replay or fail on spend consumed.
        assert!(ok >= 1, "at least one admission must succeed");
        let final_spend = store
            .get_managed_acceptance_spend_authorization(&spend_id)
            .unwrap()
            .unwrap();
        assert_eq!(final_spend["status"], "consumed");
        let attempt = store
            .get_managed_acceptance_attempt("race-1")
            .unwrap()
            .unwrap();
        assert_eq!(attempt["status"], "admitted");
    }

    #[test]
    fn revocation_blocks_spend() {
        let (_dir, store) = store();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-bob").unwrap();
        let residual = "33".repeat(32);
        let body = decision_body("mad-rev");
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &body,
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let dsha = decision["decision_body_sha256"]
            .as_str()
            .unwrap()
            .to_string();
        let auth = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-rev".into(),
                    expected_decision_body_sha256: dsha,
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let auth_id = auth["authorization_id"].as_str().unwrap().to_string();
        store
            .revoke_managed_acceptance_authorization(&principal, &auth_id)
            .unwrap();
        let err = store
            .issue_managed_acceptance_spend_authorization(&principal, &spend_req(&auth_id, "x"))
            .unwrap_err();
        assert!(err.contains("not active") || err.contains("revok"), "{err}");
    }

    #[test]
    fn finite_expiry_mandatory_and_bound_into_decision_hash() {
        let (_dir, store) = store();
        let body = decision_body("mad-exp");
        let residual = "44".repeat(32);
        let err = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &body,
                &residual,
                "draft_pending_operator",
                None,
                None,
            )
            .unwrap_err();
        assert!(err.contains("finite expires_at"), "{err}");
        let err_far = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &body,
                &residual,
                "draft_pending_operator",
                None,
                Some("2099-01-01T00:00:00Z"),
            )
            .unwrap_err();
        assert!(
            err_far.contains("far-future") || err_far.contains("finite"),
            "{err_far}"
        );
        let d1 = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &body,
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let d2 = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body("mad-exp-2"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-27T00:00:00Z"),
            )
            .unwrap();
        assert_ne!(
            d1["decision_body_sha256"], d2["decision_body_sha256"],
            "different expiry must change canonical authority hash"
        );
    }

    #[test]
    fn spend_budget_mismatch_against_decision_rejects() {
        let (_dir, store) = store();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-budget")
                .unwrap();
        let residual = "55".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body_for_attempt("mad-budget", "bad-budget"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let dsha = decision["decision_body_sha256"]
            .as_str()
            .unwrap()
            .to_string();
        let auth = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-budget".into(),
                    expected_decision_body_sha256: dsha,
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let auth_id = auth["authorization_id"].as_str().unwrap().to_string();
        let mut bad = spend_req(&auth_id, "bad-budget");
        bad.max_provider_requests = 99;
        let err = store
            .issue_managed_acceptance_spend_authorization(&principal, &bad)
            .unwrap_err();
        assert!(err.contains("max_provider_requests"), "{err}");
        bad = spend_req(&auth_id, "bad-budget");
        bad.model = "mutated-model".into();
        let err = store
            .issue_managed_acceptance_spend_authorization(&principal, &bad)
            .unwrap_err();
        assert!(err.contains("model"), "{err}");
    }

    #[test]
    fn attempt_body_mismatch_rejects_before_spend_consumption() {
        let (_dir, store) = store();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-mismatch")
                .unwrap();
        let residual = "66".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body_for_attempt("mad-mismatch", "mismatch-1"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let dsha = decision["decision_body_sha256"]
            .as_str()
            .unwrap()
            .to_string();
        let auth = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-mismatch".into(),
                    expected_decision_body_sha256: dsha,
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let auth_id = auth["authorization_id"].as_str().unwrap().to_string();
        let req = spend_req(&auth_id, "mismatch-1");
        let spend = store
            .issue_managed_acceptance_spend_authorization(&principal, &req)
            .unwrap();
        let spend_id = spend["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        let mut body = attempt_body_for(&req);
        body["model"] = json!("wrong-model");
        let err = store
            .admit_managed_acceptance_attempt_for_test(
                &principal,
                "mismatch-1",
                &body,
                &spend_id,
                true,
            )
            .unwrap_err();
        assert!(err.contains("mismatch") || err.contains("model"), "{err}");
        // Spend must remain active (not consumed on mismatch).
        let spend_row = store
            .get_managed_acceptance_spend_authorization(&spend_id)
            .unwrap()
            .unwrap();
        assert_eq!(spend_row["status"], "active");
    }

    #[test]
    fn decision_body_hash_stable_across_accept_status_transition() {
        let (_dir, store) = store();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-stable")
                .unwrap();
        let residual = "88".repeat(32);
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body("mad-stable"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-26T00:00:00Z"),
            )
            .unwrap();
        let dsha = decision["decision_body_sha256"]
            .as_str()
            .unwrap()
            .to_string();
        store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-stable".into(),
                    expected_decision_body_sha256: dsha.clone(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let after = store
            .get_managed_acceptance_decision("mad-stable")
            .unwrap()
            .unwrap();
        assert_eq!(after["status"], "operator_accepted");
        assert_eq!(
            after["decision_body_sha256"].as_str().unwrap(),
            dsha.as_str(),
            "mutable status must not rewrite the immutable decision authority hash"
        );
    }

    #[test]
    fn rfc3339_offset_expiry_compared_by_instant_not_lexically() {
        // "2026-07-25T12:00:00+02:00" is 10:00Z — before store now 12:00Z — must be expired.
        let dir = tempdir().unwrap();
        let path = dir.path().join("offset.db");
        let store = LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-offset")
                .unwrap();
        let residual = "99".repeat(32);
        // Create with far enough expiry first.
        let decision = store.upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body("mad-offset"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-25T14:00:00+02:00"), // 12:00Z == now boundary after normalize?
        );
        // 14:00+02:00 == 12:00Z; require strictly after now → should fail.
        assert!(decision.is_err(), "expiry at exact now must fail");
        let decision = store
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body("mad-offset"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-25T15:00:00+02:00"), // 13:00Z > 12:00Z
            )
            .unwrap();
        let dsha = decision["decision_body_sha256"]
            .as_str()
            .unwrap()
            .to_string();
        // Advance to after expiry (15:00+02 = 13:00Z; set now to 14:00Z).
        let store_late =
            LocalProductStore::new_with_clock(&path, || "2026-07-25T14:00:00Z".to_string())
                .unwrap();
        let err = store_late
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-offset".into(),
                    expected_decision_body_sha256: dsha,
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap_err();
        assert!(err.contains("expir"), "{err}");
    }

    #[test]
    fn expired_decision_rejects_accept_and_spend() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("exp.db");
        // Create decision while clock is early.
        let store_early =
            LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string())
                .unwrap();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-exp").unwrap();
        let residual = "77".repeat(32);
        let decision = store_early
            .upsert_managed_acceptance_decision(
                "tenant-a",
                &decision_body("mad-time"),
                &residual,
                "draft_pending_operator",
                None,
                Some("2026-07-25T13:00:00Z"),
            )
            .unwrap();
        let dsha = decision["decision_body_sha256"]
            .as_str()
            .unwrap()
            .to_string();
        // Advance clock past expiry.
        let store_late =
            LocalProductStore::new_with_clock(&path, || "2026-07-25T14:00:00Z".to_string())
                .unwrap();
        let err = store_late
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: "mad-time".into(),
                    expected_decision_body_sha256: dsha,
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                    explicit_go: true,
                },
            )
            .unwrap_err();
        assert!(err.contains("expir"), "{err}");
    }

    fn delegated_contract() -> DelegationContract {
        DelegationContract {
            schema_version: DELEGATION_SCHEMA_VERSION.into(),
            delegation_id: "delegation-golden-path-1".into(),
            created_at: "2026-07-25T12:00:00Z".into(),
            expires_at: "2026-07-26T12:00:00Z".into(),
            executions: 1,
            repositories: vec!["Igzela/alters-lab".into()],
            task_classes: vec!["documentation".into()],
            allowed_paths: vec!["docs/USER_GUIDE.md".into()],
            max_changed_files: 1,
            max_changed_lines: 100,
            max_cost_usd_per_run: 0.50,
            max_total_cost_usd: 0.50,
            protocol: "openai_compatible".into(),
            models: json!({
                "planner": "deepseek-v4-pro",
                "implementer": "deepseek-v4-flash",
                "reviewer": "deepseek-v4-pro"
            }),
            output: json!({
                "draft_pr_only": true,
                "target_main_write": false,
                "merge": false,
                "auto_merge": false
            }),
            forbidden: vec![
                "credential changes".into(),
                "authentication or permission changes".into(),
                "schema or database migrations".into(),
                "dependency changes".into(),
                "executable or workflow changes".into(),
                "destructive operations".into(),
                "release".into(),
                "deployment".into(),
            ],
        }
    }

    fn delegated_proposal() -> Value {
        let mut proposal = json!({
            "schema_version": "managed_proposal_manifest.v1",
            "target_repository": "Igzela/alters-lab",
            "target_main_sha": "6".repeat(40),
            "mutable_paths": ["docs/USER_GUIDE.md"],
            "max_cost_usd": null,
            "verifier": "deterministic_docs_health_check_v1",
            "provider_execution_binding": crate::rwe::campaign_package::canonical_deepseek_provider_binding().to_json()
        });
        proposal["manifest_sha256"] = json!(compute_attempt_manifest_sha256(&proposal).unwrap());
        proposal
    }

    fn delegated_execution() -> Value {
        json!({
            "observed_at": "2026-07-25T12:00:00Z",
            "target_repository": "Igzela/alters-lab",
            "target_main_sha": "6".repeat(40),
            "tenant_id": "tenant-a",
            "product_task_id": "product-task-golden-path-1",
            "workflow_id": "workflow-golden-path-1",
            "workflow_node_ids": ["planning", "implementation", "verification", "review"],
            "attempt_id": "attempt-golden-path-1",
            "verifier": "deterministic_docs_health_check_v1",
            "mutable_paths": ["docs/USER_GUIDE.md"],
            "cancellation_identity": "cancel-golden-path-1",
            "rollback_identity": "rollback-golden-path-1"
        })
    }

    fn delegated_test_principal(principal_id: &str) -> AuthenticatedPrincipal {
        AuthenticatedPrincipal {
            tenant_id: "tenant-a".into(),
            principal_id: principal_id.into(),
            principal_kind: PrincipalKind::OperatorApiKey,
            scopes: ALL_MANAGED_ACCEPTANCE_SCOPES
                .iter()
                .map(|scope| (*scope).to_string())
                .collect(),
            user_id: format!("user-{principal_id}"),
            role: "owner".into(),
        }
    }

    fn effect_envelope_for(
        decision_id: &str,
        envelope_id: &str,
        max_total_cost_usd: f64,
        max_child_authorizations: u64,
        expires_at: &str,
    ) -> EffectEnvelopeContract {
        EffectEnvelopeContract {
            schema_version: EFFECT_ENVELOPE_SCHEMA_VERSION.into(),
            decision_id: decision_id.into(),
            envelope_id: envelope_id.into(),
            owner_principal_id: "effect-owner".into(),
            effect_kind: "repository_maintenance".into(),
            target_repository: "Igzela/token-efficient-agent-harness-lab".into(),
            target_main_sha: "a".repeat(40),
            max_total_cost_usd,
            max_child_authorizations,
            created_at: "2026-07-25T12:00:00Z".into(),
            expires_at: expires_at.into(),
        }
    }

    fn effect_child_for(
        child_authorization_id: &str,
        operation_id: &str,
        max_cost_usd: f64,
        expires_at: &str,
    ) -> EffectChildAuthorizationRequest {
        EffectChildAuthorizationRequest {
            child_authorization_id: child_authorization_id.into(),
            operation_id: operation_id.into(),
            effect_kind: "repository_maintenance".into(),
            target_repository: "Igzela/token-efficient-agent-harness-lab".into(),
            target_main_sha: "a".repeat(40),
            max_cost_usd,
            expires_at: expires_at.into(),
        }
    }

    fn effect_evidence(child_authorization_id: &str, effect_executed: bool) -> Value {
        let payload = json!({
            "kind": "provider_free_canary",
            "child_authorization_id": child_authorization_id,
            "effect_executed": effect_executed,
        });
        let evidence_sha256 = sha256_hex(canonical_json(&payload).unwrap().as_bytes());
        json!({
            "evidence": payload,
            "evidence_sha256": evidence_sha256,
        })
    }

    #[test]
    fn effect_envelope_validation_is_immutable_and_bounded() {
        let mut envelope = effect_envelope_for(
            "effect-validation-decision",
            "effect-validation-envelope",
            0.75,
            2,
            "2026-07-26T12:00:00Z",
        );
        envelope.validate("2026-07-25T12:00:00Z").unwrap();

        envelope.target_main_sha = "not-a-commit".into();
        assert!(envelope.validate("2026-07-25T12:00:00Z").is_err());
        envelope.target_main_sha = "a".repeat(40);
        envelope.expires_at = "2026-07-25T11:59:59Z".into();
        assert!(envelope.validate("2026-07-25T12:00:00Z").is_err());
        envelope.expires_at = "2026-07-26T12:00:00Z".into();
        envelope.max_total_cost_usd = f64::NAN;
        assert!(envelope.validate("2026-07-25T12:00:00Z").is_err());
    }

    #[test]
    fn effect_parent_children_are_one_use_bounded_revocable_and_unknown_is_not_retryable() {
        let (dir, store) = store();
        let principal = delegated_test_principal("effect-owner");
        let residual = "b".repeat(64);
        let envelope = effect_envelope_for(
            "effect-parent-decision",
            "effect-parent-envelope",
            0.75,
            2,
            "2026-07-26T12:00:00Z",
        );
        let approved = store
            .approve_effect_envelope(&principal, &envelope, &residual)
            .unwrap();
        let parent_authorization_id = approved["authorization_id"].as_str().unwrap().to_string();

        let child_one = effect_child_for(
            "effect-child-one",
            "effect-operation-one",
            0.50,
            "2026-07-26T12:00:00Z",
        );
        let other_principal = delegated_test_principal("other-effect-owner");
        assert!(store
            .derive_effect_child_authorization(
                &other_principal,
                &parent_authorization_id,
                &child_one,
            )
            .is_err());
        let first = store
            .derive_effect_child_authorization(&principal, &parent_authorization_id, &child_one)
            .unwrap();
        assert_eq!(first["status"], "active");
        assert_eq!(first["one_use"], true);
        let replay = store
            .derive_effect_child_authorization(&principal, &parent_authorization_id, &child_one)
            .unwrap();
        assert_eq!(replay["replayed"], true);

        let mut mismatched_target = child_one.clone();
        mismatched_target.child_authorization_id = "effect-child-target-mismatch".into();
        mismatched_target.target_repository = "another/repository".into();
        assert!(store
            .derive_effect_child_authorization(
                &principal,
                &parent_authorization_id,
                &mismatched_target,
            )
            .is_err());
        let mut late_child = child_one.clone();
        late_child.child_authorization_id = "effect-child-late".into();
        late_child.operation_id = "effect-operation-late".into();
        late_child.expires_at = "2026-07-27T00:00:00Z".into();
        assert!(store
            .derive_effect_child_authorization(&principal, &parent_authorization_id, &late_child)
            .is_err());

        let child_two = effect_child_for(
            "effect-child-two",
            "effect-operation-two",
            0.25,
            "2026-07-26T12:00:00Z",
        );
        store
            .derive_effect_child_authorization(&principal, &parent_authorization_id, &child_two)
            .unwrap();
        let child_three = effect_child_for(
            "effect-child-three",
            "effect-operation-three",
            0.01,
            "2026-07-26T12:00:00Z",
        );
        assert!(store
            .derive_effect_child_authorization(&principal, &parent_authorization_id, &child_three,)
            .is_err());

        let revoked = store
            .revoke_managed_acceptance_authorization(&principal, &parent_authorization_id)
            .unwrap();
        assert_eq!(revoked["status"], "revoked");
        assert_eq!(
            store
                .get_managed_acceptance_spend_authorization("effect-child-one")
                .unwrap()
                .unwrap()["status"],
            "revoked"
        );
        assert!(store
            .derive_effect_child_authorization(&principal, &parent_authorization_id, &child_three)
            .is_err());

        let budget_envelope = effect_envelope_for(
            "effect-budget-decision",
            "effect-budget-envelope",
            0.60,
            3,
            "2026-07-26T12:00:00Z",
        );
        let budget_approved = store
            .approve_effect_envelope(&principal, &budget_envelope, &residual)
            .unwrap();
        let budget_parent = budget_approved["authorization_id"].as_str().unwrap();
        store
            .derive_effect_child_authorization(
                &principal,
                budget_parent,
                &effect_child_for(
                    "effect-budget-child-one",
                    "effect-budget-operation-one",
                    0.50,
                    "2026-07-26T12:00:00Z",
                ),
            )
            .unwrap();
        assert!(store
            .derive_effect_child_authorization(
                &principal,
                budget_parent,
                &effect_child_for(
                    "effect-budget-child-two",
                    "effect-budget-operation-two",
                    0.20,
                    "2026-07-26T12:00:00Z",
                ),
            )
            .is_err());

        let unknown_envelope = effect_envelope_for(
            "effect-unknown-decision",
            "effect-unknown-envelope",
            0.10,
            2,
            "2026-07-26T12:00:00Z",
        );
        let unknown_approved = store
            .approve_effect_envelope(&principal, &unknown_envelope, &residual)
            .unwrap();
        let unknown_parent = unknown_approved["authorization_id"].as_str().unwrap();
        let unknown_child = effect_child_for(
            "effect-unknown-child",
            "effect-unknown-operation",
            0.10,
            "2026-07-26T12:00:00Z",
        );
        store
            .derive_effect_child_authorization(&principal, unknown_parent, &unknown_child)
            .unwrap();
        let evidence = effect_evidence("effect-unknown-child", false);
        let settled = store
            .settle_effect_child_authorization(
                &principal,
                "effect-unknown-child",
                "effect-unknown-attempt",
                EFFECT_OUTCOME_UNKNOWN,
                &evidence,
            )
            .unwrap();
        assert_eq!(settled["status"], EFFECT_OUTCOME_UNKNOWN);
        assert_eq!(settled["terminal_class"], "effect_outcome_unknown");
        assert_eq!(settled["receipt_json"]["outcome_unknown_no_retry"], true);
        assert!(store
            .derive_effect_child_authorization(
                &principal,
                unknown_parent,
                &effect_child_for(
                    "effect-unknown-child-two",
                    "effect-unknown-operation-two",
                    0.01,
                    "2026-07-26T12:00:00Z",
                ),
            )
            .is_err());
        let settlement_replay = store
            .settle_effect_child_authorization(
                &principal,
                "effect-unknown-child",
                "effect-unknown-attempt",
                EFFECT_OUTCOME_UNKNOWN,
                &evidence,
            )
            .unwrap();
        assert_eq!(settlement_replay["idempotent_replay"], true);
        let conflicting_evidence = effect_evidence("effect-unknown-child", true);
        assert!(store
            .settle_effect_child_authorization(
                &principal,
                "effect-unknown-child",
                "effect-unknown-attempt",
                EFFECT_OUTCOME_UNKNOWN,
                &conflicting_evidence,
            )
            .is_err());

        let expiry_envelope = effect_envelope_for(
            "effect-expiry-decision",
            "effect-expiry-envelope",
            0.10,
            1,
            "2026-07-26T12:00:00Z",
        );
        assert!(expiry_envelope.validate("2026-07-26T12:00:01Z").is_err());
        drop(dir);
    }

    #[test]
    fn effect_outcome_evidence_is_strict_bounded_and_digest_bound() {
        let valid = effect_evidence("effect-evidence-child", false);
        assert!(validate_effect_outcome_evidence(&valid).is_ok());

        let nested = json!({
            "evidence": {
                "kind": "provider_free_canary",
                "child_authorization_id": "effect-evidence-child",
                "effect_executed": false,
                "metadata": {"output": "must not persist"}
            },
            "evidence_sha256": "a".repeat(64)
        });
        assert!(validate_effect_outcome_evidence(&nested).is_err());

        let oversized = json!({
            "evidence": {
                "kind": "provider_free_canary",
                "child_authorization_id": "x".repeat(257),
                "effect_executed": false
            },
            "evidence_sha256": "b".repeat(64)
        });
        assert!(validate_effect_outcome_evidence(&oversized).is_err());

        let sensitive_child_id = ["api_", "key=secret-value"].concat();
        let sensitive = json!({
            "evidence": {
                "kind": "provider_free_canary",
                "child_authorization_id": sensitive_child_id,
                "effect_executed": false
            },
            "evidence_sha256": "c".repeat(64)
        });
        assert!(validate_effect_outcome_evidence(&sensitive).is_err());
    }

    #[test]
    fn effect_child_settlement_retains_terminal_evidence_after_expiry() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("effect-expiry.db");
        let early = LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
        let principal = delegated_test_principal("effect-owner");
        let envelope = effect_envelope_for(
            "effect-expiry-settlement-decision",
            "effect-expiry-settlement-envelope",
            0.10,
            1,
            "2026-07-25T13:00:00Z",
        );
        let residual = "d".repeat(64);
        let approved = early
            .approve_effect_envelope(&principal, &envelope, &residual)
            .unwrap();
        let parent_id = approved["authorization_id"].as_str().unwrap();
        let child = effect_child_for(
            "effect-expiry-settlement-child",
            "effect-expiry-settlement-operation",
            0.10,
            "2026-07-25T13:00:00Z",
        );
        early
            .derive_effect_child_authorization(&principal, parent_id, &child)
            .unwrap();
        drop(early);

        let late = LocalProductStore::new_with_clock(&path, || "2026-07-25T14:00:00Z".to_string())
            .unwrap();
        assert!(late
            .derive_effect_child_authorization(
                &principal,
                parent_id,
                &effect_child_for(
                    "effect-expiry-settlement-new-child",
                    "effect-expiry-settlement-new-operation",
                    0.01,
                    "2026-07-25T14:30:00Z",
                ),
            )
            .is_err());
        let settled = late
            .settle_effect_child_authorization(
                &principal,
                "effect-expiry-settlement-child",
                "effect-expiry-settlement-attempt",
                EFFECT_OUTCOME_UNKNOWN,
                &effect_evidence("effect-expiry-settlement-child", false),
            )
            .unwrap();
        assert_eq!(settled["status"], EFFECT_OUTCOME_UNKNOWN);
        assert_eq!(settled["terminal_class"], "effect_outcome_unknown");
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn pg_effect_parent_child_and_unknown_outcome_have_sqlite_parity() {
        let Ok(database_url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            eprintln!("ACP_TEST_DATABASE_URL not set; skipping effect PostgreSQL parity test");
            return;
        };
        let suffix = format!(
            "{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        let store =
            LocalProductStore::new_postgres(&database_url, || "2026-07-25T12:00:00Z".to_string())
                .unwrap();
        let principal = delegated_test_principal("effect-owner");
        let envelope = effect_envelope_for(
            &format!("pg-effect-decision-{suffix}"),
            &format!("pg-effect-envelope-{suffix}"),
            0.20,
            2,
            "2026-07-26T12:00:00Z",
        );
        let residual = "e".repeat(64);
        let approved = store
            .approve_effect_envelope(&principal, &envelope, &residual)
            .unwrap();
        let parent_authorization_id = approved["authorization_id"].as_str().unwrap();
        let child_id = format!("pg-effect-child-{suffix}");
        store
            .derive_effect_child_authorization(
                &principal,
                parent_authorization_id,
                &effect_child_for(
                    &child_id,
                    &format!("pg-effect-operation-{suffix}"),
                    0.20,
                    "2026-07-26T12:00:00Z",
                ),
            )
            .unwrap();
        let attempt_id = format!("pg-effect-attempt-{suffix}");
        let outcome = store
            .settle_effect_child_authorization(
                &principal,
                &child_id,
                &attempt_id,
                EFFECT_OUTCOME_UNKNOWN,
                &effect_evidence(&child_id, false),
            )
            .unwrap();
        assert_eq!(outcome["status"], EFFECT_OUTCOME_UNKNOWN);
        assert!(store
            .derive_effect_child_authorization(
                &principal,
                parent_authorization_id,
                &effect_child_for(
                    &format!("pg-effect-child-two-{suffix}"),
                    &format!("pg-effect-operation-two-{suffix}"),
                    0.01,
                    "2026-07-26T12:00:00Z",
                ),
            )
            .is_err());
    }

    #[test]
    fn managed_workspace_descriptor_traversal_rejects_symlinks_and_directory_swap_races() {
        use std::os::unix::fs::symlink;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let dir = tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(workspace.join("docs")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(workspace.join("docs/USER_GUIDE.md"), "safe").unwrap();
        std::fs::write(outside.join("USER_GUIDE.md"), "outside").unwrap();

        let final_link = workspace.join("docs/FINAL_LINK.md");
        symlink(outside.join("USER_GUIDE.md"), &final_link).unwrap();
        assert!(open_absolute_path_without_symlinks(&final_link, libc::O_RDONLY).is_err());
        std::fs::remove_file(final_link).unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let running_swapper = running.clone();
        let workspace_swapper = workspace.clone();
        let outside_swapper = outside.clone();
        let swapper = std::thread::spawn(move || {
            let docs = workspace_swapper.join("docs");
            let real = workspace_swapper.join("docs.real");
            while running_swapper.load(Ordering::SeqCst) {
                if std::fs::rename(&docs, &real).is_ok() {
                    let _ = symlink(&outside_swapper, &docs);
                    let _ = std::fs::remove_file(&docs);
                    let _ = std::fs::rename(&real, &docs);
                }
            }
            let _ = std::fs::remove_file(&docs);
            let _ = std::fs::rename(&real, &docs);
        });
        for _ in 0..1_000 {
            if let Ok(mut file) = open_absolute_path_without_symlinks(
                &workspace.join("docs/USER_GUIDE.md"),
                libc::O_RDONLY,
            ) {
                let mut content = String::new();
                file.read_to_string(&mut content).unwrap();
                assert_eq!(content, "safe");
            }
        }
        running.store(false, Ordering::SeqCst);
        swapper.join().unwrap();
    }

    #[test]
    fn delegated_manifest_spend_lease_and_terminal_are_restart_safe() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("delegated.db");
        let store =
            LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".into()).unwrap();
        seed_key(&store, "tenant-a", "delegated-operator");
        let principal = store
            .authenticate_managed_acceptance_principal("tenant-a", "delegated-operator", Some(1.0))
            .unwrap();
        let delegation = delegated_contract();
        let persisted = store.persist_delegation(&principal, &delegation).unwrap();
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "INSERT INTO product_tasks (
                            task_id, schema_version, tenant_id, workspace_id, idempotency_key,
                            status, version, objective_fingerprint, target_id, target_repo_path,
                            source_revision, source_tree_hash, output_intent, risk_class,
                            approval_required, confirm_execution, confirm_output,
                            intake_contract_sha256, intake_json, workspace_binding_json,
                            plan_id, run_id, workspace_record_id, failure_code, failure_detail,
                            created_at, updated_at, created_by
                         ) VALUES (
                            ?1, 'product_task.v1', 'tenant-a', 'default',
                            'provider-journal-restart', 'graph_ready', 1, ?2,
                            'alters-lab-docs', '/redacted/target', ?3, NULL, 'draft_pr', 'low',
                            1, 1, 1, ?4, '{}', ?5, NULL, 'run-provider-journal-1',
                            NULL, NULL, NULL, ?6, ?6, 'test'
                         )",
                        params![
                            "product-task-golden-path-1",
                            "a".repeat(64),
                            "b".repeat(40),
                            "c".repeat(64),
                            json!({
                                "allowed_paths": ["docs/USER_GUIDE.md"],
                                "workspace_path": "/redacted/not-used"
                            })
                            .to_string(),
                            "2026-07-25T12:00:00Z"
                        ],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        store
            .bind_delegation_to_product_task(
                &principal,
                "product-task-golden-path-1",
                &delegation.delegation_id,
            )
            .unwrap();
        let proposal = delegated_proposal();
        store
            .persist_approved_delegated_proposal(
                &delegation.delegation_id,
                &proposal,
                proposal["manifest_sha256"].as_str().unwrap(),
            )
            .unwrap();
        let mut mutated_proposal = proposal.clone();
        mutated_proposal["target_main_sha"] = json!("7".repeat(40));
        mutated_proposal["manifest_sha256"] = Value::Null;
        mutated_proposal["manifest_sha256"] =
            json!(compute_attempt_manifest_sha256(&mutated_proposal).unwrap());
        let mut mutated_execution = delegated_execution();
        mutated_execution["target_main_sha"] = json!("7".repeat(40));
        let mutated_manifest =
            derive_final_execution_manifest(&mutated_proposal, &delegation, &mutated_execution)
                .unwrap();
        assert!(store
            .approve_delegated_manifest(&principal, &delegation.delegation_id, &mutated_manifest,)
            .unwrap_err()
            .contains("approved proposal"));
        let manifest =
            derive_final_execution_manifest(&proposal, &delegation, &delegated_execution())
                .unwrap();
        let approval = store
            .approve_delegated_manifest(&principal, &delegation.delegation_id, &manifest)
            .unwrap();
        let replayed_approval = store
            .approve_delegated_manifest(&principal, &delegation.delegation_id, &manifest)
            .unwrap();
        assert_eq!(approval, replayed_approval);
        let spend = store
            .issue_delegated_spend(
                &principal,
                &delegation.delegation_id,
                approval["approval_receipt_sha256"].as_str().unwrap(),
                &manifest,
            )
            .unwrap();
        assert_eq!(persisted["status"], "active");
        assert_eq!(spend["status"], "active");
        assert!(store
            .admit_delegated_attempt(
                &principal,
                &delegation.delegation_id,
                "attempt-golden-path-1",
                &manifest,
            )
            .unwrap_err()
            .contains("stale or mismatched"));
        assert_eq!(
            store
                .issue_delegated_spend(
                    &principal,
                    &delegation.delegation_id,
                    approval["approval_receipt_sha256"].as_str().unwrap(),
                    &manifest,
                )
                .unwrap(),
            spend
        );
        let activator = delegated_test_principal("delegated-activator");
        let lease = store
            .admit_delegated_attempt(
                &activator,
                &delegation.delegation_id,
                "attempt-golden-path-1",
                &manifest,
            )
            .unwrap();
        assert_eq!(lease["status"], "admitted");
        assert!(store
            .admit_delegated_attempt(
                &delegated_test_principal("delegated-activator"),
                &delegation.delegation_id,
                "attempt-golden-path-2",
                &manifest,
            )
            .is_err());
        let delegated_binding = crate::provider::managed_deepseek::ManagedCallBinding {
            product_task_id: "product-task-golden-path-1".into(),
            workflow_id: "workflow-golden-path-1".into(),
            node_id: "planning".into(),
            attempt_id: "attempt-golden-path-1".into(),
            spend_authorization_id: spend["spend_authorization_id"].as_str().unwrap().into(),
            attempt_lease_id: lease["attempt_lease_id"].as_str().unwrap().into(),
        };
        let authority = <LocalProductStore as crate::provider::managed_deepseek::ManagedAuthoritySource>::current_authority(
            &store,
            &delegated_binding,
        )
        .unwrap();
        assert_eq!(authority.lease_status, "current");
        assert_eq!(authority.spend_status, "consumed");
        let contract = authority.execution_contract.clone().unwrap();
        let mut request = crate::provider::managed_deepseek::ManagedProviderCallRequest::for_role(
            crate::provider::managed_deepseek::ManagedModelRole::Planner,
            contract.protocol,
            delegated_binding.clone(),
        );
        request.limits = contract.limits;
        request.price_profile = contract.price_profile;
        request.max_output_tokens = request.limits.max_output_tokens;
        request.messages = vec![crate::provider::managed_deepseek::ManagedMessage::text(
            "user",
            "DO_NOT_PERSIST_PROVIDER_PROMPT_CONTENT",
        )];
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "INSERT INTO workflow_runs (
                            run_sequence, run_id, plan_id, created_at, updated_at, status,
                            workflow_id, dispatch_id, started_at, completed_at, result_json,
                            boundaries_json, run_json, priority, tenant_id
                         ) VALUES (
                            1, 'run-provider-journal-1', NULL, ?1, ?1, 'running',
                            ?2, NULL, ?1, NULL, NULL, '{}', '{}', 5, 'tenant-a'
                         )",
                        params!["2026-07-25T12:00:00Z", delegated_binding.workflow_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "INSERT INTO workflow_run_nodes (
                            run_id, node_id, task_type, status, node_json, started_at,
                            attempt_count, leased_at
                         ) VALUES (
                            'run-provider-journal-1', ?1, 'managed_deepseek', 'running',
                            '{}', ?2, 1, ?2
                         )",
                        params![delegated_binding.node_id, "2026-07-25T12:00:00Z"],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        store.claim_delegated_provider_request(&request).unwrap();
        // The durable claim records the request's execution-path transport
        // provenance (for_role defaults to the fail-closed injected value).
        let claimed_journal: String = store
            .with_conn(|connection| {
                connection
                    .query_row(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation.delegation_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert!(claimed_journal.contains("\"transport_provenance\":\"injected\""));
        assert!(claimed_journal.contains("\"status\":\"sending\""));
        assert!(store
            .complete_delegated_attempt(
                &delegation.delegation_id,
                "attempt-golden-path-1",
                lease["attempt_lease_token"].as_str().unwrap(),
                "in_flight",
                &json!({"provider_requests": 0}),
                0.0,
            )
            .is_err());
        let restarted =
            LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".into()).unwrap();
        let replay_error = restarted
            .claim_delegated_provider_request(&request)
            .unwrap_err();
        assert!(replay_error.contains("replay is forbidden"));
        restarted
            .reconcile_delegated_provider_request(
                &request,
                None,
                crate::provider::managed_deepseek::ManagedFailureEffect::OutcomeUnknown,
            )
            .unwrap();
        assert!(restarted
            .claim_delegated_provider_request(&request)
            .unwrap_err()
            .contains("replay is forbidden"));
        let journal: String = restarted
            .with_conn(|connection| {
                connection
                    .query_row(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation.delegation_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert!(!journal.contains("DO_NOT_PERSIST_PROVIDER_PROMPT_CONTENT"));
        assert!(journal.contains("\"status\":\"outcome_unknown\""));
        // Provenance survives restart and reconcile unchanged; an injected
        // claim can never be upgraded to external.
        assert!(journal.contains("\"transport_provenance\":\"injected\""));
        assert!(!journal.contains("\"transport_provenance\":\"external\""));
        let replay = restarted
            .admit_delegated_attempt(
                &delegated_test_principal("delegated-activator"),
                &delegation.delegation_id,
                "attempt-golden-path-1",
                &manifest,
            )
            .unwrap();
        assert_eq!(replay["replayed"], true);
        assert!(restarted
            .complete_delegated_attempt(
                &delegation.delegation_id,
                "attempt-golden-path-1",
                lease["attempt_lease_token"].as_str().unwrap(),
                "failed",
                &json!({"provider_requests": 1, "raw_output": "[redacted]"}),
                0.51,
            )
            .is_err());
        let terminal = restarted
            .complete_delegated_attempt(
                &delegation.delegation_id,
                "attempt-golden-path-1",
                lease["attempt_lease_token"].as_str().unwrap(),
                "outcome_unknown",
                &json!({"provider_requests": 1, "raw_output": "[redacted]"}),
                0.10,
            )
            .unwrap();
        assert_eq!(terminal["status"], "closed");
        assert_eq!(terminal["spend_authorization_state"], "expired");
        assert_eq!(terminal["attempt_lease_state"], "closed");
        assert_eq!(terminal["delegation_state"], "expired");
        assert!(restarted
            .complete_delegated_attempt(
                &delegation.delegation_id,
                "attempt-golden-path-1",
                lease["attempt_lease_token"].as_str().unwrap(),
                "failed",
                &json!({"provider_requests": 1, "raw_output": "[redacted]"}),
                0.10,
            )
            .is_err());
        let retry = restarted.issue_delegated_spend(
            &principal,
            &delegation.delegation_id,
            approval["approval_receipt_sha256"].as_str().unwrap(),
            &manifest,
        );
        assert!(retry.is_err(), "outcome_unknown must not permit a retry");
    }

    #[test]
    fn provider_journal_claim_records_injected_provenance_and_reconcile_preserves_it() {
        use crate::provider::managed_deepseek::{
            DeepSeekProtocol, ManagedCallBinding, ManagedCallLimits, ManagedModelRole,
            ManagedProviderCallRequest,
        };
        use crate::provider::transport::ProviderTransportProvenance;
        let binding = ManagedCallBinding {
            product_task_id: "pt-journal".into(),
            workflow_id: "wf-journal".into(),
            node_id: "planning".into(),
            attempt_id: "attempt-journal".into(),
            spend_authorization_id: "spend-journal".into(),
            attempt_lease_id: "lease-journal".into(),
        };
        let mut request = ManagedProviderCallRequest::for_role(
            ManagedModelRole::Planner,
            DeepSeekProtocol::OpenAiCompatible,
            binding,
        );
        request.limits = ManagedCallLimits {
            max_requests: 3,
            max_retries: 0,
            max_input_tokens: 8_000,
            max_output_tokens: 4_000,
            max_cumulative_tokens: 24_000,
            timeout_ms: 30_000,
            max_cost_usd: None,
        };
        // for_role defaults to the fail-closed injected provenance; only the
        // executor stamping from the actual transport can set external.
        assert_eq!(request.transport_provenance.as_str(), "injected");
        let scheduler = ManagedSchedulerLeaseAuthority {
            run_id: "run-journal".into(),
            attempt_count: 1,
            lease_owner_token_sha256: "lease-token-sha".into(),
            allowed_paths: json!([]),
            workspace_path: "/redacted/workspace".into(),
        };
        let claimed =
            claim_provider_journal_entry("[]", &request, &scheduler, "2026-07-25T12:00:00Z")
                .unwrap();
        let parsed: Value = serde_json::from_str(&claimed).unwrap();
        assert_eq!(
            parsed[0]["schema_version"],
            "managed_provider_request_claim.v1"
        );
        assert_eq!(parsed[0]["transport_provenance"], "injected");
        assert_eq!(parsed[0]["status"], "sending");
        // Reconcile preserves provenance and never upgrades it.
        let reconciled = reconcile_provider_journal_entry(
            &claimed,
            &request,
            None,
            crate::provider::managed_deepseek::ManagedFailureEffect::PreSend,
            "2026-07-25T12:00:00Z",
        )
        .unwrap();
        let parsed2: Value = serde_json::from_str(&reconciled).unwrap();
        assert_eq!(parsed2[0]["transport_provenance"], "injected");
        assert_eq!(parsed2[0]["status"], "failed_before_send");
        // A request that forges external provenance cannot reconcile or claim
        // the same node: the claim hash and node identity bind the provenance.
        let mut forged = request.clone();
        forged.transport_provenance = ProviderTransportProvenance::External;
        assert!(reconcile_provider_journal_entry(
            &reconciled,
            &forged,
            None,
            crate::provider::managed_deepseek::ManagedFailureEffect::PreSend,
            "2026-07-25T12:00:00Z",
        )
        .is_err());
        assert!(claim_provider_journal_entry(
            &reconciled,
            &forged,
            &scheduler,
            "2026-07-25T12:00:00Z"
        )
        .is_err());
        // An external-provenance claim on a fresh node records external.
        let mut external = request.clone();
        external.binding.node_id = "review".into();
        external.transport_provenance = ProviderTransportProvenance::External;
        let external_claimed =
            claim_provider_journal_entry("[]", &external, &scheduler, "2026-07-25T12:00:00Z")
                .unwrap();
        let parsed3: Value = serde_json::from_str(&external_claimed).unwrap();
        assert_eq!(parsed3[0]["transport_provenance"], "external");
    }

    #[test]
    fn delegated_authority_rechecks_revocation_before_provider_or_workspace_effect() {
        let (_dir, store) = store();
        let now = "2026-07-25T12:00:00Z";
        let delegation = delegated_contract();
        let principal = AuthenticatedPrincipal {
            tenant_id: "tenant-a".into(),
            principal_id: "delegated-operator".into(),
            principal_kind: PrincipalKind::OperatorApiKey,
            scopes: ALL_MANAGED_ACCEPTANCE_SCOPES
                .iter()
                .map(|scope| (*scope).to_string())
                .collect(),
            user_id: "operator".into(),
            role: "owner".into(),
        };
        // The test store clock is fixed before the delegation expiry and this
        // direct authenticated principal is deliberately non-fixture.
        assert!(delegation.validate(now).is_ok());
        let target_repo = _dir.path().join("revocation-target");
        let workspace_path = _dir.path().join("revocation-workspace");
        std::fs::create_dir_all(&target_repo).unwrap();
        std::fs::create_dir_all(&workspace_path).unwrap();
        let workspace = store
            .record_supervised_patch_workspace(
                &json!({
                    "run_id": "run-revocation-recheck",
                    "target_id": "alters-lab-docs",
                    "target_repo_path": target_repo,
                    "workspace_path": workspace_path,
                    "source_revision": "source-revocation-recheck",
                    "workspace_mode": "copy",
                    "status": "workspace_created"
                }),
                "test",
            )
            .unwrap();
        let workspace_id = workspace["workspace_id"].as_str().unwrap();
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "INSERT INTO product_tasks (
                            task_id, schema_version, tenant_id, workspace_id, idempotency_key,
                            status, version, objective_fingerprint, target_id, target_repo_path,
                            source_revision, source_tree_hash, output_intent, risk_class,
                            approval_required, confirm_execution, confirm_output,
                            intake_contract_sha256, intake_json, workspace_binding_json,
                            plan_id, run_id, workspace_record_id, failure_code, failure_detail,
                            created_at, updated_at, created_by
                         ) VALUES (
                            ?1, 'product_task.v1', 'tenant-a', 'default',
                            'revocation-recheck', 'graph_ready', 1, ?2,
                            'alters-lab-docs', '/redacted/target', ?3, NULL, 'draft_pr', 'low',
                            1, 1, 1, ?4, '{}', ?5, NULL, NULL,
                            ?6, NULL, NULL, ?7, ?7, 'test'
                         )",
                        params![
                            "product-task-golden-path-1",
                            "a".repeat(64),
                            "b".repeat(40),
                            "c".repeat(64),
                            json!({
                                "allowed_paths": ["docs/USER_GUIDE.md"],
                                "workspace_path": "/redacted/not-used"
                            })
                            .to_string(),
                            workspace_id,
                            now
                        ],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        store
            .persist_delegation_for_product_task(
                &principal,
                "product-task-golden-path-1",
                &delegation,
            )
            .unwrap();
        let proposal = delegated_proposal();
        store
            .persist_approved_delegated_proposal(
                &delegation.delegation_id,
                &proposal,
                proposal["manifest_sha256"].as_str().unwrap(),
            )
            .unwrap();
        let manifest =
            derive_final_execution_manifest(&proposal, &delegation, &delegated_execution())
                .unwrap();
        let approval = store
            .approve_delegated_manifest(&principal, &delegation.delegation_id, &manifest)
            .unwrap();
        let spend = store
            .issue_delegated_spend(
                &principal,
                &delegation.delegation_id,
                approval["approval_receipt_sha256"].as_str().unwrap(),
                &manifest,
            )
            .unwrap();
        let activator = delegated_test_principal("delegated-activator");
        let lease = store
            .admit_delegated_attempt(
                &activator,
                &delegation.delegation_id,
                "attempt-golden-path-1",
                &manifest,
            )
            .unwrap();
        let binding = crate::provider::managed_deepseek::ManagedCallBinding {
            product_task_id: "product-task-golden-path-1".into(),
            workflow_id: "workflow-golden-path-1".into(),
            node_id: "planning".into(),
            attempt_id: "attempt-golden-path-1".into(),
            spend_authorization_id: spend["spend_authorization_id"].as_str().unwrap().into(),
            attempt_lease_id: lease["attempt_lease_id"].as_str().unwrap().into(),
        };
        store
            .revoke_delegation(&principal, &delegation.delegation_id)
            .unwrap();
        let recoverable: (String, String, String, Option<String>) = store
            .with_conn(|connection| {
                connection
                    .query_row(
                        "SELECT status, spend_status, attempt_status, terminal_receipt_json
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation.delegation_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(recoverable.0, "revoked");
        assert_eq!(recoverable.1, "revoked");
        assert_eq!(recoverable.2, "closed");
        assert!(recoverable.3.is_some());
        assert!(<LocalProductStore as crate::provider::managed_deepseek::ManagedAuthoritySource>::current_authority(&store, &binding).is_err());
    }

    #[test]
    fn delegated_authority_rejects_mutation_revocation_expiry_and_output_escape() {
        let delegation = delegated_contract();
        let mut proposal = delegated_proposal();
        proposal["max_cost_usd"] = json!(0.50);
        assert!(
            derive_final_execution_manifest(&proposal, &delegation, &delegated_execution())
                .is_err()
        );
        let mut expired = delegation.clone();
        expired.expires_at = "2026-07-25T11:59:59Z".into();
        assert!(expired.validate("2026-07-25T12:00:00Z").is_err());
        let manifest = derive_final_execution_manifest(
            &delegated_proposal(),
            &delegation,
            &delegated_execution(),
        )
        .unwrap();
        let bad_artifact = json!({
            "artifact_sha256": "a".repeat(64),
            "changed_files": ["../outside"],
            "changed_lines": 1
        });
        let verification = json!({"status": "succeeded", "verification_sha256": "b".repeat(64)});
        let review = json!({
            "schema_version": "managed_deepseek_review_receipt.v1",
            "status": "accepted",
            "material_objection_count": 0,
            "resolved_model": "deepseek-v4-pro"
        });
        let provider_execution = json!({
            "schema_version": "managed_deepseek_execution_evidence.v1",
            "provider_request_count": 3,
            "cumulative_tokens": 18,
            "realized_cost_usd": 0.01,
            "requests": [
                {"stage":"planning","role":"planner","protocol":"openai_compatible","requested_model":"deepseek-v4-pro","resolved_model":"deepseek-v4-pro","request_id":"request-1","usage":{"input_tokens":4,"output_tokens":2,"cache_read_tokens":0,"cache_creation_tokens":0,"reasoning_output_tokens":0,"fresh_input_tokens":4,"cumulative_tokens":6,"model":"deepseek-v4-pro","request_id":"request-1"},"realized_cost_usd":0.004},
                {"stage":"implementation","role":"implementer","protocol":"openai_compatible","requested_model":"deepseek-v4-flash","resolved_model":"deepseek-v4-flash","request_id":"request-2","usage":{"input_tokens":4,"output_tokens":2,"cache_read_tokens":0,"cache_creation_tokens":0,"reasoning_output_tokens":0,"fresh_input_tokens":4,"cumulative_tokens":6,"model":"deepseek-v4-flash","request_id":"request-2"},"realized_cost_usd":0.002},
                {"stage":"review","role":"reviewer","protocol":"openai_compatible","requested_model":"deepseek-v4-pro","resolved_model":"deepseek-v4-pro","request_id":"request-3","usage":{"input_tokens":4,"output_tokens":2,"cache_read_tokens":0,"cache_creation_tokens":0,"reasoning_output_tokens":0,"fresh_input_tokens":4,"cumulative_tokens":6,"model":"deepseek-v4-pro","request_id":"request-3"},"realized_cost_usd":0.004}
            ]
        });
        let err = confirm_delegated_artifact_output(
            &delegation,
            &manifest,
            &bad_artifact,
            &verification,
            &review,
            &provider_execution,
            "6".repeat(40).as_str(),
            0.01,
        )
        .unwrap_err();
        assert!(err.contains("path"));

        // Strict containment: the parent of the allowed file entry, pseudo
        // children of it, and prefix-collision names are never admitted.
        for escaped_path in [
            "docs",
            "docs/USER_GUIDE.md/child",
            "README.md.bak",
            "README.md/child",
            "docs/USER_GUIDE.md/../../outside",
        ] {
            let escaping = json!({
                "artifact_sha256": "a".repeat(64),
                "changed_files": [escaped_path],
                "changed_lines": 1
            });
            let err = confirm_delegated_artifact_output(
                &delegation,
                &manifest,
                &escaping,
                &verification,
                &review,
                &provider_execution,
                "6".repeat(40).as_str(),
                0.01,
            )
            .unwrap_err();
            assert!(
                err.contains("path"),
                "{escaped_path} must fail closed: {err}"
            );
        }

        let artifact = json!({
            "artifact_sha256": "a".repeat(64),
            "changed_files": ["docs/USER_GUIDE.md"],
            "changed_lines": 1
        });
        let mut failed_verification = verification.clone();
        failed_verification["status"] = json!("failed");
        assert!(confirm_delegated_artifact_output(
            &delegation,
            &manifest,
            &artifact,
            &failed_verification,
            &review,
            &provider_execution,
            "6".repeat(40).as_str(),
            0.01,
        )
        .unwrap_err()
        .contains("verification"));
        assert!(confirm_delegated_artifact_output(
            &delegation,
            &manifest,
            &artifact,
            &verification,
            &review,
            &provider_execution,
            "7".repeat(40).as_str(),
            0.01,
        )
        .unwrap_err()
        .contains("target"));
        let mut mismatched_usage = provider_execution.clone();
        mismatched_usage["requests"][1]["usage"]["model"] = json!("deepseek-v4-pro");
        assert!(confirm_delegated_artifact_output(
            &delegation,
            &manifest,
            &artifact,
            &verification,
            &review,
            &mismatched_usage,
            "6".repeat(40).as_str(),
            0.01,
        )
        .unwrap_err()
        .contains("provider request"));
        let mut mismatched_cost = provider_execution.clone();
        mismatched_cost["requests"][0]["realized_cost_usd"] = json!(0.005);
        assert!(confirm_delegated_artifact_output(
            &delegation,
            &manifest,
            &artifact,
            &verification,
            &review,
            &mismatched_cost,
            "6".repeat(40).as_str(),
            0.01,
        )
        .unwrap_err()
        .contains("provider request"));
    }

    #[test]
    fn managed_workspace_action_parser_allows_only_one_json_fence() {
        let action = r#"{"schema_version":"managed_workspace_action.v1","action":"replace_text"}"#;
        let fenced = format!("```json\n{action}\n```");
        assert_eq!(
            LocalProductStore::parse_managed_workspace_action(&fenced).unwrap()["schema_version"],
            "managed_workspace_action.v1"
        );
        assert!(
            LocalProductStore::parse_managed_workspace_action("before\n```json\n{}\n```").is_err()
        );
        assert!(
            LocalProductStore::parse_managed_workspace_action("```json\n{}\n```\nafter").is_err()
        );
    }

    #[test]
    fn managed_workspace_action_requires_store_owned_current_authority() {
        let (_dir, store) = store();
        let workspace = tempdir().unwrap();
        let docs = workspace.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        let target = docs.join("USER_GUIDE.md");
        let long_prefix = (0..120)
            .map(|line| format!("guide line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(
            &target,
            format!("# Guide\n{long_prefix}\nalters-lab doctor checks health.\n"),
        )
        .unwrap();
        let binding = crate::provider::managed_deepseek::ManagedCallBinding {
            product_task_id: "product-task-golden-path-1".into(),
            workflow_id: "workflow-golden-path-1".into(),
            node_id: "implementation".into(),
            attempt_id: "attempt-golden-path-1".into(),
            spend_authorization_id: "spend-golden-path-1".into(),
            attempt_lease_id: "lease-golden-path-1".into(),
        };
        let metadata = json!({
            "workspace_path": workspace.path(),
            "workspace_root": workspace.path(),
            "allowed_paths": ["docs/USER_GUIDE.md"]
        });
        let action = json!({
            "schema_version": "managed_workspace_action.v1",
            "action": "replace_text",
            "path": "docs/USER_GUIDE.md",
            "old_text": "alters-lab doctor checks health.",
            "new_text": "alters-lab doctor performs a read-only health check."
        });
        assert!(store
            .apply_managed_workspace_action(&binding, &metadata, &action.to_string())
            .unwrap_err()
            .contains("current delegated authority"));
        assert!(std::fs::read_to_string(&target)
            .unwrap()
            .contains("checks health"));

        let escape = json!({
            "schema_version": "managed_workspace_action.v1",
            "action": "write_file",
            "path": "../outside.md",
            "content": "unsafe"
        });
        assert!(store
            .apply_managed_workspace_action(&binding, &metadata, &escape.to_string())
            .is_err());
        assert!(store
            .apply_managed_workspace_action(&binding, &metadata, "{malformed")
            .is_err());
        assert!(<LocalProductStore as crate::provider::managed_deepseek::ManagedAuthoritySource>::apply_workspace_action(
            &store,
            &binding,
            &metadata,
            &action.to_string(),
        )
        .is_err());
    }

    #[test]
    fn delegated_product_task_prepare_approve_lease_and_activate_is_provider_free() {
        struct DelegatedGoldenPathEnvGuard {
            values: Vec<(&'static str, Option<std::ffi::OsString>)>,
            _lock: std::sync::MutexGuard<'static, ()>,
        }

        impl DelegatedGoldenPathEnvGuard {
            fn enable() -> Self {
                let lock = crate::cli::config::cli_env_test_lock()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let names = [
                    PRODUCT_TASK_GATE,
                    "ACP_ENABLE_TARGET_REPO_OUTPUT",
                    "ACP_TARGET_REPO_OUTPUT_KILL_SWITCH",
                    "ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST",
                ];
                let values = names
                    .into_iter()
                    .map(|name| (name, std::env::var_os(name)))
                    .collect();
                std::env::set_var(PRODUCT_TASK_GATE, "1");
                std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
                std::env::set_var("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH", "0");
                std::env::set_var("ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST", "github.com");
                Self {
                    values,
                    _lock: lock,
                }
            }
        }

        impl Drop for DelegatedGoldenPathEnvGuard {
            fn drop(&mut self) {
                for (name, value) in self.values.drain(..) {
                    if let Some(value) = value {
                        std::env::set_var(name, value);
                    } else {
                        std::env::remove_var(name);
                    }
                }
            }
        }

        let _env = DelegatedGoldenPathEnvGuard::enable();

        let (dir, store) = store();
        let store = Arc::new(store);
        let repo = dir.path().join("target");
        std::fs::create_dir_all(repo.join("docs")).unwrap();
        std::fs::write(
            repo.join("docs/USER_GUIDE.md"),
            "# User guide\n\n`alters-lab doctor` checks health.\n",
        )
        .unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "delegated@example.invalid"],
            vec!["config", "user.name", "Delegated Test"],
            vec!["add", "docs/USER_GUIDE.md"],
            vec!["commit", "-m", "initial"],
            vec![
                "remote",
                "add",
                "origin",
                "https://github.com/Igzela/alters-lab.git",
            ],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()
                .unwrap()
                .success());
        }
        let revision = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&repo)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        let intake = ProductTaskIntakeRequest {
            objective: "Clarify that alters-lab doctor performs a read-only health check.".into(),
            target_id: "alters-lab-docs".into(),
            target_repo_path: repo.to_string_lossy().into_owned(),
            source_kind: None,
            source_revision: revision.clone(),
            source_tree_hash: None,
            allowed_paths: vec!["docs/USER_GUIDE.md".into()],
            verification_commands: vec![ProductVerificationCommand {
                command: "grep -E read-only[[:space:]]health[[:space:]]check docs/USER_GUIDE.md"
                    .into(),
                timeout_ms: 5_000,
            }],
            output_intent: "draft_pr".into(),
            executor_policy: ProductExecutorPolicy {
                allowed_executors: vec!["managed_deepseek".into()],
                prefer: Some("managed_deepseek".into()),
            },
            budget: None,
            risk_class: "low".into(),
            approval_required: true,
            confirm_execution: Some(true),
            confirm_output: Some(true),
            idempotency_key: "delegated-product-route-1".into(),
            expected_version: None,
            tenant_id: Some("tenant-a".into()),
            workspace_id: Some("default".into()),
            workspace_mode: Some("git_worktree".into()),
            matrix_binding: None,
        };
        let mut foreign_intake = intake.clone();
        foreign_intake.idempotency_key = "delegated-product-route-foreign-tenant".into();
        foreign_intake.tenant_id = Some("foreign".into());
        let foreign_validated = validate_intake(&foreign_intake, "foreign", "default").unwrap();
        let foreign_task = store
            .admit_product_task(&foreign_validated, "executor")
            .unwrap();
        let validated = validate_intake(&intake, "tenant-a", "default").unwrap();
        let task = store.admit_product_task(&validated, "executor").unwrap();
        let task_id = task["task_id"].as_str().unwrap();

        seed_key(&store, "tenant-a", "delegated-operator");
        let principal = store
            .authenticate_managed_acceptance_principal("tenant-a", "delegated-operator", Some(1.0))
            .unwrap();
        let delegation = delegated_contract();
        store.persist_delegation(&principal, &delegation).unwrap();
        store
            .bind_delegation_to_product_task(&principal, task_id, &delegation.delegation_id)
            .unwrap();
        let mut proposal = json!({
            "schema_version": "pe7_product_golden_path_live_seal_manifest.v1",
            "target": {
                "repository": "Igzela/alters-lab",
                "default_branch": "main",
                "default_branch_sha": revision,
                "allowed_mutable_paths": ["docs/USER_GUIDE.md"],
                "target_main_must_remain_unchanged": true,
                "output": "one_unmerged_acp_draft_pr_only"
            },
            "provider": {
                "protocol": "openai_compatible",
                "execution_binding": crate::rwe::campaign_package::canonical_deepseek_provider_binding().to_json()
            },
            "role_models": {
                "planner": "deepseek-v4-pro",
                "implementer": "deepseek-v4-flash",
                "reviewer": "deepseek-v4-pro"
            },
            "limits": {
                "max_cost_usd": null
            }
        });
        proposal["manifest_sha256"] = json!(compute_attempt_manifest_sha256(&proposal).unwrap());
        store
            .persist_approved_delegated_proposal(
                &delegation.delegation_id,
                &proposal,
                proposal["manifest_sha256"].as_str().unwrap(),
            )
            .unwrap();
        let runner_absence = store
            .prepare_delegated_managed_product_task(
                task_id,
                "executor",
                &[],
                &proposal,
                &delegation,
                "attempt-runner-absent",
            )
            .unwrap_err();
        assert!(runner_absence.contains("available managed_deepseek executor"));
        let foreign_error = store
            .prepare_delegated_managed_product_task(
                foreign_task["task_id"].as_str().unwrap(),
                "executor",
                &["managed_deepseek".into()],
                &proposal,
                &delegation,
                "attempt-foreign-tenant",
            )
            .unwrap_err();
        assert!(foreign_error.contains("tenant"));
        let prepared = store
            .prepare_delegated_managed_product_task(
                task_id,
                "executor",
                &["managed_deepseek".into()],
                &proposal,
                &delegation,
                "attempt-golden-path-1",
            )
            .unwrap();
        assert_eq!(prepared["scheduler_eligible"], false);
        let replayed_prepare = store
            .prepare_delegated_managed_product_task(
                task_id,
                "executor",
                &["managed_deepseek".into()],
                &proposal,
                &delegation,
                "attempt-golden-path-1",
            )
            .unwrap();
        assert_eq!(
            replayed_prepare["plan"]["plan_id"],
            prepared["plan"]["plan_id"]
        );
        assert_eq!(
            replayed_prepare["final_manifest"]["manifest_sha256"],
            prepared["final_manifest"]["manifest_sha256"]
        );
        let orphan_plan_id = prepared["plan"]["plan_id"].as_str().unwrap().to_string();
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "UPDATE product_tasks SET plan_id=NULL WHERE task_id=?1",
                        params![task_id],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        let recovered_store = LocalProductStore::new_with_clock(store.db_path(), || {
            "2026-07-25T12:00:00Z".to_string()
        })
        .unwrap();
        let recovered_prepare = recovered_store
            .prepare_delegated_managed_product_task(
                task_id,
                "recovery-owner",
                &["managed_deepseek".into()],
                &proposal,
                &delegation,
                "attempt-golden-path-1",
            )
            .unwrap();
        assert_eq!(
            recovered_prepare["plan"]["plan_id"].as_str(),
            Some(orphan_plan_id.as_str())
        );
        assert_eq!(
            recovered_prepare["final_manifest"]["manifest_sha256"],
            prepared["final_manifest"]["manifest_sha256"]
        );
        assert_eq!(
            recovered_store
                .list_workflow_plans_with_offset(10_000, 0)
                .unwrap()
                .iter()
                .filter(|plan| {
                    plan.pointer("/advisory/product_task_id")
                        .and_then(Value::as_str)
                        == Some(task_id)
                })
                .count(),
            1
        );
        let manifest = prepared["final_manifest"].clone();
        let approval = store
            .approve_delegated_manifest(&principal, &delegation.delegation_id, &manifest)
            .unwrap();
        let spend = store
            .issue_delegated_spend(
                &principal,
                &delegation.delegation_id,
                approval["approval_receipt_sha256"].as_str().unwrap(),
                &manifest,
            )
            .unwrap();
        let activator = delegated_test_principal("delegated-activator");
        let lease = store
            .admit_delegated_attempt(
                &activator,
                &delegation.delegation_id,
                "attempt-golden-path-1",
                &manifest,
            )
            .unwrap();
        let activated = store
            .activate_delegated_managed_product_task(
                task_id,
                "executor",
                &manifest,
                spend["spend_authorization_id"].as_str().unwrap(),
                lease["attempt_lease_id"].as_str().unwrap(),
            )
            .unwrap();
        assert_eq!(activated["task"]["status"], "graph_ready");
        assert_eq!(activated["scheduler_eligible"], true);
        let nodes = activated["run"]["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 4);
        assert_eq!(
            nodes[0]["managed_deepseek"]["binding"]["attempt_id"],
            "attempt-golden-path-1"
        );
        let replayed_activation = store
            .activate_delegated_managed_product_task(
                task_id,
                "executor",
                &manifest,
                spend["spend_authorization_id"].as_str().unwrap(),
                lease["attempt_lease_id"].as_str().unwrap(),
            )
            .unwrap();
        assert_eq!(replayed_activation["replayed"], true);
        assert_eq!(
            replayed_activation["run"]["run_id"],
            activated["run"]["run_id"]
        );
        assert_eq!(
            store
                .search_workflow_runs(100, 0, None)
                .unwrap()
                .iter()
                .filter(|run| run.get("plan_id") == activated["run"].get("plan_id"))
                .count(),
            1
        );

        // Fork a crash-safe local database snapshot at the exact activated
        // frontier, then prove an unknown provider effect closes all delegated
        // authority through the ordinary finalizer without a second send.
        let activated_workspace_path = activated["task"]
            .pointer("/workspace_binding/workspace_path")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let revocation_db = dir.path().join("delegated-revocation.db");
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "VACUUM INTO ?1",
                        params![revocation_db.to_string_lossy().as_ref()],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        let revocation_workspace = dir.path().join("delegated-revocation-workspace");
        std::fs::create_dir_all(revocation_workspace.join("docs")).unwrap();
        std::fs::copy(
            PathBuf::from(&activated_workspace_path).join("docs/USER_GUIDE.md"),
            revocation_workspace.join("docs/USER_GUIDE.md"),
        )
        .unwrap();
        let revocation_workspace = std::fs::canonicalize(revocation_workspace).unwrap();
        let revocation_store = LocalProductStore::new_with_clock(&revocation_db, || {
            "2026-07-25T12:00:00Z".to_string()
        })
        .unwrap();
        let revocation_workspace_id = activated["task"]["workspace_record_id"]
            .as_str()
            .unwrap()
            .to_string();
        let mut revocation_binding = revocation_store.get_product_task(task_id).unwrap().unwrap()
            ["workspace_binding"]
            .clone();
        revocation_binding["workspace_path"] = json!(revocation_workspace);
        revocation_binding["workspace_root"] = json!(revocation_workspace);
        revocation_store
            .with_conn(|connection| {
                let workspace_json: String = connection
                    .query_row(
                        "SELECT workspace_json FROM supervised_patch_workspaces
                         WHERE workspace_id=?1",
                        params![revocation_workspace_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let mut workspace_json: Value =
                    serde_json::from_str(&workspace_json).map_err(|error| error.to_string())?;
                workspace_json["workspace_mode"] = json!("copy");
                workspace_json["workspace_path"] = json!(revocation_workspace);
                connection
                    .execute(
                        "UPDATE product_tasks SET workspace_binding_json=?1 WHERE task_id=?2",
                        params![revocation_binding.to_string(), task_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "UPDATE supervised_patch_workspaces
                         SET workspace_path=?1, workspace_canonical_path=?1, workspace_json=?2
                         WHERE workspace_id=?3",
                        params![
                            revocation_workspace.to_string_lossy().as_ref(),
                            workspace_json.to_string(),
                            revocation_workspace_id
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        revocation_store
            .revoke_delegation(&principal, &delegation.delegation_id)
            .unwrap();
        let revoked_terminal: (String, String, String, String) = revocation_store
            .with_conn(|connection| {
                connection
                    .query_row(
                        "SELECT status, spend_status, attempt_status, terminal_receipt_json
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                        params![delegation.delegation_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(
            (
                &revoked_terminal.0[..],
                &revoked_terminal.1[..],
                &revoked_terminal.2[..]
            ),
            ("revoked", "revoked", "closed")
        );
        assert!(revoked_terminal
            .3
            .contains("managed_delegated_revocation_terminal_evidence.v1"));
        assert!(!revocation_workspace.exists());
        let failure_db = dir.path().join("delegated-outcome-unknown.db");
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "VACUUM INTO ?1",
                        params![failure_db.to_string_lossy().as_ref()],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        let failure_workspace = dir.path().join("delegated-outcome-unknown-workspace");
        std::fs::create_dir_all(failure_workspace.join("docs")).unwrap();
        std::fs::copy(
            PathBuf::from(&activated_workspace_path).join("docs/USER_GUIDE.md"),
            failure_workspace.join("docs/USER_GUIDE.md"),
        )
        .unwrap();
        let failure_workspace = std::fs::canonicalize(failure_workspace).unwrap();
        let failure_store = Arc::new(
            LocalProductStore::new_with_clock(&failure_db, || "2026-07-25T12:00:00Z".to_string())
                .unwrap(),
        );
        let mut failure_binding =
            failure_store.get_product_task(task_id).unwrap().unwrap()["workspace_binding"].clone();
        failure_binding["workspace_path"] = json!(failure_workspace);
        failure_binding["workspace_root"] = json!(failure_workspace);
        let workspace_record_id = activated["task"]["workspace_record_id"]
            .as_str()
            .unwrap()
            .to_string();
        failure_store
            .with_conn(|connection| {
                let workspace_json: String = connection
                    .query_row(
                        "SELECT workspace_json FROM supervised_patch_workspaces
                         WHERE workspace_id=?1",
                        params![workspace_record_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let mut workspace_json: Value =
                    serde_json::from_str(&workspace_json).map_err(|error| error.to_string())?;
                workspace_json["workspace_mode"] = json!("copy");
                workspace_json["workspace_path"] = json!(failure_workspace);
                connection
                    .execute(
                        "UPDATE product_tasks SET workspace_binding_json=?1
                         WHERE task_id=?2",
                        params![failure_binding.to_string(), task_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "UPDATE supervised_patch_workspaces
                         SET workspace_path=?1, workspace_canonical_path=?1,
                             workspace_json=?2
                         WHERE workspace_id=?3",
                        params![
                            failure_workspace.to_string_lossy().as_ref(),
                            workspace_json.to_string(),
                            workspace_record_id
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        let unknown_transport = Arc::new(DelegatedOutcomeUnknownTransport {
            sends: AtomicUsize::new(0),
        });
        let unknown_transport_trait: Arc<dyn HttpTransport> = unknown_transport.clone();
        let unknown_source: Arc<dyn crate::provider::managed_deepseek::ManagedAuthoritySource> =
            failure_store.clone();
        let unknown_executor = ManagedDeepSeekNodeExecutor::new(
            delegated_mock_provider("deepseek-v4-pro", unknown_transport_trait.clone()),
            delegated_mock_provider("deepseek-v4-flash", unknown_transport_trait.clone()),
            delegated_mock_provider("deepseek-v4-pro", unknown_transport_trait),
            unknown_source,
            ManagedDeepSeekExecutorConfig {
                protocol: DeepSeekProtocol::OpenAiCompatible,
                limits: ManagedCallLimits {
                    max_requests: 3,
                    max_retries: 0,
                    max_input_tokens: 8_000,
                    max_output_tokens: 4_000,
                    max_cumulative_tokens: 24_000,
                    timeout_ms: 30_000,
                    max_cost_usd: Some(0.50),
                },
                price_profile: DeepSeekPriceProfile::default(),
            },
        )
        .unwrap();
        let failure_run_id = activated["run"]["run_id"].as_str().unwrap();
        let mut failure_tick = Value::Null;
        for _ in 0..12 {
            failure_tick = failure_store
                .tick_with_executor(failure_run_id, "executor", 0, &unknown_executor)
                .unwrap();
            if failure_tick["run"]["status"] == "failed" {
                break;
            }
        }
        assert_eq!(unknown_transport.sends.load(Ordering::SeqCst), 1);
        let failed_run = failure_store
            .get_workflow_run(failure_run_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            failure_tick["run"]["status"], "failed",
            "run={failed_run:#} tick={failure_tick:#}"
        );
        assert_eq!(
            failed_run["nodes"][0]["result"]["error_domain"],
            "provider_outcome_unknown"
        );
        let failure_terminal = failure_store
            .finalize_product_task_after_execution(task_id, "recovery-owner")
            .unwrap();
        assert_eq!(
            failure_terminal["delegated_terminal"]["terminal"]["terminal_class"],
            "outcome_unknown"
        );
        assert_eq!(
            failure_terminal["delegated_terminal"]["terminal"]["spend_authorization_state"],
            "expired"
        );
        assert_eq!(
            failure_terminal["delegated_terminal"]["terminal"]["attempt_lease_state"],
            "closed"
        );
        assert_eq!(
            failure_terminal["delegated_terminal"]["terminal"]["delegation_state"],
            "expired"
        );
        assert!(!failure_workspace.exists());
        let recovered_failure_store =
            LocalProductStore::new_with_clock(&failure_db, || "2026-07-25T12:00:00Z".into())
                .unwrap();
        let recovered = recovered_failure_store
            .finalize_product_task_after_execution(task_id, "recovery-owner")
            .unwrap();
        assert_eq!(recovered["phase"], "terminal_failure");
        assert!(recovered["delegated_terminal"].is_null());
        assert_eq!(unknown_transport.sends.load(Ordering::SeqCst), 1);

        let cancelled_db = dir.path().join("delegated-late-response-cancelled.db");
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "VACUUM INTO ?1",
                        params![cancelled_db.to_string_lossy().as_ref()],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        let cancelled_workspace = dir.path().join("delegated-late-response-workspace");
        std::fs::create_dir_all(cancelled_workspace.join("docs")).unwrap();
        std::fs::copy(
            PathBuf::from(&activated_workspace_path).join("docs/USER_GUIDE.md"),
            cancelled_workspace.join("docs/USER_GUIDE.md"),
        )
        .unwrap();
        let cancelled_workspace = std::fs::canonicalize(cancelled_workspace).unwrap();
        let cancelled_store = Arc::new(
            LocalProductStore::new_with_clock(&cancelled_db, || "2026-07-25T12:00:00Z".to_string())
                .unwrap(),
        );
        let mut cancelled_binding = cancelled_store.get_product_task(task_id).unwrap().unwrap()
            ["workspace_binding"]
            .clone();
        cancelled_binding["workspace_path"] = json!(cancelled_workspace);
        cancelled_binding["workspace_root"] = json!(cancelled_workspace);
        cancelled_store
            .with_conn(|connection| {
                let workspace_json: String = connection
                    .query_row(
                        "SELECT workspace_json FROM supervised_patch_workspaces
                         WHERE workspace_id=?1",
                        params![workspace_record_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let mut workspace_json: Value =
                    serde_json::from_str(&workspace_json).map_err(|error| error.to_string())?;
                workspace_json["workspace_mode"] = json!("copy");
                workspace_json["workspace_path"] = json!(cancelled_workspace);
                connection
                    .execute(
                        "UPDATE product_tasks SET workspace_binding_json=?1
                         WHERE task_id=?2",
                        params![cancelled_binding.to_string(), task_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "UPDATE supervised_patch_workspaces
                         SET workspace_path=?1, workspace_canonical_path=?1,
                             workspace_json=?2
                         WHERE workspace_id=?3",
                        params![
                            cancelled_workspace.to_string_lossy().as_ref(),
                            workspace_json.to_string(),
                            workspace_record_id
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        let cancelled_preimage =
            std::fs::read(cancelled_workspace.join("docs/USER_GUIDE.md")).unwrap();
        let implementation_entered = Arc::new(Barrier::new(2));
        let implementation_release = Arc::new(Barrier::new(2));
        let blocked_transport = Arc::new(DelegatedBlockedImplementationTransport {
            responses: Mutex::new(vec![
                delegated_openai_response(
                    "blocked-plan-1",
                    "deepseek-v4-pro",
                    json!({
                        "schema_version": "managed_deepseek_plan.v1",
                        "status": "planned",
                        "path": "docs/USER_GUIDE.md",
                        "intent": "clarify_doctor_read_only_health_check"
                    }),
                ),
                delegated_openai_response(
                    "blocked-implementation-1",
                    "deepseek-v4-flash",
                    json!({
                        "schema_version": "managed_workspace_action.v1",
                        "action": "replace_text",
                        "path": "docs/USER_GUIDE.md",
                        "old_text": "`alters-lab doctor` checks health.",
                        "new_text": "`alters-lab doctor` performs a read-only health check."
                    }),
                ),
            ]),
            sends: AtomicUsize::new(0),
            implementation_entered: Arc::clone(&implementation_entered),
            implementation_release: Arc::clone(&implementation_release),
        });
        let blocked_transport_trait: Arc<dyn HttpTransport> = blocked_transport.clone();
        let blocked_source: Arc<dyn crate::provider::managed_deepseek::ManagedAuthoritySource> =
            cancelled_store.clone();
        let blocked_executor = ManagedDeepSeekNodeExecutor::new(
            delegated_mock_provider("deepseek-v4-pro", blocked_transport_trait.clone()),
            delegated_mock_provider("deepseek-v4-flash", blocked_transport_trait.clone()),
            delegated_mock_provider("deepseek-v4-pro", blocked_transport_trait),
            blocked_source,
            ManagedDeepSeekExecutorConfig {
                protocol: DeepSeekProtocol::OpenAiCompatible,
                limits: ManagedCallLimits {
                    max_requests: 3,
                    max_retries: 0,
                    max_input_tokens: 8_000,
                    max_output_tokens: 4_000,
                    max_cumulative_tokens: 24_000,
                    timeout_ms: 30_000,
                    max_cost_usd: Some(0.50),
                },
                price_profile: DeepSeekPriceProfile::default(),
            },
        )
        .unwrap();
        let cancelled_run_id = activated["run"]["run_id"].as_str().unwrap().to_string();
        cancelled_store
            .tick_with_executor(&cancelled_run_id, "executor", 0, &blocked_executor)
            .unwrap();
        assert_eq!(blocked_transport.sends.load(Ordering::SeqCst), 1);
        let execution_store = Arc::clone(&cancelled_store);
        let execution_run_id = cancelled_run_id.clone();
        let late_response = thread::spawn(move || {
            execution_store.tick_with_executor(&execution_run_id, "executor", 0, &blocked_executor)
        });
        implementation_entered.wait();
        let cancelled = cancelled_store
            .request_workflow_run_cancel(
                &cancelled_run_id,
                "cancellation-authority",
                Some("deterministic late-response boundary"),
            )
            .unwrap();
        assert_eq!(cancelled["status"], "cancelled");
        implementation_release.wait();
        let late_result = late_response.join().unwrap().unwrap();
        assert_eq!(late_result["result"]["status"], "failed");
        assert!(late_result["result"]["error_message"]
            .as_str()
            .is_some_and(|message| message.contains("stale") || message.contains("cancelled")));
        assert_eq!(blocked_transport.sends.load(Ordering::SeqCst), 2);
        assert_eq!(
            std::fs::read(cancelled_workspace.join("docs/USER_GUIDE.md")).unwrap(),
            cancelled_preimage,
            "a provider response returned after cancellation must not mutate the workspace"
        );
        let cancelled_journal: Vec<Value> = cancelled_store
            .with_conn(|connection| {
                connection
                    .query_row(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE attempt_id=?1",
                        params!["attempt-golden-path-1"],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(|error| error.to_string())
            })
            .and_then(|encoded| serde_json::from_str(&encoded).map_err(|error| error.to_string()))
            .unwrap();
        let cancelled_implementation = cancelled_journal
            .iter()
            .find(|entry| entry.get("role").and_then(Value::as_str) == Some("implementer"))
            .unwrap();
        assert_eq!(cancelled_implementation["status"], "succeeded");
        assert_eq!(
            cancelled_store
                .get_workflow_run(&cancelled_run_id)
                .unwrap()
                .unwrap()["status"],
            "failed"
        );

        let transport = Arc::new(DelegatedRouteMockTransport {
            responses: Mutex::new(vec![
                delegated_openai_response(
                    "mock-plan-1",
                    "deepseek-v4-pro",
                    json!({
                        "schema_version": "managed_deepseek_plan.v1",
                        "status": "planned",
                        "path": "docs/USER_GUIDE.md",
                        "intent": "clarify_doctor_read_only_health_check"
                    }),
                ),
                delegated_openai_response(
                    "mock-implementation-1",
                    "deepseek-v4-flash",
                    json!({
                        "schema_version": "managed_workspace_action.v1",
                        "action": "replace_text",
                        "path": "docs/USER_GUIDE.md",
                        "old_text": "`alters-lab doctor` checks health.",
                        "new_text": "`alters-lab doctor` performs a read-only health check."
                    }),
                ),
                delegated_openai_response(
                    "mock-review-1",
                    "deepseek-v4-pro",
                    json!({
                        "schema_version": "managed_deepseek_review.v1",
                        "status": "accepted",
                        "material_objections": []
                    }),
                ),
            ]),
            sends: AtomicUsize::new(0),
        });
        let transport_trait: Arc<dyn HttpTransport> = transport.clone();
        let source: Arc<dyn crate::provider::managed_deepseek::ManagedAuthoritySource> =
            store.clone();
        let executor = ManagedDeepSeekNodeExecutor::new(
            delegated_mock_provider("deepseek-v4-pro", transport_trait.clone()),
            delegated_mock_provider("deepseek-v4-flash", transport_trait.clone()),
            delegated_mock_provider("deepseek-v4-pro", transport_trait),
            source,
            ManagedDeepSeekExecutorConfig {
                protocol: DeepSeekProtocol::OpenAiCompatible,
                limits: ManagedCallLimits {
                    max_requests: 3,
                    max_retries: 0,
                    max_input_tokens: 8_000,
                    max_output_tokens: 4_000,
                    max_cumulative_tokens: 24_000,
                    timeout_ms: 30_000,
                    max_cost_usd: Some(0.50),
                },
                price_profile: DeepSeekPriceProfile::default(),
            },
        )
        .unwrap();
        let run_id = activated["run"]["run_id"].as_str().unwrap();
        let mut tick_results = Vec::new();
        for _ in 0..8 {
            let tick = store
                .tick_with_executor(run_id, "executor", 0, &executor)
                .unwrap();
            tick_results.push(tick.clone());
            if tick
                .pointer("/run/status")
                .and_then(Value::as_str)
                .is_some_and(|status| {
                    matches!(status, "completed" | "failed" | "cancelled" | "killed")
                })
            {
                break;
            }
        }
        let completed_run = store.get_workflow_run(run_id).unwrap().unwrap();
        assert_eq!(
            completed_run["status"], "completed",
            "run={completed_run:#} ticks={tick_results:#?}"
        );
        assert_eq!(transport.sends.load(Ordering::SeqCst), 3);
        let workspace_path = activated_workspace_path.as_str();
        let guide =
            std::fs::read_to_string(PathBuf::from(workspace_path).join("docs/USER_GUIDE.md"))
                .unwrap();
        assert!(guide.contains("performs a read-only health check"));
        let finalized = store
            .finalize_product_task_after_execution(task_id, "executor")
            .unwrap();
        assert_eq!(finalized["task"]["status"], "awaiting_approval");
        let task_version = finalized["task"]["version"].as_u64().unwrap();
        let original_provider_journal = store
            .with_conn(|connection| {
                connection
                    .query_row(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE attempt_id=?1",
                        params!["attempt-golden-path-1"],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert!(store
            .approve_delegated_product_task(
                &principal,
                task_id,
                "artifact-confirmer",
                task_version,
                &delegation.delegation_id,
                &manifest,
                &revision,
            )
            .unwrap_err()
            .contains("confirmer authority"));
        assert!(store
            .approve_delegated_product_task(
                &activator,
                task_id,
                "artifact-confirmer",
                task_version,
                &delegation.delegation_id,
                &manifest,
                &revision,
            )
            .unwrap_err()
            .contains("confirmer authority"));
        let mut tampered_provider_journal: Value =
            serde_json::from_str(&original_provider_journal).unwrap();
        tampered_provider_journal[0]["request_id"] = json!("tampered-request-id");
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=?1 WHERE attempt_id=?2",
                        params![
                            tampered_provider_journal.to_string(),
                            "attempt-golden-path-1"
                        ],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert!(store
            .approve_delegated_product_task(
                &delegated_test_principal("artifact-confirmer"),
                task_id,
                "artifact-confirmer",
                task_version,
                &delegation.delegation_id,
                &manifest,
                &revision,
            )
            .unwrap_err()
            .contains("journal"));
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=?1 WHERE attempt_id=?2",
                        params![original_provider_journal, "attempt-golden-path-1"],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        let delegated_approval = store
            .approve_delegated_product_task(
                &delegated_test_principal("artifact-confirmer"),
                task_id,
                "artifact-confirmer",
                task_version,
                &delegation.delegation_id,
                &manifest,
                &revision,
            )
            .unwrap();
        assert_eq!(
            delegated_approval["artifact_confirmation"]["schema_version"],
            "managed_delegated_artifact_confirmation.v1"
        );
        assert_eq!(delegated_approval["replayed"], false);
        let replayed_approval = store
            .approve_delegated_product_task(
                &delegated_test_principal("artifact-confirmer"),
                task_id,
                "artifact-confirmer",
                task_version,
                &delegation.delegation_id,
                &manifest,
                &revision,
            )
            .unwrap();
        assert_eq!(replayed_approval["replayed"], true);
        assert_eq!(
            replayed_approval["approval"]["approval_id"],
            delegated_approval["approval"]["approval_id"]
        );
        let output = store
            .output_product_task(
                task_id,
                "output-owner",
                task_version,
                delegated_approval["approval"]["approval_id"].as_str(),
                true,
            )
            .unwrap();
        assert_eq!(output["output"]["mode"], "draft_pr");
        assert_eq!(output["output"]["status"], "planned", "output={output:#}");
        assert_eq!(output["output"]["network_effect"], false);
        assert_eq!(output["output"]["target_repository"], "Igzela/alters-lab");
        assert!(output["output"]["head_branch"]
            .as_str()
            .is_some_and(|branch| branch.starts_with("acp/")));
        assert_eq!(
            output["output"]["operation"]["request"]["source_revision"],
            revision
        );
        assert_eq!(output["task"]["status"], "output_pending");
        assert_eq!(transport.sends.load(Ordering::SeqCst), 3);
        assert!(store
            .complete_delegated_product_task_terminal(
                &delegation.delegation_id,
                "attempt-golden-path-1",
                task_id,
                "cleanup-owner",
            )
            .is_err());
        assert!(PathBuf::from(workspace_path).exists());
        let target_main_after = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "main"])
                .current_dir(&repo)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        assert_eq!(target_main_after, revision);
        let workspace_record_id = finalized["task"]["workspace_record_id"].as_str().unwrap();
        let cleaned = store
            .cleanup_workspace(workspace_record_id, "cleanup-owner")
            .unwrap();
        assert_eq!(cleaned["status"], "cleaned");
        assert!(!PathBuf::from(workspace_path).exists());
        let terminal_receipt = json!({
            "schema_version": "managed_delegated_terminal_evidence.v1",
            "product_task_id": task_id,
            "workflow_run_id": run_id,
            "manifest_sha256": manifest["manifest_sha256"],
            "artifact_confirmation_sha256": delegated_approval["artifact_confirmation"]["artifact_confirmation_sha256"],
            "output_mode": output["output"]["mode"],
            "output_status": output["output"]["status"],
            "provider_requests": transport.sends.load(Ordering::SeqCst),
            "cleanup_status": cleaned["status"],
            "target_main_sha": target_main_after,
        });
        let realized_cost_usd = delegated_approval["realized_cost_usd"].as_f64().unwrap();
        let terminal = store
            .complete_delegated_attempt(
                &delegation.delegation_id,
                "attempt-golden-path-1",
                lease["attempt_lease_token"].as_str().unwrap(),
                "succeeded",
                &terminal_receipt,
                realized_cost_usd,
            )
            .unwrap();
        assert_eq!(terminal["status"], "closed");
        assert_eq!(terminal["terminal_class"], "succeeded");
        assert_eq!(terminal["spend_authorization_state"], "expired");
        assert_eq!(terminal["attempt_lease_state"], "closed");
        assert_eq!(terminal["delegation_state"], "expired");
        assert_eq!(
            store
                .complete_delegated_attempt(
                    &delegation.delegation_id,
                    "attempt-golden-path-1",
                    lease["attempt_lease_token"].as_str().unwrap(),
                    "succeeded",
                    &terminal_receipt,
                    realized_cost_usd,
                )
                .unwrap()["replayed"],
            true
        );
    }

    #[test]
    fn bootstrap_rebinds_only_unadmitted_delegation_and_is_idempotent() {
        struct ProductGateGuard(Option<std::ffi::OsString>);

        impl Drop for ProductGateGuard {
            fn drop(&mut self) {
                match self.0.take() {
                    Some(value) => std::env::set_var(PRODUCT_TASK_GATE, value),
                    None => std::env::remove_var(PRODUCT_TASK_GATE),
                }
            }
        }

        let _env_lock = crate::cli::config::cli_env_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let prior_gate = std::env::var_os(PRODUCT_TASK_GATE);
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        let _gate = ProductGateGuard(prior_gate);

        let (dir, store) = store();
        let target = dir.path().join("target");
        std::fs::create_dir_all(target.join("docs")).unwrap();
        std::fs::write(target.join("docs/USER_GUIDE.md"), "guide\n").unwrap();
        let intake = ProductTaskIntakeRequest {
            objective: "Create a bounded local artifact".into(),
            target_id: "bootstrap-recovery".into(),
            target_repo_path: target.to_string_lossy().into_owned(),
            source_kind: Some("local_folder".into()),
            source_revision: "unused-local-folder-revision".into(),
            source_tree_hash: None,
            allowed_paths: vec!["docs/USER_GUIDE.md".into()],
            verification_commands: vec![ProductVerificationCommand {
                command: "test -f docs/USER_GUIDE.md".into(),
                timeout_ms: 5_000,
            }],
            output_intent: "artifact_only".into(),
            executor_policy: ProductExecutorPolicy {
                allowed_executors: vec!["deterministic".into()],
                prefer: Some("deterministic".into()),
            },
            budget: None,
            risk_class: "low".into(),
            approval_required: true,
            confirm_execution: Some(true),
            confirm_output: Some(true),
            idempotency_key: "bootstrap-recovery-rebind".into(),
            expected_version: None,
            tenant_id: Some("local".into()),
            workspace_id: Some("default".into()),
            workspace_mode: Some("local_folder".into()),
            matrix_binding: None,
        };
        let validated = validate_intake(&intake, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "operator").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        let task_version_before = task["version"].as_i64().unwrap();

        seed_key(&store, "local", "bootstrap-reconcile-operator");
        let principal = store
            .authenticate_managed_acceptance_principal(
                "local",
                "bootstrap-reconcile-operator",
                Some(1.0),
            )
            .unwrap();
        let bootstrap_scopes = vec![SCOPE_IDENTITY_DELEGATE.to_string()];
        let over_scoped_bootstrap = vec![
            SCOPE_IDENTITY_DELEGATE.to_string(),
            SCOPE_RISK_ACKNOWLEDGE.to_string(),
        ];
        store
            .record_api_key_metadata_for_tenant(
                "local",
                LOCAL_BOOTSTRAP_API_KEY_ID,
                "local-admin",
                "admin",
                &over_scoped_bootstrap,
                "test-bootstrap",
            )
            .unwrap();
        assert!(store
            .authenticate_bootstrap_identity_delegation_principal("local", Some(1.0))
            .is_err());
        store
            .record_api_key_metadata_for_tenant(
                "local",
                LOCAL_BOOTSTRAP_API_KEY_ID,
                "local-admin",
                "admin",
                &bootstrap_scopes,
                "test-bootstrap",
            )
            .unwrap();
        let bootstrap = store
            .authenticate_bootstrap_identity_delegation_principal("local", Some(1.0))
            .unwrap();
        let mut delegation = delegated_contract();
        delegation.delegation_id = "bootstrap-rebind-delegation".into();
        store
            .persist_delegation_for_product_task(&principal, task_id, &delegation)
            .unwrap();
        let reviewer_scopes = vec![
            SCOPE_RISK_ACKNOWLEDGE.to_string(),
            SCOPE_DELEGATED_AUTONOMY.to_string(),
            SCOPE_DELEGATED_MANIFEST_APPROVE.to_string(),
            SCOPE_SPEND_AUTHORIZE.to_string(),
        ];
        store
            .record_api_key_metadata_for_tenant(
                "local",
                "bootstrap-reconcile-reissued",
                "reviewer-user",
                "reviewer",
                &reviewer_scopes,
                "test-bootstrap",
            )
            .unwrap();
        let reissued_reviewer = store
            .authenticate_managed_acceptance_principal_for_tenant(
                "local",
                "bootstrap-reconcile-reissued",
                Some(1.0),
            )
            .unwrap();

        let rebound = store
            .rebind_unadmitted_delegation_for_bootstrap(
                &bootstrap,
                task_id,
                &delegation.delegation_id,
                &reissued_reviewer,
            )
            .unwrap();
        assert_eq!(rebound["status"], "active");
        assert_eq!(rebound["delegation_state"], "active");
        assert_eq!(rebound["operation_identity"], delegation.delegation_id);
        assert_eq!(rebound["product_task_id"], task_id);
        assert_eq!(
            rebound["reviewer_principal_id"],
            "bootstrap-reconcile-reissued"
        );
        let original_principal: String = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT principal_id FROM managed_acceptance_delegations WHERE delegation_id=?1",
                    params![delegation.delegation_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(original_principal, "bootstrap-reconcile-operator");
        assert_eq!(rebound["replayed"], false);
        assert_eq!(
            store
                .delegated_authority_state(&delegation.delegation_id)
                .unwrap()["delegation_state"],
            "active"
        );
        assert_eq!(
            store.get_product_task(task_id).unwrap().unwrap()["version"],
            task_version_before
        );

        let audit_count_before_replay = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action='managed_acceptance.delegation_bootstrap_rebound' AND resource=?1",
                    params![delegation.delegation_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();

        let replay = store
            .rebind_unadmitted_delegation_for_bootstrap(
                &bootstrap,
                task_id,
                &delegation.delegation_id,
                &reissued_reviewer,
            )
            .unwrap();
        assert_eq!(replay["replayed"], true);
        assert_eq!(
            store.get_product_task(task_id).unwrap().unwrap()["version"],
            task_version_before
        );
        let audit_count_after_replay = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action='managed_acceptance.delegation_bootstrap_rebound' AND resource=?1",
                    params![delegation.delegation_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(audit_count_after_replay, audit_count_before_replay);

        store
            .record_api_key_metadata_for_tenant(
                "local",
                "bootstrap-reconcile-second-reviewer",
                "second-reviewer-user",
                "reviewer",
                &reviewer_scopes,
                "test-bootstrap",
            )
            .unwrap();
        let second_reviewer = store
            .authenticate_managed_acceptance_principal_for_tenant(
                "local",
                "bootstrap-reconcile-second-reviewer",
                Some(1.0),
            )
            .unwrap();
        assert!(store
            .rebind_unadmitted_delegation_for_bootstrap(
                &bootstrap,
                task_id,
                &delegation.delegation_id,
                &second_reviewer,
            )
            .is_err());
        let mut foreign_intake = intake;
        foreign_intake.idempotency_key = "bootstrap-rebind-foreign-task".into();
        let foreign_task = store
            .admit_product_task(
                &validate_intake(&foreign_intake, "local", "default").unwrap(),
                "operator",
            )
            .unwrap();
        let foreign_task_id = foreign_task["task_id"].as_str().unwrap();
        assert_ne!(foreign_task_id, task_id);
        assert!(store
            .rebind_unadmitted_delegation_for_bootstrap(
                &bootstrap,
                foreign_task_id,
                &delegation.delegation_id,
                &reissued_reviewer,
            )
            .is_err());
        assert!(store
            .authenticate_bootstrap_identity_delegation_principal("foreign-tenant", Some(1.0))
            .is_err());

        store
            .record_api_key_metadata_with_expiry_for_tenant(
                "foreign",
                "bootstrap-reconcile-foreign-reviewer",
                "foreign-reviewer-user",
                "reviewer",
                &reviewer_scopes,
                None,
                "test-bootstrap",
            )
            .unwrap();
        let foreign_reviewer = store
            .authenticate_managed_acceptance_principal(
                "foreign",
                "bootstrap-reconcile-foreign-reviewer",
                Some(1.0),
            )
            .unwrap();
        assert!(store
            .rebind_unadmitted_delegation_for_bootstrap(
                &bootstrap,
                task_id,
                &delegation.delegation_id,
                &foreign_reviewer,
            )
            .is_err());

        store
            .record_api_key_metadata_for_tenant(
                "local",
                "bootstrap-reconcile-operator-profile",
                "operator-profile-user",
                "operator",
                &ALL_MANAGED_ACCEPTANCE_SCOPES
                    .iter()
                    .map(|scope| (*scope).to_string())
                    .collect::<Vec<_>>(),
                "test-bootstrap",
            )
            .unwrap();
        let operator_profile = store
            .authenticate_managed_acceptance_principal_for_tenant(
                "local",
                "bootstrap-reconcile-operator-profile",
                Some(1.0),
            )
            .unwrap();
        assert!(store
            .rebind_unadmitted_delegation_for_bootstrap(
                &bootstrap,
                task_id,
                &delegation.delegation_id,
                &operator_profile,
            )
            .is_err());

        let mut unbound = delegated_contract();
        unbound.delegation_id = "bootstrap-rebind-unbound".into();
        store.persist_delegation(&principal, &unbound).unwrap();
        assert!(store
            .rebind_unadmitted_delegation_for_bootstrap(
                &bootstrap,
                task_id,
                &unbound.delegation_id,
                &principal,
            )
            .is_err());

        let mut revoked = delegated_contract();
        revoked.delegation_id = "bootstrap-rebind-revoked".into();
        store
            .persist_delegation_for_product_task(&principal, task_id, &revoked)
            .unwrap();
        store
            .revoke_delegation(&principal, &revoked.delegation_id)
            .unwrap();
        assert!(store
            .rebind_unadmitted_delegation_for_bootstrap(
                &bootstrap,
                task_id,
                &revoked.delegation_id,
                &principal,
            )
            .is_err());
    }
}
