#![allow(unused_imports)]

use super::require_finite_expiry;

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

use super::super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use crate::infrastructure::auth::{LOCAL_BOOTSTRAP_API_KEY_ID, LOCAL_BOOTSTRAP_TENANT_ID};
use crate::node_executor::ProcessOutcome;
use crate::provider::managed_deepseek::DEEPSEEK_PROVIDER_KIND;
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

pub(super) fn validate_bootstrap_identity_delegation_scopes(
    scopes: &[String],
) -> Result<(), String> {
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

pub(super) const MAX_EFFECT_CHILD_AUTHORIZATIONS: u64 = 64;

pub(super) const MAX_EFFECT_OUTCOME_EVIDENCE_BYTES: usize = 2048;

pub(super) const DELEGATED_MANIFEST_APPROVAL_SCHEMA_VERSION: &str =
    "managed_delegated_manifest_approval.v1";

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
    pub(super) fn validate_against(
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

pub(super) fn validate_effect_envelope_body(
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
        let codex_luna = crate::rwe::campaign_package::CODEX_SUBSCRIPTION_LUNA_MODEL;
        let codex_terra = crate::rwe::campaign_package::CODEX_SUBSCRIPTION_TERRA_MODEL;
        let valid_codex_models = [codex_luna, codex_terra].iter().any(|model| {
            self.models
                == json!({
                    "planner": model,
                    "implementer": model,
                    "reviewer": model
                })
        });
        let valid_models = self.models == default_models
            || ["deepseek-v4-pro", "deepseek-v4-flash"]
                .iter()
                .any(|model| {
                    self.models
                        == json!({
                            "planner": model,
                            "implementer": model,
                            "reviewer": model
                        })
                });
        let valid_models = valid_models || valid_codex_models;
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
    let requested_model = delegation
        .models
        .get("implementer")
        .and_then(Value::as_str)
        .ok_or("delegation implementer model binding is required")?;
    let expected_binding = proposal
        .get("campaign_package_id")
        .and_then(Value::as_str)
        .filter(|package_id| !package_id.trim().is_empty())
        .map(|package_id| {
            let package =
                crate::rwe::campaign_package::resolve_frozen_campaign_package(package_id)?;
            package.validate()?;
            package.provider_execution_binding_for_model(requested_model)
        })
        .transpose()?
        .unwrap_or_else(crate::rwe::campaign_package::canonical_deepseek_provider_binding);
    let binding_value = proposal
        .get("provider_execution_binding")
        .or_else(|| proposal.pointer("/provider/execution_binding"));
    let binding = match binding_value {
        Some(v) => crate::rwe::campaign_package::FrozenProviderExecutionBinding::from_json(v)?,
        None => expected_binding.clone(),
    };
    if binding != expected_binding {
        return Err(
            "immutable proposal provider execution binding does not exactly match the frozen package"
                .into(),
        );
    }
    if binding.admitted_model != requested_model {
        return Err("immutable proposal model does not exactly match its provider binding".into());
    }
    let provider = managed_provider_value_for_binding(&binding);
    let is_deepseek =
        binding.provider_kind == crate::provider::managed_deepseek::DEEPSEEK_PROVIDER_KIND;
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
        "campaign_package_id": proposal.get("campaign_package_id"),
        "matrix_model_descriptor_id": proposal.get("matrix_model_descriptor_id"),
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
            "max_output_tokens": if !is_deepseek
                && execution.get("verifier").and_then(Value::as_str)
                    == Some(crate::rwe::frozen_rwe_bindings::FROZEN_RWE_VERIFIER_IDENTITY)
            {
                8_192
            } else {
                4_000
            },
            "max_cumulative_tokens": if execution.get("verifier").and_then(Value::as_str)
                == Some(crate::rwe::frozen_rwe_bindings::FROZEN_RWE_VERIFIER_IDENTITY)
            {
                if is_deepseek { 16_000 } else { 20_192 }
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

pub(super) fn managed_provider_value_for_binding(
    binding: &crate::rwe::campaign_package::FrozenProviderExecutionBinding,
) -> Value {
    let mut provider = binding.to_json();
    provider["kind"] = json!(binding.provider_kind);
    provider["price_profile"] = if binding.provider_kind
        == crate::provider::managed_deepseek::DEEPSEEK_PROVIDER_KIND
        && !binding.cost_unavailable
    {
        serde_json::to_value(crate::provider::managed_deepseek::DeepSeekPriceProfile::default())
            .expect("DeepSeek price profile serializes")
    } else {
        Value::Null
    };
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
    let provider_binding = manifest
        .get("provider_execution_binding")
        .map(crate::rwe::campaign_package::FrozenProviderExecutionBinding::from_json)
        .transpose()?
        .unwrap_or_else(crate::rwe::campaign_package::canonical_deepseek_provider_binding);
    let expected_models = [
        manifest.pointer("/models/planner").and_then(Value::as_str),
        manifest
            .pointer("/models/implementer")
            .and_then(Value::as_str),
        manifest.pointer("/models/reviewer").and_then(Value::as_str),
    ];
    if expected_models.iter().any(Option::is_none) {
        return Err("delegated artifact confirmation model bindings are missing".into());
    }
    let expected_models = expected_models.map(Option::unwrap);
    let expected_provider_schema = if provider_binding.cost_unavailable {
        provider_binding.provider_kind.as_str()
    } else {
        DEEPSEEK_PROVIDER_KIND
    };
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
        || review.get("resolved_model").and_then(Value::as_str) != Some(expected_models[2])
    {
        return Err("deterministic verification and bounded Pro review are required".into());
    }
    let provider_requests = provider_execution
        .get("requests")
        .and_then(Value::as_array)
        .ok_or("provider execution requests are required")?;
    let expected_route = [
        ("planning", "planner", expected_models[0]),
        ("implementation", "implementer", expected_models[1]),
        ("review", "reviewer", expected_models[2]),
    ];
    let mut request_ids = std::collections::HashSet::new();
    let mut summed_request_cost_usd = 0.0;
    let mut summed_cumulative_tokens = 0_u64;
    let max_cumulative_tokens = manifest
        .pointer("/limits/max_cumulative_tokens")
        .and_then(Value::as_u64)
        .ok_or("delegated artifact confirmation token ceiling is missing")?;
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
                let request_cost = request.get("realized_cost_usd").and_then(Value::as_f64);
                let cost_ok = if provider_binding.cost_unavailable {
                    request.get("realized_cost_usd") == Some(&Value::Null)
                } else {
                    request_cost.is_some_and(|cost| cost.is_finite() && cost >= 0.0)
                };
                if !cost_ok {
                    return false;
                }
                let Some(new_cumulative_tokens) =
                    summed_cumulative_tokens.checked_add(usage.cumulative_tokens)
                else {
                    return false;
                };
                summed_cumulative_tokens = new_cumulative_tokens;
                summed_request_cost_usd += request_cost.unwrap_or(0.0);
                request.get("stage").and_then(Value::as_str) == Some(stage)
                    && request.get("role").and_then(Value::as_str) == Some(role)
                    && request.get("protocol").and_then(Value::as_str) == Some("openai_compatible")
                    && (!provider_binding.cost_unavailable
                        || request.get("provider_kind").and_then(Value::as_str)
                            == Some(expected_provider_schema))
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
            .is_some_and(|tokens| tokens <= max_cumulative_tokens && tokens > 0)
        && if provider_binding.cost_unavailable {
            provider_execution.get("realized_cost_usd") == Some(&Value::Null)
                && realized_cost_usd == 0.0
        } else {
            provider_execution
                .get("realized_cost_usd")
                .and_then(Value::as_f64)
                .is_some_and(|cost| (cost - realized_cost_usd).abs() <= 1e-12)
        }
        && provider_execution
            .get("cumulative_tokens")
            .and_then(Value::as_u64)
            == Some(summed_cumulative_tokens)
        && (provider_binding.cost_unavailable
            || (summed_request_cost_usd - realized_cost_usd).abs() <= 1e-12)
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
        "realized_cost_usd": if provider_binding.cost_unavailable {
            Value::Null
        } else {
            json!(realized_cost_usd)
        },
        "output": {"draft_pr_only": true, "merged": false, "authorized": true}
    })))
}

pub(super) fn replay_delegated_artifact_confirmation(
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

pub(super) fn validate_delegated_manifest_policy(manifest: &Value) -> Result<&str, String> {
    let manifest_sha256 = manifest
        .get("manifest_sha256")
        .and_then(Value::as_str)
        .ok_or("final manifest hash is required")?;
    if manifest_sha256 != compute_attempt_manifest_sha256(manifest)? {
        return Err("final manifest hash mismatch".into());
    }
    let expected_output = json!({
        "draft_pr_only": true,
        "target_main_write": false,
        "merge": false,
        "auto_merge": false
    });
    let requested_model = manifest
        .pointer("/models/implementer")
        .and_then(Value::as_str)
        .ok_or("final manifest implementer model is required")?;
    let package_id = manifest
        .get("campaign_package_id")
        .and_then(Value::as_str)
        .or_else(|| {
            manifest
                .pointer("/execution/campaign_package_id")
                .and_then(Value::as_str)
        })
        .filter(|package_id| !package_id.trim().is_empty());
    let expected_binding = match package_id {
        Some(package_id) => {
            let package =
                crate::rwe::campaign_package::resolve_frozen_campaign_package(package_id)?;
            package.validate()?;
            package.provider_execution_binding_for_model(requested_model)?
        }
        None => crate::rwe::campaign_package::canonical_deepseek_provider_binding(),
    };
    let observed_binding = match manifest.get("provider_execution_binding") {
        Some(b) => crate::rwe::campaign_package::FrozenProviderExecutionBinding::from_json(b)?,
        None => expected_binding.clone(),
    };
    if observed_binding != expected_binding {
        return Err(
            "final manifest provider execution binding does not exactly match the frozen package"
                .into(),
        );
    }
    let expected_models = if package_id.is_some() {
        json!({
            "planner": expected_binding.admitted_model.clone(),
            "implementer": expected_binding.admitted_model.clone(),
            "reviewer": expected_binding.admitted_model.clone()
        })
    } else {
        json!({
            "planner": "deepseek-v4-pro",
            "implementer": "deepseek-v4-flash",
            "reviewer": "deepseek-v4-pro"
        })
    };
    let expected_provider = managed_provider_value_for_binding(&expected_binding);
    let expected_binding_json = expected_binding.to_json();
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
    let is_deepseek =
        expected_binding.provider_kind == crate::provider::managed_deepseek::DEEPSEEK_PROVIDER_KIND;
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
        && manifest
            .get("provider_execution_binding")
            .map_or(package_id.is_none(), |b| b == &expected_binding_json);
    let policy_matches = manifest
        .pointer("/target/repository")
        .and_then(Value::as_str)
        == Some("Igzela/alters-lab")
        && (docs_policy || rwe_policy)
        && manifest.get("protocol").and_then(Value::as_str)
            == Some(expected_binding.protocol.as_str())
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
                && ((is_deepseek && output == Some(4_000) && cum == Some(16_000))
                    || (!is_deepseek && output == Some(8_192) && cum == Some(20_192)))
                && timeout == Some(900_000);
            (docs_policy && docs_limits) || (rwe_policy && rwe_limits)
        }
        && manifest.get("output") == Some(&expected_output);
    if !policy_matches {
        return Err("final manifest is outside the persisted delegation policy".into());
    }
    Ok(manifest_sha256)
}

pub(super) fn delegated_execution_contract(
    manifest: &Value,
    node_id: &str,
) -> Result<crate::provider::managed_deepseek::PersistedManagedExecutionContract, String> {
    validate_delegated_manifest_policy(manifest)?;
    let provider_binding = match manifest.get("provider_execution_binding") {
        Some(b) => crate::rwe::campaign_package::FrozenProviderExecutionBinding::from_json(b)?,
        None => crate::rwe::campaign_package::canonical_deepseek_provider_binding(),
    };
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
    // The managed execution contract retains the existing typed price-profile
    // slot for authority equality.  Subscription bindings deliberately carry
    // JSON null there: no USD quote is claimed or used for accounting.
    let price_profile = match manifest.pointer("/provider/price_profile") {
        Some(Value::Null) if provider_binding.cost_unavailable => {
            crate::provider::managed_deepseek::DeepSeekPriceProfile::default()
        }
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| "delegated manifest price profile is malformed")?,
        None => return Err("delegated manifest price profile is missing".into()),
    };
    Ok(
        crate::provider::managed_deepseek::PersistedManagedExecutionContract {
            thinking: provider_binding.reasoning_effort.map(|effort| {
                crate::provider::managed_deepseek::ThinkingConfiguration {
                    mode: "enabled".into(),
                    reasoning_effort: Some(effort),
                }
            }),
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

pub(super) fn validate_manifest_approval_row(
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

pub(super) fn delegated_manifest_approval_receipt(
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

pub(super) fn validate_existing_manifest_approval(
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

pub(super) fn record_workspace_action_in_journal(
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

pub(super) fn record_workspace_action_failure_in_journal(
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
    pub(super) tenant_id: String,
    pub(super) principal_id: String,
    pub(super) principal_kind: PrincipalKind,
    pub(super) scopes: Vec<String>,
    pub(super) user_id: String,
    pub(super) role: String,
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
    pub(super) facts: ManagedCodexLaunchFacts,
    pub(super) product_task_id: String,
    pub(super) product_task_version: u64,
    pub(super) tenant_id: String,
    pub(super) principal_id: String,
    pub(super) principal_kind: PrincipalKind,
    pub(super) spend_authorization_id: String,
    pub(super) attempt_id: String,
    pub(super) execution_id: String,
    pub(super) lease_token: String,
    pub(super) spend_body: Value,
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

pub(super) fn is_forbidden_principal_id(id: &str) -> bool {
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

pub(super) fn is_forbidden_role(role: &str) -> bool {
    let lower = role.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "agent" | "bot" | "automation" | "ci" | "system" | "anonymous" | "fixture"
    )
}

pub(in crate::storage::local_product_store) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(in crate::storage::local_product_store) fn canonical_json(
    value: &Value,
) -> Result<String, String> {
    Ok(sort_value(value).to_string())
}

pub(super) fn sort_value(value: &Value) -> Value {
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

pub(super) fn required_str(v: &Value, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{key} required"))
}

/// Parse a canonical RFC3339 timestamp and normalize to UTC.
pub(super) fn parse_rfc3339_utc(
    field: &str,
    value: &str,
) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(value.trim())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|_| format!("{field} must be canonical RFC3339/UTC"))
}

/// True when `expires_at` is at or before `now` using parsed UTC instants (not lexical strings).
pub(super) fn is_at_or_before(expires_at: &str, now: &str) -> Result<bool, String> {
    let exp = parse_rfc3339_utc("expires_at", expires_at)?;
    let n = parse_rfc3339_utc("now", now)?;
    Ok(exp <= n)
}

/// Strictly after using parsed UTC instants.
pub(super) fn is_strictly_after(expires_at: &str, now: &str) -> Result<bool, String> {
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
