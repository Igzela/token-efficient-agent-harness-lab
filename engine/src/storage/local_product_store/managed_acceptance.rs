//! Store-owned managed-acceptance decision, authorization, and attempt admission.
//!
//! Free-form actor strings are never authority. Principals come from authenticated
//! product/API context (or explicit fixture principal in tests only).

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};

/// Kind of authenticated principal. Fixture is test-only and cannot authorize production live spend.
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

/// Authenticated principal bound by store/API authority (not a free-form display name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPrincipal {
    pub tenant_id: String,
    pub principal_id: String,
    pub principal_kind: PrincipalKind,
    pub scopes: Vec<String>,
}

impl AuthenticatedPrincipal {
    /// Build from API auth context. Rejects empty/system/automation identities.
    pub fn from_api_key(
        tenant_id: &str,
        api_key_id: &str,
        scopes: impl IntoIterator<Item = String>,
    ) -> Result<Self, String> {
        let tenant_id = tenant_id.trim();
        let principal_id = api_key_id.trim();
        if tenant_id.is_empty() || principal_id.is_empty() {
            return Err("authenticated principal requires tenant_id and api_key_id".into());
        }
        if is_forbidden_principal_id(principal_id) {
            return Err(format!(
                "principal_id {principal_id:?} is not admitted for operator authority"
            ));
        }
        if principal_id == "none" {
            return Err(
                "unauthenticated api_key_id=none cannot authorize managed acceptance".into(),
            );
        }
        Ok(Self {
            tenant_id: tenant_id.to_string(),
            principal_id: principal_id.to_string(),
            principal_kind: PrincipalKind::OperatorApiKey,
            scopes: scopes.into_iter().collect(),
        })
    }

    /// Explicit fixture principal for provider-free tests only.
    pub fn fixture_for_tests(tenant_id: &str, fixture_id: &str) -> Result<Self, String> {
        let fixture_id = fixture_id.trim();
        if !fixture_id.starts_with("fixture-principal-") {
            return Err("fixture principal id must use fixture-principal- prefix".into());
        }
        Ok(Self {
            tenant_id: tenant_id.trim().to_string(),
            principal_id: fixture_id.to_string(),
            principal_kind: PrincipalKind::FixturePrincipal,
            scopes: vec!["dispatch:execute".into(), "product:manage".into()],
        })
    }

    pub fn may_authorize_production_live_start(&self) -> bool {
        matches!(self.principal_kind, PrincipalKind::OperatorApiKey)
            && !is_forbidden_principal_id(&self.principal_id)
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
    ]
    .iter()
    .any(|f| {
        lower == *f || lower.starts_with(&format!("{f}-")) || lower.starts_with(&format!("{f}_"))
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn canonical_json(value: &Value) -> Result<String, String> {
    // Deterministic: serde_json Map is ordered by insertion; callers must build
    // with sorted keys via BTreeMap when needed. We re-parse through Value::Object
    // sorted keys for stability.
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

impl LocalProductStore {
    /// Persist a draft or accepted decision body. Body must already carry full canonical fields.
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
                let tx = conn
                    .unchecked_transaction()
                    .map_err(|e| e.to_string())?;
                // Conflicting body for same decision_id fails closed.
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
                        return Err("managed acceptance decision body conflict for decision_id".into());
                    }
                    // Exact replay returns existing.
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
                        return Err("managed acceptance decision body conflict for decision_id".into());
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

    /// Operator risk acceptance: multi-field phrase + hashes validated by caller; store binds principal.
    pub fn accept_managed_acceptance_decision(
        &self,
        principal: &AuthenticatedPrincipal,
        decision_id: &str,
        decision_body_sha256: &str,
        residual_finding_sha256: &str,
        required_phrase: &str,
        submitted_phrase: &str,
        explicit_go: bool,
        scope: &Value,
        expires_at: &str,
    ) -> Result<Value, String> {
        if !explicit_go {
            return Err("explicit_go required".into());
        }
        if submitted_phrase != required_phrase {
            return Err("operator risk-acceptance phrase mismatch".into());
        }
        if matches!(principal.principal_kind, PrincipalKind::FixturePrincipal) {
            // Fixture may accept for dry-run only; execution_granted stays false always.
        } else if !principal.may_authorize_production_live_start() {
            return Err("principal cannot authorize managed acceptance".into());
        }
        if is_forbidden_principal_id(&principal.principal_id)
            && !matches!(principal.principal_kind, PrincipalKind::FixturePrincipal)
        {
            return Err("forbidden principal".into());
        }
        let now = self.now();
        let auth_id = format!("maa-{}", Uuid::new_v4());

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
                let decision = load_decision_sqlite(&tx, decision_id)?;
                if decision.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id.as_str())
                {
                    return Err("decision tenant mismatch".into());
                }
                if decision.get("decision_body_sha256").and_then(Value::as_str)
                    != Some(decision_body_sha256)
                {
                    return Err("decision_body_sha256 mismatch".into());
                }
                if decision
                    .get("residual_finding_sha256")
                    .and_then(Value::as_str)
                    != Some(residual_finding_sha256)
                {
                    return Err("residual_finding_sha256 mismatch".into());
                }
                let status = decision.get("status").and_then(Value::as_str).unwrap_or("");
                if status == "revoked" || status == "invalidated" || status == "expired" {
                    return Err(format!("decision status {status} cannot be accepted"));
                }
                // Exact replay of same principal+hashes returns existing auth.
                if let Some(existing) = tx
                    .query_row(
                        "SELECT authorization_id FROM managed_acceptance_authorizations
                         WHERE decision_id=?1 AND principal_id=?2 AND decision_body_sha256=?3",
                        params![decision_id, principal.principal_id, decision_body_sha256],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                {
                    return load_authorization_sqlite(&tx, &existing);
                }
                let auth_body = sort_value(&json!({
                    "schema_version": "managed_acceptance_authorization.v1",
                    "authorization_id": auth_id,
                    "decision_id": decision_id,
                    "tenant_id": principal.tenant_id,
                    "principal_kind": principal.principal_kind.as_str(),
                    "principal_id": principal.principal_id,
                    "decision_body_sha256": decision_body_sha256,
                    "residual_finding_sha256": residual_finding_sha256,
                    "scope": scope,
                    "expires_at": expires_at,
                    "mutation_authority": "authorization_receipt_only",
                    "execution_granted": false,
                    "fixture_only": matches!(principal.principal_kind, PrincipalKind::FixturePrincipal),
                }));
                let authorization_sha256 = sha256_hex(canonical_json(&auth_body)?.as_bytes());
                tx.execute(
                    "UPDATE managed_acceptance_decisions SET status='operator_accepted', principal_kind=?1, principal_id=?2, updated_at=?3 WHERE decision_id=?4",
                    params![
                        principal.principal_kind.as_str(),
                        principal.principal_id,
                        now,
                        decision_id
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
                        decision_id,
                        principal.tenant_id,
                        principal.principal_kind.as_str(),
                        principal.principal_id,
                        decision_body_sha256,
                        residual_finding_sha256,
                        authorization_sha256,
                        scope.to_string(),
                        auth_body.to_string(),
                        now,
                        expires_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    &principal.principal_id,
                    "managed_acceptance.decision_accepted",
                    decision_id,
                    &json!({
                        "authorization_id": auth_id,
                        "authorization_sha256": authorization_sha256,
                        "execution_granted": false,
                    }),
                )?;
                let row = load_authorization_sqlite(&tx, &auth_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!("maa:{decision_id}")],
                )
                .map_err(|e| e.to_string())?;
                let decision = load_decision_pg(&mut tx, decision_id)?;
                if decision.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id.as_str())
                {
                    return Err("decision tenant mismatch".into());
                }
                if decision.get("decision_body_sha256").and_then(Value::as_str)
                    != Some(decision_body_sha256)
                {
                    return Err("decision_body_sha256 mismatch".into());
                }
                if let Some(row) = tx
                    .query_opt(
                        "SELECT authorization_id FROM managed_acceptance_authorizations
                         WHERE decision_id=$1 AND principal_id=$2 AND decision_body_sha256=$3 FOR UPDATE",
                        &[&decision_id, &principal.principal_id, &decision_body_sha256],
                    )
                    .map_err(|e| e.to_string())?
                {
                    let existing: String = row.get(0);
                    return load_authorization_pg(&mut tx, &existing);
                }
                let auth_body = sort_value(&json!({
                    "schema_version": "managed_acceptance_authorization.v1",
                    "authorization_id": auth_id,
                    "decision_id": decision_id,
                    "tenant_id": principal.tenant_id,
                    "principal_kind": principal.principal_kind.as_str(),
                    "principal_id": principal.principal_id,
                    "decision_body_sha256": decision_body_sha256,
                    "residual_finding_sha256": residual_finding_sha256,
                    "scope": scope,
                    "expires_at": expires_at,
                    "mutation_authority": "authorization_receipt_only",
                    "execution_granted": false,
                    "fixture_only": matches!(principal.principal_kind, PrincipalKind::FixturePrincipal),
                }));
                let authorization_sha256 = sha256_hex(canonical_json(&auth_body)?.as_bytes());
                let pk = principal.principal_kind.as_str();
                tx.execute(
                    "UPDATE managed_acceptance_decisions SET status='operator_accepted', principal_kind=$1, principal_id=$2, updated_at=$3 WHERE decision_id=$4",
                    &[&pk, &principal.principal_id, &now, &decision_id],
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
                        &decision_id,
                        &principal.tenant_id,
                        &pk,
                        &principal.principal_id,
                        &decision_body_sha256,
                        &residual_finding_sha256,
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
                let row = load_authorization_pg(&mut tx, &auth_id)?;
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

    pub fn revoke_managed_acceptance_authorization(
        &self,
        principal: &AuthenticatedPrincipal,
        authorization_id: &str,
    ) -> Result<Value, String> {
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
                let auth = load_authorization_sqlite(&tx, authorization_id)?;
                if auth.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id.as_str())
                    && !principal.scopes.iter().any(|s| s == "team:admin")
                {
                    return Err("principal cannot revoke this authorization".into());
                }
                tx.execute(
                    "UPDATE managed_acceptance_authorizations SET status='revoked', revoked_at=?1, updated_at=?1 WHERE authorization_id=?2",
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

    /// Exactly-once attempt admission. Same body → idempotent replay; conflict → reject.
    pub fn admit_managed_acceptance_attempt(
        &self,
        principal: &AuthenticatedPrincipal,
        attempt_id: &str,
        attempt_body: &Value,
        authorization_id: &str,
        allow_fixture_dry_run: bool,
    ) -> Result<Value, String> {
        let body = sort_value(attempt_body);
        let attempt_body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
        let now = self.now();

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
                let auth = load_authorization_sqlite(&tx, authorization_id)?;
                validate_auth_for_attempt(&auth, principal, allow_fixture_dry_run, &now)?;
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
                    // Exact idempotent replay
                    let mut row = load_attempt_sqlite(&tx, attempt_id)?;
                    if let Value::Object(ref mut m) = row {
                        m.insert("idempotent_replay".into(), json!(true));
                    }
                    return Ok(row);
                }
                // Also reject same body under different attempt_id
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
                let decision_id = auth
                    .get("decision_id")
                    .and_then(Value::as_str)
                    .ok_or("authorization missing decision_id")?
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
                tx.execute(
                    "INSERT INTO managed_acceptance_attempts (
                        attempt_id, tenant_id, product_task_id, workflow_node_id, execution_id,
                        decision_id, authorization_id, manifest_sha256, attempt_body_sha256,
                        status, terminal_class, body_json, receipt_json, created_at, updated_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'admitted',NULL,?10,NULL,?11,?11)",
                    params![
                        attempt_id,
                        principal.tenant_id,
                        product_task_id,
                        workflow_node_id,
                        execution_id,
                        decision_id,
                        authorization_id,
                        manifest_sha256,
                        attempt_body_sha256,
                        body.to_string(),
                        now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                let row = load_attempt_sqlite(&tx, attempt_id)?;
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
                let auth = load_authorization_pg(&mut tx, authorization_id)?;
                validate_auth_for_attempt(&auth, principal, allow_fixture_dry_run, &now)?;
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
                    let mut existing = load_attempt_pg(&mut tx, attempt_id)?;
                    if let Value::Object(ref mut m) = existing {
                        m.insert("idempotent_replay".into(), json!(true));
                    }
                    return Ok(existing);
                }
                let decision_id = auth
                    .get("decision_id")
                    .and_then(Value::as_str)
                    .ok_or("authorization missing decision_id")?
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
                let status = "admitted";
                tx.execute(
                    "INSERT INTO managed_acceptance_attempts (
                        attempt_id, tenant_id, product_task_id, workflow_node_id, execution_id,
                        decision_id, authorization_id, manifest_sha256, attempt_body_sha256,
                        status, terminal_class, body_json, receipt_json, created_at, updated_at
                     ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NULL,$11,NULL,$12,$12)",
                    &[
                        &attempt_id,
                        &principal.tenant_id,
                        &product_task_id,
                        &workflow_node_id,
                        &execution_id,
                        &decision_id,
                        &authorization_id,
                        &manifest_sha256,
                        &attempt_body_sha256,
                        &status,
                        &body.to_string(),
                        &now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                let row = load_attempt_pg(&mut tx, attempt_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    pub fn complete_managed_acceptance_attempt(
        &self,
        attempt_id: &str,
        status: &str,
        terminal_class: &str,
        receipt: &Value,
    ) -> Result<Value, String> {
        validate_attempt_terminal_status(status)?;
        let now = self.now();
        let receipt_s = receipt.to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
                let current: String = tx
                    .query_row(
                        "SELECT status FROM managed_acceptance_attempts WHERE attempt_id=?1",
                        params![attempt_id],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if matches!(
                    current.as_str(),
                    "succeeded" | "failed" | "cancelled" | "outcome_unknown" | "replayed"
                ) {
                    // Late write after terminal: reject unless exact same receipt replay.
                    let existing = load_attempt_sqlite(&tx, attempt_id)?;
                    if existing.get("status").and_then(Value::as_str) == Some(status)
                        && existing.get("terminal_class").and_then(Value::as_str)
                            == Some(terminal_class)
                    {
                        return Ok(existing);
                    }
                    return Err("late terminal write after attempt already terminal".into());
                }
                tx.execute(
                    "UPDATE managed_acceptance_attempts SET status=?1, terminal_class=?2, receipt_json=?3, updated_at=?4 WHERE attempt_id=?5",
                    params![status, terminal_class, receipt_s, now, attempt_id],
                )
                .map_err(|e| e.to_string())?;
                let row = load_attempt_sqlite(&tx, attempt_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let current: String = tx
                    .query_one(
                        "SELECT status FROM managed_acceptance_attempts WHERE attempt_id=$1 FOR UPDATE",
                        &[&attempt_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                if matches!(
                    current.as_str(),
                    "succeeded" | "failed" | "cancelled" | "outcome_unknown" | "replayed"
                ) {
                    let existing = load_attempt_pg(&mut tx, attempt_id)?;
                    if existing.get("status").and_then(Value::as_str) == Some(status) {
                        return Ok(existing);
                    }
                    return Err("late terminal write after attempt already terminal".into());
                }
                tx.execute(
                    "UPDATE managed_acceptance_attempts SET status=$1, terminal_class=$2, receipt_json=$3, updated_at=$4 WHERE attempt_id=$5",
                    &[&status, &terminal_class, &receipt_s, &now, &attempt_id],
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

fn validate_auth_for_attempt(
    auth: &Value,
    principal: &AuthenticatedPrincipal,
    allow_fixture_dry_run: bool,
    now: &str,
) -> Result<(), String> {
    if auth.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id.as_str()) {
        return Err("authorization tenant mismatch".into());
    }
    if auth.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id.as_str()) {
        return Err("authorization principal mismatch".into());
    }
    if auth.get("status").and_then(Value::as_str) != Some("active") {
        return Err("authorization is not active".into());
    }
    if let Some(exp) = auth.get("expires_at").and_then(Value::as_str) {
        if exp < now {
            return Err("authorization expired".into());
        }
    }
    let fixture_only = auth
        .get("body_json")
        .and_then(|b| {
            // body may be nested already expanded
            b.get("fixture_only").or_else(|| auth.get("fixture_only"))
        })
        .and_then(Value::as_bool)
        .or_else(|| auth.get("fixture_only").and_then(Value::as_bool))
        .unwrap_or(false);
    // Also parse body_json string if present
    let fixture_only = if !fixture_only {
        auth.get("fixture_only")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| {
                auth.get("body_json")
                    .and_then(Value::as_str)
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .and_then(|v| v.get("fixture_only").and_then(Value::as_bool))
                    .unwrap_or(false)
            })
    } else {
        true
    };
    if matches!(principal.principal_kind, PrincipalKind::FixturePrincipal) {
        if !allow_fixture_dry_run {
            return Err("fixture principal cannot admit production live attempt".into());
        }
        if !fixture_only {
            return Err("fixture principal requires fixture_only authorization".into());
        }
    } else if fixture_only {
        return Err("fixture_only authorization cannot admit production principal".into());
    }
    // execution_granted is always false on ack; live path still needs separate spend auth later.
    Ok(())
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

fn load_attempt_sqlite(conn: &rusqlite::Connection, attempt_id: &str) -> Result<Value, String> {
    conn.query_row(
        "SELECT attempt_id, tenant_id, product_task_id, workflow_node_id, execution_id,
                decision_id, authorization_id, manifest_sha256, attempt_body_sha256,
                status, terminal_class, body_json, receipt_json, created_at, updated_at
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
                "manifest_sha256": row.get::<_, String>(7)?,
                "attempt_body_sha256": row.get::<_, String>(8)?,
                "status": row.get::<_, String>(9)?,
                "terminal_class": row.get::<_, Option<String>>(10)?,
                "body_json": serde_json::from_str::<Value>(&row.get::<_, String>(11)?).unwrap_or(Value::Null),
                "receipt_json": row.get::<_, Option<String>>(12)?.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                "created_at": row.get::<_, String>(13)?,
                "updated_at": row.get::<_, String>(14)?,
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
                    decision_id, authorization_id, manifest_sha256, attempt_body_sha256,
                    status, terminal_class, body_json, receipt_json, created_at, updated_at
             FROM managed_acceptance_attempts WHERE attempt_id=$1",
            &[&attempt_id],
        )
        .map_err(|e| e.to_string())?;
    let body_s: String = row.get(11);
    let receipt_s: Option<String> = row.get(12);
    Ok(json!({
        "schema_version": "managed_acceptance_attempt.v1",
        "attempt_id": row.get::<_, String>(0),
        "tenant_id": row.get::<_, String>(1),
        "product_task_id": row.get::<_, Option<String>>(2),
        "workflow_node_id": row.get::<_, Option<String>>(3),
        "execution_id": row.get::<_, String>(4),
        "decision_id": row.get::<_, String>(5),
        "authorization_id": row.get::<_, String>(6),
        "manifest_sha256": row.get::<_, String>(7),
        "attempt_body_sha256": row.get::<_, String>(8),
        "status": row.get::<_, String>(9),
        "terminal_class": row.get::<_, Option<String>>(10),
        "body_json": serde_json::from_str::<Value>(&body_s).unwrap_or(Value::Null),
        "receipt_json": receipt_s.and_then(|s| serde_json::from_str(&s).ok()),
        "created_at": row.get::<_, String>(13),
        "updated_at": row.get::<_, String>(14),
        "idempotent_replay": false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> (tempfile::TempDir, LocalProductStore) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ma.db");
        let store = LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
        (dir, store)
    }

    #[test]
    fn free_form_agent_principal_rejected() {
        let err = AuthenticatedPrincipal::from_api_key("t1", "agent", vec![]).unwrap_err();
        assert!(err.contains("not admitted"));
        let err = AuthenticatedPrincipal::from_api_key("t1", "none", vec![]).unwrap_err();
        assert!(
            err.contains("unauthenticated") || err.contains("not admitted"),
            "{err}"
        );
    }

    #[test]
    fn decision_accept_attempt_idempotent_and_conflict() {
        let (_dir, store) = store();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-alice")
                .unwrap();
        let residual = "ab".repeat(32);
        let body = json!({
            "decision_id": "mad-test-1",
            "schema_version": "codex_partial_mediation_authority_decision.v2",
            "trial": {"max_retries": 0, "max_provider_requests": 1},
        });
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
                "mad-test-1",
                &dsha,
                &residual,
                "I ACCEPT residual Codex partial-mediation risk for one disposable bounded trial",
                "I ACCEPT residual Codex partial-mediation risk for one disposable bounded trial",
                true,
                &json!({"dry_run": true}),
                "2026-07-26T00:00:00Z",
            )
            .unwrap();
        assert_eq!(auth["execution_granted"], false);
        assert_eq!(auth["mutation_authority"], "authorization_receipt_only");
        assert!(auth["fixture_only"].as_bool().unwrap_or(false));
        let auth_id = auth["authorization_id"].as_str().unwrap().to_string();

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
                &auth_id,
                true,
            )
            .unwrap();
        assert_eq!(a1["status"], "admitted");
        let a2 = store
            .admit_managed_acceptance_attempt(
                &principal,
                "attempt-1",
                &attempt_body,
                &auth_id,
                true,
            )
            .unwrap();
        assert_eq!(a2["idempotent_replay"], true);

        let conflict = store.admit_managed_acceptance_attempt(
            &principal,
            "attempt-1",
            &json!({
                "manifest_sha256": "ee".repeat(32),
                "execution_id": "codex-attempt-1",
            }),
            &auth_id,
            true,
        );
        assert!(conflict.unwrap_err().contains("conflict"));

        // Fixture cannot admit without allow_fixture_dry_run
        let err = store
            .admit_managed_acceptance_attempt(
                &principal,
                "attempt-2",
                &json!({"manifest_sha256": "ff".repeat(32), "execution_id": "x"}),
                &auth_id,
                false,
            )
            .unwrap_err();
        assert!(err.contains("fixture") || err.contains("production"));
    }

    #[test]
    fn revocation_and_late_terminal_write() {
        let (_dir, store) = store();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-bob").unwrap();
        let residual = "11".repeat(32);
        let body = json!({"decision_id": "mad-rev", "v": 1});
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
                "mad-rev",
                &dsha,
                &residual,
                "I ACCEPT residual Codex partial-mediation risk for one disposable bounded trial",
                "I ACCEPT residual Codex partial-mediation risk for one disposable bounded trial",
                true,
                &json!({}),
                "2026-07-26T00:00:00Z",
            )
            .unwrap();
        let auth_id = auth["authorization_id"].as_str().unwrap().to_string();
        store
            .admit_managed_acceptance_attempt(
                &principal,
                "att-rev",
                &json!({"manifest_sha256": "22".repeat(32), "execution_id": "e1"}),
                &auth_id,
                true,
            )
            .unwrap();
        store
            .complete_managed_acceptance_attempt(
                "att-rev",
                "succeeded",
                "succeeded_fixture",
                &json!({"ok": true}),
            )
            .unwrap();
        let late = store.complete_managed_acceptance_attempt(
            "att-rev",
            "failed",
            "failed_provider",
            &json!({"ok": false}),
        );
        assert!(late.unwrap_err().contains("late terminal"));
        let revoked = store
            .revoke_managed_acceptance_authorization(&principal, &auth_id)
            .unwrap();
        assert_eq!(revoked["status"], "revoked");
        assert!(store
            .get_active_managed_acceptance_authorization(&auth_id)
            .unwrap()
            .is_none());
    }
}
