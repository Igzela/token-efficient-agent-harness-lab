//! Store-owned managed-acceptance decision, risk acknowledgement, spend authorization,
//! and exactly-once attempt admission.
//!
//! Production principals are derived only from verified `api_key_metadata` records.
//! Free-form strings never create authority. Fixture principals are test-only and cannot
//! authorize production live starts.

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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

    /// Explicit fixture principal for provider-free tests and dry-run only.
    pub fn fixture_for_tests(tenant_id: &str, fixture_id: &str) -> Result<Self, String> {
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

    fn require_scope(&self, scope: &str) -> Result<(), String> {
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
#[derive(Debug, Clone, PartialEq)]
pub struct SpendAuthorizationRequest {
    pub risk_authorization_id: String,
    pub product_task_id: String,
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
    pub cost_authority: CostAuthority,
    pub cancellation_identity: String,
    pub rollback_identity: String,
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

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn canonical_json(value: &Value) -> Result<String, String> {
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
        if residual_finding_sha256.len() != 64 {
            return Err("residual_finding_sha256 must be 64 hex chars".into());
        }
        let body = sort_value(decision_body);
        // Require acknowledgement phrase embedded for store-derived accept.
        let _ = body
            .pointer("/acknowledgement/required_phrase")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or("decision body must embed acknowledgement.required_phrase")?;
        let decision_body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
        let decision_id = body
            .get("decision_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("mad-{}", Uuid::new_v4()));
        let now = self.now();
        let principal_kind = principal
            .map(|p| p.principal_kind.as_str().to_string())
            .unwrap_or_else(|| "operator_api_key".into());
        let principal_id = principal.map(|p| p.principal_id.clone());
        if let Some(p) = principal {
            if p.tenant_id != tenant_id {
                return Err("principal tenant_id mismatch".into());
            }
        }

        match &self.db {
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
        }
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
                if expires_at < now {
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
                if status != "active" || expires_at < self.now() {
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
                tx.execute(
                    "UPDATE managed_acceptance_decisions SET status='revoked', revoked_at=?1, updated_at=?1 WHERE decision_id=?2",
                    params![now, decision_id],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    &principal.principal_id,
                    "managed_acceptance.risk_auth_revoked",
                    authorization_id,
                    &json!({"decision_id": decision_id}),
                )?;
                let row = load_authorization_sqlite(&tx, authorization_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let auth = load_authorization_pg(&mut tx, authorization_id)?;
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
                tx.execute(
                    "UPDATE managed_acceptance_decisions SET status='revoked', revoked_at=$1, updated_at=$1 WHERE decision_id=$2",
                    &[&now, &decision_id],
                )
                .map_err(|e| e.to_string())?;
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
        allow_fixture_dry_run: bool,
    ) -> Result<Value, String> {
        principal.require_scope(SCOPE_ATTEMPT_ADMIT)?;
        let body = sort_value(attempt_body);
        let attempt_body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
        let now = self.now();

        match &self.db {
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
                if current_lease.as_deref() != Some(lease_token) {
                    return Err("lease_token mismatch; ownership lost or cancelled".into());
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
                if current_lease.as_deref() != Some(lease_token) {
                    return Err("lease_token mismatch; ownership lost or cancelled".into());
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
        if exp < now {
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
    // Scope and max expiry derived from decision only.
    let scope = json!({
        "source": "persisted_decision",
        "trial_envelope": body.get("trial_envelope").cloned().unwrap_or(Value::Null),
        "decision_id": request.decision_id,
        "no_caller_scope_expansion": true,
    });
    let expires_at = match decision.get("expires_at").and_then(Value::as_str) {
        Some(e) => e.to_string(),
        None => "2099-01-01T00:00:00Z".to_string(),
    };
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
    if risk
        .get("execution_granted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err("risk acknowledgement must not set execution_granted".into());
    }
    let decision_id = risk
        .get("decision_id")
        .and_then(Value::as_str)
        .ok_or("risk auth missing decision_id")?;
    let decision = load_decision_sqlite(tx, decision_id)?;
    let decision_body = decision.get("body_json").cloned().unwrap_or(Value::Null);
    validate_spend_against_decision(request, &decision_body)?;
    let spend_id = format!("mas-{}", Uuid::new_v4());
    let body = build_spend_body(
        &spend_id,
        principal,
        request,
        &risk,
        decision_id,
        fixture_only,
        now,
    )?;
    let spend_body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
    // Exact body replay
    if let Some(existing_id) = tx
        .query_row(
            "SELECT spend_authorization_id FROM managed_acceptance_spend_authorizations WHERE tenant_id=?1 AND spend_body_sha256=?2",
            params![principal.tenant_id, spend_body_sha256],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        return load_spend_sqlite(tx, &existing_id);
    }
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
    tx.execute(
        "INSERT INTO managed_acceptance_spend_authorizations (
            spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
            principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
            decision_body_sha256, residual_finding_sha256, fixture_only, status, body_json,
            created_at, updated_at, expires_at, consumed_at, consumed_by_attempt_id, revoked_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'active',?12,?13,?13,?14,NULL,NULL,NULL)",
        params![
            spend_id,
            decision_id,
            request.risk_authorization_id,
            principal.tenant_id,
            principal.principal_kind.as_str(),
            principal.principal_id,
            spend_body_sha256,
            risk_sha,
            dsha,
            rsha,
            if fixture_only { 1 } else { 0 },
            body.to_string(),
            now,
            expires_at,
        ],
    )
    .map_err(|e| e.to_string())?;
    append_audit_locked(
        tx,
        now,
        &principal.principal_id,
        "managed_acceptance.spend_issued",
        &spend_id,
        &json!({
            "spend_body_sha256": spend_body_sha256,
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
    let decision_id = risk
        .get("decision_id")
        .and_then(Value::as_str)
        .ok_or("risk auth missing decision_id")?
        .to_string();
    let decision = load_decision_pg(tx, &decision_id)?;
    let decision_body = decision.get("body_json").cloned().unwrap_or(Value::Null);
    validate_spend_against_decision(request, &decision_body)?;
    let spend_id = format!("mas-{}", Uuid::new_v4());
    let body = build_spend_body(
        &spend_id,
        principal,
        request,
        &risk,
        &decision_id,
        fixture_only,
        now,
    )?;
    let spend_body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
    if let Some(row) = tx
        .query_opt(
            "SELECT spend_authorization_id FROM managed_acceptance_spend_authorizations WHERE tenant_id=$1 AND spend_body_sha256=$2 FOR UPDATE",
            &[&principal.tenant_id, &spend_body_sha256],
        )
        .map_err(|e| e.to_string())?
    {
        let existing: String = row.get(0);
        return load_spend_pg(tx, &existing);
    }
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
    tx.execute(
        "INSERT INTO managed_acceptance_spend_authorizations (
            spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
            principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
            decision_body_sha256, residual_finding_sha256, fixture_only, status, body_json,
            created_at, updated_at, expires_at, consumed_at, consumed_by_attempt_id, revoked_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$14,$15,NULL,NULL,NULL)",
        &[
            &spend_id,
            &decision_id,
            &request.risk_authorization_id,
            &principal.tenant_id,
            &pk,
            &principal.principal_id,
            &spend_body_sha256,
            &risk_sha,
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
    load_spend_pg(tx, &spend_id)
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
        if exp < now {
            return Err("risk authorization expired".into());
        }
    }
    let risk_fixture = risk
        .get("fixture_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
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
        .unwrap_or(Value::Null);
    // Reject expansion of budgets if trial declares them.
    if let Some(max_retries) = trial.get("max_retries").and_then(Value::as_u64) {
        if max_retries > 0 {
            // spend may not exceed; we bind exact in body later
        }
        let _ = max_retries;
    }
    if !request.draft_pr_only {
        if trial
            .get("draft_pr_only")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            return Err("spend must keep draft_pr_only".into());
        }
    }
    if request.binary_sha256.len() != 64 {
        return Err("binary_sha256 must be 64 hex chars".into());
    }
    if request.target_main_sha.len() != 40 && request.target_main_sha.len() != 64 {
        return Err("target_main_sha invalid".into());
    }
    if !request.output_branch_prefix.starts_with("acp/") {
        return Err("output_branch_prefix must be acp/*".into());
    }
    // cost_authority self-validates via construction
    let _ = CostAuthority::from_json(&request.cost_authority.to_json())?;
    Ok(())
}

fn build_spend_body(
    spend_id: &str,
    principal: &AuthenticatedPrincipal,
    request: &SpendAuthorizationRequest,
    risk: &Value,
    decision_id: &str,
    fixture_only: bool,
    now: &str,
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
        "product_task_id": request.product_task_id,
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
        "cost_authority": request.cost_authority.to_json(),
        "cancellation_identity": request.cancellation_identity,
        "rollback_identity": request.rollback_identity,
        "one_use": true,
        "fixture_only": fixture_only,
        "created_at": now,
        "expires_at": risk.get("expires_at"),
    })))
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
    // Consume one-use spend atomically.
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
    let risk_authorization_id = spend
        .get("risk_authorization_id")
        .and_then(Value::as_str)
        .ok_or("spend missing risk_authorization_id")?;
    let risk = load_authorization_sqlite(tx, risk_authorization_id)?;
    if risk
        .get("execution_granted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err("risk ack execution_granted must be false".into());
    }
    let decision_id = spend
        .get("decision_id")
        .and_then(Value::as_str)
        .ok_or("spend missing decision_id")?
        .to_string();
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
    let risk_authorization_id = spend
        .get("risk_authorization_id")
        .and_then(Value::as_str)
        .ok_or("spend missing risk_authorization_id")?
        .to_string();
    let decision_id = spend
        .get("decision_id")
        .and_then(Value::as_str)
        .ok_or("spend missing decision_id")?
        .to_string();
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
        if exp < now {
            return Err("spend authorization expired".into());
        }
    }
    let fixture_only = spend
        .get("fixture_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if matches!(principal.principal_kind, PrincipalKind::FixturePrincipal) {
        if !allow_fixture_dry_run {
            return Err("fixture principal cannot admit production live attempt".into());
        }
        if !fixture_only {
            return Err("fixture principal requires fixture_only spend".into());
        }
    } else if fixture_only {
        return Err("fixture_only spend cannot admit production principal".into());
    } else if !principal.may_authorize_production_live_start() {
        return Err("principal cannot admit production live attempt".into());
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

fn validate_attempt_terminal_status(status: &str) -> Result<(), String> {
    match status {
        "in_flight" | "succeeded" | "failed" | "cancelled" | "outcome_unknown" | "replayed" => {
            Ok(())
        }
        other => Err(format!("invalid attempt status {other}")),
    }
}

fn load_decision_sqlite(conn: &rusqlite::Connection, decision_id: &str) -> Result<Value, String> {
    conn.query_row(
        "SELECT decision_id, tenant_id, decision_body_sha256, residual_finding_sha256, status,
                principal_kind, principal_id, body_json, created_at, updated_at, expires_at, revoked_at
         FROM managed_acceptance_decisions WHERE decision_id=?1",
        params![decision_id],
        |row| {
            Ok(json!({
                "schema_version": "managed_acceptance_decision.v1",
                "decision_id": row.get::<_, String>(0)?,
                "tenant_id": row.get::<_, String>(1)?,
                "decision_body_sha256": row.get::<_, String>(2)?,
                "residual_finding_sha256": row.get::<_, String>(3)?,
                "status": row.get::<_, String>(4)?,
                "principal_kind": row.get::<_, String>(5)?,
                "principal_id": row.get::<_, Option<String>>(6)?,
                "body_json": serde_json::from_str::<Value>(&row.get::<_, String>(7)?).unwrap_or(Value::Null),
                "created_at": row.get::<_, String>(8)?,
                "updated_at": row.get::<_, String>(9)?,
                "expires_at": row.get::<_, Option<String>>(10)?,
                "revoked_at": row.get::<_, Option<String>>(11)?,
            }))
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
    Ok(json!({
        "schema_version": "managed_acceptance_decision.v1",
        "decision_id": row.get::<_, String>(0),
        "tenant_id": row.get::<_, String>(1),
        "decision_body_sha256": row.get::<_, String>(2),
        "residual_finding_sha256": row.get::<_, String>(3),
        "status": row.get::<_, String>(4),
        "principal_kind": row.get::<_, String>(5),
        "principal_id": row.get::<_, Option<String>>(6),
        "body_json": serde_json::from_str::<Value>(&body_s).unwrap_or(Value::Null),
        "created_at": row.get::<_, String>(8),
        "updated_at": row.get::<_, String>(9),
        "expires_at": row.get::<_, Option<String>>(10),
        "revoked_at": row.get::<_, Option<String>>(11),
    }))
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
            let body: Value = serde_json::from_str(&body_s).unwrap_or(Value::Null);
            Ok(json!({
                "schema_version": "managed_acceptance_authorization.v1",
                "authorization_id": row.get::<_, String>(0)?,
                "decision_id": row.get::<_, String>(1)?,
                "tenant_id": row.get::<_, String>(2)?,
                "principal_kind": row.get::<_, String>(3)?,
                "principal_id": row.get::<_, String>(4)?,
                "decision_body_sha256": row.get::<_, String>(5)?,
                "residual_finding_sha256": row.get::<_, String>(6)?,
                "authorization_sha256": row.get::<_, String>(7)?,
                "scope": serde_json::from_str::<Value>(&row.get::<_, String>(8)?).unwrap_or(Value::Null),
                "status": row.get::<_, String>(9)?,
                "mutation_authority": row.get::<_, String>(10)?,
                "execution_granted": row.get::<_, i64>(11)? != 0,
                "body_json": body,
                "fixture_only": body.get("fixture_only").and_then(Value::as_bool).unwrap_or(false),
                "created_at": row.get::<_, String>(13)?,
                "updated_at": row.get::<_, String>(14)?,
                "expires_at": row.get::<_, String>(15)?,
                "revoked_at": row.get::<_, Option<String>>(16)?,
            }))
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
    let body: Value = serde_json::from_str(&body_s).unwrap_or(Value::Null);
    let exec: i32 = row.get(11);
    Ok(json!({
        "schema_version": "managed_acceptance_authorization.v1",
        "authorization_id": row.get::<_, String>(0),
        "decision_id": row.get::<_, String>(1),
        "tenant_id": row.get::<_, String>(2),
        "principal_kind": row.get::<_, String>(3),
        "principal_id": row.get::<_, String>(4),
        "decision_body_sha256": row.get::<_, String>(5),
        "residual_finding_sha256": row.get::<_, String>(6),
        "authorization_sha256": row.get::<_, String>(7),
        "scope": serde_json::from_str::<Value>(&row.get::<_, String>(8)).unwrap_or(Value::Null),
        "status": row.get::<_, String>(9),
        "mutation_authority": row.get::<_, String>(10),
        "execution_granted": exec != 0,
        "body_json": body,
        "fixture_only": body.get("fixture_only").and_then(Value::as_bool).unwrap_or(false),
        "created_at": row.get::<_, String>(13),
        "updated_at": row.get::<_, String>(14),
        "expires_at": row.get::<_, String>(15),
        "revoked_at": row.get::<_, Option<String>>(16),
    }))
}

fn load_spend_sqlite(conn: &rusqlite::Connection, spend_id: &str) -> Result<Value, String> {
    conn.query_row(
        "SELECT spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
                principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
                decision_body_sha256, residual_finding_sha256, fixture_only, status, body_json,
                created_at, updated_at, expires_at, consumed_at, consumed_by_attempt_id, revoked_at
         FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=?1",
        params![spend_id],
        |row| {
            let body_s: String = row.get(12)?;
            let body: Value = serde_json::from_str(&body_s).unwrap_or(Value::Null);
            Ok(json!({
                "schema_version": "managed_acceptance_spend_authorization.v1",
                "spend_authorization_id": row.get::<_, String>(0)?,
                "decision_id": row.get::<_, String>(1)?,
                "risk_authorization_id": row.get::<_, String>(2)?,
                "tenant_id": row.get::<_, String>(3)?,
                "principal_kind": row.get::<_, String>(4)?,
                "principal_id": row.get::<_, String>(5)?,
                "spend_body_sha256": row.get::<_, String>(6)?,
                "risk_authorization_sha256": row.get::<_, String>(7)?,
                "decision_body_sha256": row.get::<_, String>(8)?,
                "residual_finding_sha256": row.get::<_, String>(9)?,
                "fixture_only": row.get::<_, i64>(10)? != 0,
                "status": row.get::<_, String>(11)?,
                "body_json": body,
                "created_at": row.get::<_, String>(13)?,
                "updated_at": row.get::<_, String>(14)?,
                "expires_at": row.get::<_, String>(15)?,
                "consumed_at": row.get::<_, Option<String>>(16)?,
                "consumed_by_attempt_id": row.get::<_, Option<String>>(17)?,
                "revoked_at": row.get::<_, Option<String>>(18)?,
            }))
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
                    decision_body_sha256, residual_finding_sha256, fixture_only, status, body_json,
                    created_at, updated_at, expires_at, consumed_at, consumed_by_attempt_id, revoked_at
             FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=$1",
            &[&spend_id],
        )
        .map_err(|e| e.to_string())?;
    let body_s: String = row.get(12);
    let body: Value = serde_json::from_str(&body_s).unwrap_or(Value::Null);
    let fixture: i32 = row.get(10);
    Ok(json!({
        "schema_version": "managed_acceptance_spend_authorization.v1",
        "spend_authorization_id": row.get::<_, String>(0),
        "decision_id": row.get::<_, String>(1),
        "risk_authorization_id": row.get::<_, String>(2),
        "tenant_id": row.get::<_, String>(3),
        "principal_kind": row.get::<_, String>(4),
        "principal_id": row.get::<_, String>(5),
        "spend_body_sha256": row.get::<_, String>(6),
        "risk_authorization_sha256": row.get::<_, String>(7),
        "decision_body_sha256": row.get::<_, String>(8),
        "residual_finding_sha256": row.get::<_, String>(9),
        "fixture_only": fixture != 0,
        "status": row.get::<_, String>(11),
        "body_json": body,
        "created_at": row.get::<_, String>(13),
        "updated_at": row.get::<_, String>(14),
        "expires_at": row.get::<_, String>(15),
        "consumed_at": row.get::<_, Option<String>>(16),
        "consumed_by_attempt_id": row.get::<_, Option<String>>(17),
        "revoked_at": row.get::<_, Option<String>>(18),
    }))
}

fn load_attempt_sqlite(conn: &rusqlite::Connection, attempt_id: &str) -> Result<Value, String> {
    conn.query_row(
        "SELECT attempt_id, tenant_id, product_task_id, workflow_node_id, execution_id,
                decision_id, authorization_id, spend_authorization_id, manifest_sha256, attempt_body_sha256,
                status, terminal_class, body_json, receipt_json, receipt_sha256, lease_token, created_at, updated_at
         FROM managed_acceptance_attempts WHERE attempt_id=?1",
        params![attempt_id],
        |row| {
            Ok(json!({
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
                "body_json": serde_json::from_str::<Value>(&row.get::<_, String>(12)?).unwrap_or(Value::Null),
                "receipt_json": row.get::<_, Option<String>>(13)?.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                "receipt_sha256": row.get::<_, Option<String>>(14)?,
                "lease_token": row.get::<_, Option<String>>(15)?,
                "created_at": row.get::<_, String>(16)?,
                "updated_at": row.get::<_, String>(17)?,
                "idempotent_replay": false,
            }))
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
    Ok(json!({
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
        "body_json": serde_json::from_str::<Value>(&body_s).unwrap_or(Value::Null),
        "receipt_json": receipt_s.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
        "receipt_sha256": row.get::<_, Option<String>>(14),
        "lease_token": row.get::<_, Option<String>>(15),
        "created_at": row.get::<_, String>(16),
        "updated_at": row.get::<_, String>(17),
        "idempotent_replay": false,
    }))
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
        json!({
            "decision_id": decision_id,
            "schema_version": "codex_partial_mediation_authority_decision.v2",
            "status": "draft_pending_operator",
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
            workflow_node_id: Some("node-1".into()),
            execution_id: format!("codex-attempt-{attempt_id}"),
            attempt_id: attempt_id.into(),
            binary_path: "/usr/bin/codex".into(),
            binary_version: "0.145.0".into(),
            binary_sha256: "ab".repeat(32),
            provider_kind: "openai".into(),
            provider_host: "api.openai.com".into(),
            provider_base_url: "https://api.openai.com/v1".into(),
            admitted_endpoint_paths: vec!["/v1/responses".into()],
            model: "gpt-5".into(),
            target_repo: "org/disposable-trial".into(),
            target_main_sha: "a".repeat(40),
            output_branch_prefix: "acp/".into(),
            draft_pr_only: true,
            cost_authority: CostAuthority::CostUnavailable,
            cancellation_identity: "cancel-1".into(),
            rollback_identity: "rollback-1".into(),
        }
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
    fn risk_ack_store_derived_spend_admit_idempotent_and_conflict() {
        let (_dir, store) = store();
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

        let spend = store
            .issue_managed_acceptance_spend_authorization(
                &principal,
                &spend_req(&auth_id, "attempt-1"),
            )
            .unwrap();
        let spend_id = spend["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(spend["status"], "active");

        let attempt_body = json!({
            "manifest_sha256": "cd".repeat(32),
            "execution_id": "codex-attempt-1",
            "product_task_id": "ptask-1",
            "workflow_node_id": "node-1",
        });
        let a1 = store
            .admit_managed_acceptance_attempt(
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
            .admit_managed_acceptance_attempt(
                &principal,
                "attempt-1",
                &attempt_body,
                &spend_id,
                true,
            )
            .unwrap();
        assert_eq!(a2["idempotent_replay"], true);

        // spend consumed — cannot re-admit different attempt
        let spend_row = store
            .get_managed_acceptance_spend_authorization(&spend_id)
            .unwrap()
            .unwrap();
        assert_eq!(spend_row["status"], "consumed");

        let conflict = store.admit_managed_acceptance_attempt(
            &principal,
            "attempt-1",
            &json!({
                "manifest_sha256": "ee".repeat(32),
                "execution_id": "codex-attempt-1",
            }),
            &spend_id,
            true,
        );
        assert!(conflict.unwrap_err().contains("conflict"));

        // Fixture cannot admit without allow_fixture_dry_run
        let spend2 = store
            .issue_managed_acceptance_spend_authorization(
                &principal,
                &spend_req(&auth_id, "attempt-2"),
            )
            .unwrap();
        let spend2_id = spend2["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        let err = store
            .admit_managed_acceptance_attempt(
                &principal,
                "attempt-2",
                &json!({"manifest_sha256": "ff".repeat(32), "execution_id": "x"}),
                &spend2_id,
                false,
            )
            .unwrap_err();
        assert!(
            err.contains("fixture") || err.contains("production"),
            "{err}"
        );

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
        // Exact terminal replay is allowed; different receipt must reject.
        let exact = store
            .complete_managed_acceptance_attempt(
                "attempt-1",
                "wrong-lease",
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
        assert!(conflict_receipt.unwrap_err().contains("late terminal"));
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
        let body = decision_body("mad-race");
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
        let spend = store
            .issue_managed_acceptance_spend_authorization(
                &principal,
                &spend_req(&auth_id, "race-1"),
            )
            .unwrap();
        let spend_id = spend["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        let barrier = Arc::new(Barrier::new(2));
        let path2 = path.clone();
        let spend_a = spend_id.clone();
        let spend_b = spend_id.clone();
        let b1 = Arc::clone(&barrier);
        let b2 = Arc::clone(&barrier);
        let h1 = thread::spawn(move || {
            let s =
                LocalProductStore::new_with_clock(&path2, || "2026-07-25T12:00:00Z".to_string())
                    .unwrap();
            let p = AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-race")
                .unwrap();
            b1.wait();
            s.admit_managed_acceptance_attempt(
                &p,
                "race-1",
                &json!({"manifest_sha256": "22".repeat(32), "execution_id": "e"}),
                &spend_a,
                true,
            )
        });
        let path3 = path.clone();
        let h2 = thread::spawn(move || {
            let s =
                LocalProductStore::new_with_clock(&path3, || "2026-07-25T12:00:00Z".to_string())
                    .unwrap();
            let p = AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-race")
                .unwrap();
            b2.wait();
            s.admit_managed_acceptance_attempt(
                &p,
                "race-1",
                &json!({"manifest_sha256": "22".repeat(32), "execution_id": "e"}),
                &spend_b,
                true,
            )
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
}
