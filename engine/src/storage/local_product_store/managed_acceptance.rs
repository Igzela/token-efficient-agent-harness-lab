//! Store-owned managed-acceptance decision, risk acknowledgement, spend authorization,
//! and exactly-once attempt admission.
//!
//! Production principals are derived only from verified `api_key_metadata` records.
//! Free-form strings never create authority. Fixture principals are test-only and cannot
//! authorize production live starts.

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use uuid::Uuid;

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};

/// Required scopes for managed-acceptance authority operations.
pub const SCOPE_RISK_ACKNOWLEDGE: &str = "managed_acceptance:risk_acknowledge";
pub const SCOPE_SPEND_AUTHORIZE: &str = "managed_acceptance:spend_authorize";
pub const SCOPE_ATTEMPT_ADMIT: &str = "managed_acceptance:attempt_admit";
pub const SCOPE_REVOKE: &str = "managed_acceptance:revoke";

pub const ALL_MANAGED_ACCEPTANCE_SCOPES: &[&str] = &[
    SCOPE_RISK_ACKNOWLEDGE,
    SCOPE_SPEND_AUTHORIZE,
    SCOPE_ATTEMPT_ADMIT,
    SCOPE_REVOKE,
];

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
    /// Derive a production principal from verified store-owned API key metadata.
    /// Rejects missing, revoked, expired, inactive, forbidden, or under-scoped keys.
    pub fn authenticate_managed_acceptance_principal(
        &self,
        tenant_id: &str,
        key_id: &str,
        now_unix: Option<f64>,
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
            .get_api_key_metadata(key_id)?
            .ok_or_else(|| format!("api key {key_id} not found"))?;
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
        let _ = self.touch_api_key_last_used(key_id);
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
        // Require acknowledgement phrase embedded for store-derived accept.
        let _ = body
            .pointer("/acknowledgement/required_phrase")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or("decision body must embed acknowledgement.required_phrase")?;
        let trial = body
            .get("trial_envelope")
            .ok_or("decision body must embed trial_envelope")?;
        validate_trial_envelope_shape(trial)?;
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
        let now = self.now();
        if !is_strictly_after(&expires_at, &now)? {
            return Err("decision expires_at must be strictly after store clock now".into());
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
                let row = load_decision_pg(&mut tx, &decision_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }?;
        Ok(row)
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
                let _ = transition;
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
        let prepared = self.prepare_managed_codex_spawn(facts, allow_fixture_dry_run)?;
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
        let prepared = self.prepare_managed_codex_spawn(&lease.facts, false)?;
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
        if spend.get("status").and_then(Value::as_str) != Some("active") {
            return Err("managed Codex bound spend is not active for lease admission".to_string());
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
    // Scope and max expiry derived from decision only — never invent far-future expiry.
    let expires_at = require_finite_expiry(decision.get("expires_at").and_then(Value::as_str))?;
    if is_at_or_before(&expires_at, now)? {
        return Err("decision expired".into());
    }
    let scope = json!({
        "source": "persisted_decision",
        "trial_envelope": body.get("trial_envelope").cloned().unwrap_or(Value::Null),
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
    let _transition = persist_transition_pg(
        tx,
        decision,
        "draft_pending_operator",
        "operator_accepted",
        principal,
        now,
        "risk_acknowledgement_accepted",
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
        "in_flight" | "succeeded" | "failed" | "cancelled" | "outcome_unknown" | "replayed" => {
            Ok(())
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::codex_partial_mediation_authority_decision::OPERATOR_RISK_ACCEPTANCE_PHRASE;
    use std::sync::{Arc, Barrier};
    use std::thread;
    use tempfile::tempdir;

    fn store() -> (tempfile::TempDir, LocalProductStore) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ma.db");
        let store = LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
        (dir, store)
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

    fn seed_key(store: &LocalProductStore, key_id: &str) {
        store
            .record_api_key_metadata(
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
        seed_key(&store, "key_operator_ok");
        let p = store
            .authenticate_managed_acceptance_principal("tenant-a", "key_operator_ok", Some(1.0))
            .unwrap();
        assert_eq!(p.principal_id(), "key_operator_ok");
        assert!(p.has_scope(SCOPE_RISK_ACKNOWLEDGE));
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
}
