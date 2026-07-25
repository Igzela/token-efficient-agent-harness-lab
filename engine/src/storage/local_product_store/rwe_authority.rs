//! Store-owned RWE run authorization, admission, and evidence persistence.
//!
//! Authorization creation is authenticated-principal only. The raw body/hash upsert is
//! private; the owner recomputes the canonical body hash. Run admission revalidates the
//! complete authorization envelope. Task attempts and terminal receipts are
//! exact-replay-or-conflict (no UPSERT mutation). Terminalization requires the current
//! lease token.

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{append_audit_locked, AuthenticatedPrincipal, DatabaseConnection, LocalProductStore};

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
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

fn canonical_json(value: &Value) -> Result<String, String> {
    Ok(sort_value(value).to_string())
}

/// Request to issue a one-use RWE run authorization (owner recomputes body/hash).
#[derive(Debug, Clone, PartialEq)]
pub struct RweAuthorizationIssueRequest {
    pub authorization_id: String,
    pub corpus_sha256: String,
    pub golden_path_terminal_evidence_id: String,
    pub task_ids: Vec<String>,
    pub max_total_provider_requests: u64,
    pub max_total_tokens: u64,
    pub max_wall_time_ms: u64,
    pub cost_authority: super::CostAuthority,
    pub target_repo: String,
    pub target_main_sha: String,
    pub executor_identity: String,
    pub model_identity: String,
    pub draft_pr_only: bool,
    pub expires_at: String,
    pub fixture_only: bool,
}

impl LocalProductStore {
    /// Authenticated-only RWE authorization creation. Recomputes canonical body/hash inside
    /// the owner; caller-supplied body_sha is never trusted.
    pub fn issue_rwe_run_authorization(
        &self,
        principal: &AuthenticatedPrincipal,
        request: &RweAuthorizationIssueRequest,
    ) -> Result<Value, String> {
        principal.require_scope(super::SCOPE_SPEND_AUTHORIZE)?;
        let fixture_only = request.fixture_only;
        if fixture_only
            != matches!(
                principal.principal_kind(),
                super::PrincipalKind::FixturePrincipal
            )
        {
            return Err("fixture_only mismatch with principal kind".into());
        }
        if !fixture_only && !principal.may_authorize_production_live_start() {
            return Err("principal cannot authorize production RWE spend".into());
        }
        if request.corpus_sha256.len() != 64
            || !request.corpus_sha256.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err("corpus_sha256 must be 64 hex chars".into());
        }
        if request.expires_at.trim().is_empty() || request.expires_at.starts_with("2099") {
            return Err("finite expires_at required".into());
        }
        if request.task_ids.is_empty() {
            return Err("task_ids required".into());
        }
        if request.target_repo.trim().is_empty() {
            return Err("target_repo required".into());
        }
        if request.target_main_sha.len() != 40 && request.target_main_sha.len() != 64 {
            return Err("target_main_sha invalid".into());
        }
        if !request.draft_pr_only {
            return Err("draft_pr_only required".into());
        }
        if request.max_total_provider_requests == 0
            || request.max_total_tokens == 0
            || request.max_wall_time_ms == 0
        {
            return Err("aggregate budgets must be positive".into());
        }
        // Bind / verify Golden Path terminal evidence when not fixture-only.
        if !fixture_only {
            if request.golden_path_terminal_evidence_id.trim().is_empty() {
                return Err("golden_path_terminal_evidence_id required for live RWE".into());
            }
            let te = self
                .get_product_task_terminal_evidence(&request.golden_path_terminal_evidence_id)
                .or_else(|_| {
                    // Allow lookup by task_id stored as evidence id for fixtures.
                    self.get_product_task_terminal_evidence(
                        request.golden_path_terminal_evidence_id.trim(),
                    )
                });
            match te {
                Ok(ev) if !ev.is_null() => {
                    // Must be a real terminal evidence document.
                    if ev.get("schema_version").and_then(Value::as_str)
                        != Some("product_task_terminal_evidence.v2")
                    {
                        return Err(
                            "golden_path_terminal_evidence_id is not accepted terminal evidence"
                                .into(),
                        );
                    }
                }
                _ => {
                    return Err(
                        "golden_path_terminal_evidence_id not found in terminal-evidence owner"
                            .into(),
                    );
                }
            }
        } else if request.golden_path_terminal_evidence_id.trim().is_empty() {
            return Err(
                "golden_path_terminal_evidence_id required even for fixture_only rows".into(),
            );
        }

        let body_json = sort_value(&json!({
            "schema_version": "rwe_run_authorization.v1",
            "authorization_id": request.authorization_id,
            "tenant_id": principal.tenant_id(),
            "corpus_sha256": request.corpus_sha256,
            "golden_path_terminal_evidence_id": request.golden_path_terminal_evidence_id,
            "principal_id": principal.principal_id(),
            "principal_kind": principal.principal_kind().as_str(),
            "task_ids": request.task_ids,
            "max_total_provider_requests": request.max_total_provider_requests,
            "max_total_tokens": request.max_total_tokens,
            "max_wall_time_ms": request.max_wall_time_ms,
            "cost_authority": request.cost_authority.to_json(),
            "target_repo": request.target_repo,
            "target_main_sha": request.target_main_sha,
            "executor_identity": request.executor_identity,
            "model_identity": request.model_identity,
            "draft_pr_only": request.draft_pr_only,
            "one_use": true,
            "fixture_only": fixture_only,
            "expires_at": request.expires_at,
        }));
        let body_sha256 = sha256_hex(canonical_json(&body_json)?.as_bytes());
        self.insert_rwe_run_authorization_owned(
            principal.tenant_id(),
            &request.authorization_id,
            principal.principal_id(),
            principal.principal_kind().as_str(),
            &request.corpus_sha256,
            &body_sha256,
            &body_json,
            &request.expires_at,
            fixture_only,
        )
    }

    /// Private owner insert/exact-replay. Not a public caller-supplied body/hash bypass.
    fn insert_rwe_run_authorization_owned(
        &self,
        tenant_id: &str,
        authorization_id: &str,
        principal_id: &str,
        principal_kind: &str,
        corpus_sha256: &str,
        body_sha256: &str,
        body_json: &Value,
        expires_at: &str,
        fixture_only: bool,
    ) -> Result<Value, String> {
        let now = self.now();
        if expires_at <= now.as_str() {
            return Err("RWE authorization already expired at issue time".into());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                if let Some((existing_sha, _status)) = tx
                    .query_row(
                        "SELECT body_sha256, status FROM rwe_run_authorizations WHERE authorization_id=?1",
                        params![authorization_id],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                {
                    if existing_sha != body_sha256 {
                        return Err("conflicting RWE authorization body".into());
                    }
                    return load_rwe_auth_sqlite(&tx, authorization_id);
                }
                tx.execute(
                    "INSERT INTO rwe_run_authorizations (
                        authorization_id, tenant_id, principal_id, principal_kind, corpus_sha256,
                        body_sha256, body_json, fixture_only, status, created_at, updated_at,
                        expires_at, consumed_at, consumed_by_run_id, revoked_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'active',?9,?9,?10,NULL,NULL,NULL)",
                    params![
                        authorization_id,
                        tenant_id,
                        principal_id,
                        principal_kind,
                        corpus_sha256,
                        body_sha256,
                        body_json.to_string(),
                        if fixture_only { 1 } else { 0 },
                        now,
                        expires_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    principal_id,
                    "rwe.authorization_issued",
                    authorization_id,
                    &json!({"body_sha256": body_sha256, "fixture_only": fixture_only}),
                )?;
                let row = load_rwe_auth_sqlite(&tx, authorization_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!("rwea:{authorization_id}")],
                )
                .map_err(|e| e.to_string())?;
                if let Some(row) = tx
                    .query_opt(
                        "SELECT body_sha256 FROM rwe_run_authorizations WHERE authorization_id=$1 FOR UPDATE",
                        &[&authorization_id],
                    )
                    .map_err(|e| e.to_string())?
                {
                    let existing: String = row.get(0);
                    if existing != body_sha256 {
                        return Err("conflicting RWE authorization body".into());
                    }
                    return load_rwe_auth_pg(&mut tx, authorization_id);
                }
                let fixture_i: i32 = if fixture_only { 1 } else { 0 };
                let active = "active";
                tx.execute(
                    "INSERT INTO rwe_run_authorizations (
                        authorization_id, tenant_id, principal_id, principal_kind, corpus_sha256,
                        body_sha256, body_json, fixture_only, status, created_at, updated_at,
                        expires_at, consumed_at, consumed_by_run_id, revoked_at
                     ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10,$11,NULL,NULL,NULL)",
                    &[
                        &authorization_id,
                        &tenant_id,
                        &principal_id,
                        &principal_kind,
                        &corpus_sha256,
                        &body_sha256,
                        &body_json.to_string(),
                        &fixture_i,
                        &active,
                        &now,
                        &expires_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                let row = load_rwe_auth_pg(&mut tx, authorization_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    pub fn get_rwe_run(&self, run_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT run_id FROM rwe_runs WHERE run_id=?1",
                    params![run_id],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .map(|_| load_rwe_run_sqlite(conn, run_id))
                .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                if client
                    .query_opt("SELECT run_id FROM rwe_runs WHERE run_id=$1", &[&run_id])
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    Ok(Some(load_rwe_run_pg(client, run_id)?))
                } else {
                    Ok(None)
                }
            }),
        }
    }

    pub fn get_rwe_run_authorization(
        &self,
        authorization_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT authorization_id FROM rwe_run_authorizations WHERE authorization_id=?1",
                    params![authorization_id],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .map(|_| load_rwe_auth_sqlite(conn, authorization_id))
                .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                if client
                    .query_opt(
                        "SELECT authorization_id FROM rwe_run_authorizations WHERE authorization_id=$1",
                        &[&authorization_id],
                    )
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    Ok(Some(load_rwe_auth_pg(client, authorization_id)?))
                } else {
                    Ok(None)
                }
            }),
        }
    }

    pub fn revoke_rwe_run_authorization(
        &self,
        principal: &AuthenticatedPrincipal,
        authorization_id: &str,
    ) -> Result<Value, String> {
        principal.require_scope(super::SCOPE_REVOKE)?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let auth = load_rwe_auth_sqlite(&tx, authorization_id)?;
                if auth.get("principal_id").and_then(Value::as_str)
                    != Some(principal.principal_id())
                    && !principal.has_scope("team:admin")
                {
                    return Err("principal cannot revoke this RWE authorization".into());
                }
                tx.execute(
                    "UPDATE rwe_run_authorizations SET status='revoked', revoked_at=?1, updated_at=?1 WHERE authorization_id=?2 AND status='active'",
                    params![now, authorization_id],
                )
                .map_err(|e| e.to_string())?;
                let row = load_rwe_auth_sqlite(&tx, authorization_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let auth = load_rwe_auth_pg(&mut tx, authorization_id)?;
                if auth.get("principal_id").and_then(Value::as_str)
                    != Some(principal.principal_id())
                    && !principal.has_scope("team:admin")
                {
                    return Err("principal cannot revoke this RWE authorization".into());
                }
                tx.execute(
                    "UPDATE rwe_run_authorizations SET status='revoked', revoked_at=$1, updated_at=$1 WHERE authorization_id=$2 AND status='active'",
                    &[&now, &authorization_id],
                )
                .map_err(|e| e.to_string())?;
                let row = load_rwe_auth_pg(&mut tx, authorization_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    /// Admit a run: revalidate complete authorization envelope, compare run body, consume one-use.
    pub fn admit_rwe_run(
        &self,
        principal: &AuthenticatedPrincipal,
        run_id: &str,
        authorization_id: &str,
        run_body: &Value,
        allow_fixture: bool,
    ) -> Result<Value, String> {
        let now = self.now();
        let body = sort_value(run_body);
        let run_body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                if let Some(_status) = tx
                    .query_row(
                        "SELECT status FROM rwe_runs WHERE run_id=?1",
                        params![run_id],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                {
                    let existing = load_rwe_run_sqlite(&tx, run_id)?;
                    return exact_run_replay_or_conflict(&existing, &run_body_sha256);
                }
                let auth = load_rwe_auth_sqlite(&tx, authorization_id)?;
                validate_rwe_auth_for_admit(
                    &auth,
                    principal,
                    &body,
                    allow_fixture,
                    &now,
                )?;
                let updated = tx
                    .execute(
                        "UPDATE rwe_run_authorizations SET status='consumed', consumed_at=?1, consumed_by_run_id=?2, updated_at=?1 WHERE authorization_id=?3 AND status='active'",
                        params![now, run_id, authorization_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("RWE authorization already consumed".into());
                }
                let lease_token = format!("rwe-lease-{}", Uuid::new_v4());
                let admit_envelope = sort_value(&json!({
                    "admit_state": {
                        "lease_token": lease_token,
                        "run_body_sha256": run_body_sha256,
                        "run_body": body,
                    },
                    "live_baseline_sealed": false,
                    "provider_free_fixture_completion": false,
                    "live_provider_request": false,
                }));
                let corpus_sha256 = auth
                    .get("corpus_sha256")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                tx.execute(
                    "INSERT INTO rwe_runs (
                        run_id, tenant_id, authorization_id, corpus_sha256, principal_id,
                        status, evidence_json, evidence_sha256, created_at, updated_at
                     ) VALUES (?1,?2,?3,?4,?5,'admitted',?6,NULL,?7,?7)",
                    params![
                        run_id,
                        principal.tenant_id(),
                        authorization_id,
                        corpus_sha256,
                        principal.principal_id(),
                        admit_envelope.to_string(),
                        now
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    principal.principal_id(),
                    "rwe.run_admitted",
                    run_id,
                    &json!({
                        "authorization_id": authorization_id,
                        "run_body_sha256": run_body_sha256,
                    }),
                )?;
                let mut row = load_rwe_run_sqlite(&tx, run_id)?;
                if let Value::Object(ref mut m) = row {
                    m.insert("lease_token".into(), json!(lease_token));
                }
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!("rwer:{run_id}")],
                )
                .map_err(|e| e.to_string())?;
                if tx
                    .query_opt(
                        "SELECT status FROM rwe_runs WHERE run_id=$1 FOR UPDATE",
                        &[&run_id],
                    )
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    let existing = load_rwe_run_pg(&mut tx, run_id)?;
                    return exact_run_replay_or_conflict(&existing, &run_body_sha256);
                }
                let auth = load_rwe_auth_pg(&mut tx, authorization_id)?;
                validate_rwe_auth_for_admit(
                    &auth,
                    principal,
                    &body,
                    allow_fixture,
                    &now,
                )?;
                let updated = tx
                    .execute(
                        "UPDATE rwe_run_authorizations SET status='consumed', consumed_at=$1, consumed_by_run_id=$2, updated_at=$1 WHERE authorization_id=$3 AND status='active'",
                        &[&now, &run_id, &authorization_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("RWE authorization already consumed".into());
                }
                let lease_token = format!("rwe-lease-{}", Uuid::new_v4());
                let admit_envelope = sort_value(&json!({
                    "admit_state": {
                        "lease_token": lease_token,
                        "run_body_sha256": run_body_sha256,
                        "run_body": body,
                    },
                    "live_baseline_sealed": false,
                    "provider_free_fixture_completion": false,
                    "live_provider_request": false,
                }));
                let corpus_sha256 = auth
                    .get("corpus_sha256")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let status = "admitted";
                let tenant = principal.tenant_id();
                let pid = principal.principal_id();
                tx.execute(
                    "INSERT INTO rwe_runs (
                        run_id, tenant_id, authorization_id, corpus_sha256, principal_id,
                        status, evidence_json, evidence_sha256, created_at, updated_at
                     ) VALUES ($1,$2,$3,$4,$5,$6,$7,NULL,$8,$8)",
                    &[
                        &run_id,
                        &tenant,
                        &authorization_id,
                        &corpus_sha256,
                        &pid,
                        &status,
                        &admit_envelope.to_string(),
                        &now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                let mut row = load_rwe_run_pg(&mut tx, run_id)?;
                if let Value::Object(ref mut m) = row {
                    m.insert("lease_token".into(), json!(lease_token));
                }
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    /// Immutable exact-replay-or-conflict task-attempt persistence (no UPSERT mutation).
    pub fn persist_rwe_task_attempt(
        &self,
        run_id: &str,
        task_attempt_id: &str,
        task_id: &str,
        definition_sha256: &str,
        classification: &str,
        evidence: &Value,
    ) -> Result<Value, String> {
        let now = self.now();
        let evidence_sorted = sort_value(evidence);
        let evidence_s = canonical_json(&evidence_sorted)?;
        let evidence_sha = sha256_hex(evidence_s.as_bytes());
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                if let Some((existing_sha, existing_class)) = tx
                    .query_row(
                        "SELECT evidence_sha256, classification FROM rwe_task_attempts WHERE task_attempt_id=?1",
                        params![task_attempt_id],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                {
                    if existing_sha != evidence_sha || existing_class != classification {
                        return Err("conflicting RWE task-attempt evidence".into());
                    }
                    return Ok(json!({
                        "task_attempt_id": task_attempt_id,
                        "run_id": run_id,
                        "evidence_sha256": evidence_sha,
                        "classification": classification,
                        "idempotent_replay": true,
                    }));
                }
                // Run must be admitted (not terminal) for new attempts.
                let status: String = tx
                    .query_row(
                        "SELECT status FROM rwe_runs WHERE run_id=?1",
                        params![run_id],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if status != "admitted" {
                    return Err(format!(
                        "cannot persist task attempt on run status {status}"
                    ));
                }
                tx.execute(
                    "INSERT INTO rwe_task_attempts (
                        task_attempt_id, run_id, task_id, definition_sha256, classification,
                        evidence_json, evidence_sha256, created_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![
                        task_attempt_id,
                        run_id,
                        task_id,
                        definition_sha256,
                        classification,
                        evidence_s,
                        evidence_sha,
                        now
                    ],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(json!({
                    "task_attempt_id": task_attempt_id,
                    "run_id": run_id,
                    "evidence_sha256": evidence_sha,
                    "classification": classification,
                    "idempotent_replay": false,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                if let Some(row) = tx
                    .query_opt(
                        "SELECT evidence_sha256, classification FROM rwe_task_attempts WHERE task_attempt_id=$1 FOR UPDATE",
                        &[&task_attempt_id],
                    )
                    .map_err(|e| e.to_string())?
                {
                    let existing_sha: String = row.get(0);
                    let existing_class: String = row.get(1);
                    if existing_sha != evidence_sha || existing_class != classification {
                        return Err("conflicting RWE task-attempt evidence".into());
                    }
                    return Ok(json!({
                        "task_attempt_id": task_attempt_id,
                        "run_id": run_id,
                        "evidence_sha256": evidence_sha,
                        "classification": classification,
                        "idempotent_replay": true,
                    }));
                }
                let status: String = tx
                    .query_one(
                        "SELECT status FROM rwe_runs WHERE run_id=$1 FOR UPDATE",
                        &[&run_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                if status != "admitted" {
                    return Err(format!(
                        "cannot persist task attempt on run status {status}"
                    ));
                }
                tx.execute(
                    "INSERT INTO rwe_task_attempts (
                        task_attempt_id, run_id, task_id, definition_sha256, classification,
                        evidence_json, evidence_sha256, created_at
                     ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
                    &[
                        &task_attempt_id,
                        &run_id,
                        &task_id,
                        &definition_sha256,
                        &classification,
                        &evidence_s,
                        &evidence_sha,
                        &now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(json!({
                    "task_attempt_id": task_attempt_id,
                    "run_id": run_id,
                    "evidence_sha256": evidence_sha,
                    "classification": classification,
                    "idempotent_replay": false,
                }))
            }),
        }
    }

    /// Terminalize under current lease; exact receipt replay allowed; late write rejected.
    pub fn complete_rwe_run(
        &self,
        run_id: &str,
        lease_token: &str,
        status: &str,
        evidence: &Value,
        evidence_sha256: &str,
    ) -> Result<Value, String> {
        if !matches!(
            status,
            "fixture_complete" | "succeeded" | "failed" | "cancelled" | "outcome_unknown"
        ) {
            return Err(format!("invalid RWE terminal status {status}"));
        }
        let now = self.now();
        let evidence_sorted = sort_value(evidence);
        let recomputed = sha256_hex(canonical_json(&evidence_sorted)?.as_bytes());
        if recomputed != evidence_sha256 {
            return Err("evidence_sha256 mismatch vs evidence body".into());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let existing = load_rwe_run_sqlite(&tx, run_id)?;
                let cur_status = existing
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if cur_status != "admitted" {
                    // Terminal exact replay or late-write reject.
                    if cur_status == status
                        && existing.get("evidence_sha256").and_then(Value::as_str)
                            == Some(evidence_sha256)
                    {
                        let mut row = existing;
                        if let Value::Object(ref mut m) = row {
                            m.insert("idempotent_replay".into(), json!(true));
                        }
                        return Ok(row);
                    }
                    return Err("late RWE terminal write rejected".into());
                }
                let admit = existing
                    .get("evidence_json")
                    .cloned()
                    .unwrap_or(Value::Null);
                let expected_lease = admit
                    .pointer("/admit_state/lease_token")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if expected_lease != lease_token {
                    return Err("RWE lease_token mismatch".into());
                }
                // Preserve admit metadata flags under terminal evidence.
                let mut terminal = evidence_sorted.clone();
                if let Value::Object(ref mut m) = terminal {
                    m.insert("admit_state".into(), admit.get("admit_state").cloned().unwrap_or(Value::Null));
                    m.entry("live_baseline_sealed".to_string())
                        .or_insert(json!(false));
                    m.entry("provider_free_fixture_completion".to_string())
                        .or_insert(json!(status == "fixture_complete"));
                    m.entry("live_provider_request".to_string())
                        .or_insert(json!(false));
                }
                let terminal_s = canonical_json(&terminal)?;
                let terminal_sha = sha256_hex(terminal_s.as_bytes());
                tx.execute(
                    "UPDATE rwe_runs SET status=?1, evidence_json=?2, evidence_sha256=?3, updated_at=?4 WHERE run_id=?5 AND status='admitted'",
                    params![status, terminal_s, terminal_sha, now, run_id],
                )
                .map_err(|e| e.to_string())?;
                let row = load_rwe_run_sqlite(&tx, run_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let existing = load_rwe_run_pg(&mut tx, run_id)?;
                let cur_status = existing
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if cur_status != "admitted" {
                    if cur_status == status
                        && existing.get("evidence_sha256").and_then(Value::as_str)
                            == Some(evidence_sha256)
                    {
                        let mut row = existing;
                        if let Value::Object(ref mut m) = row {
                            m.insert("idempotent_replay".into(), json!(true));
                        }
                        return Ok(row);
                    }
                    return Err("late RWE terminal write rejected".into());
                }
                let admit = existing
                    .get("evidence_json")
                    .cloned()
                    .unwrap_or(Value::Null);
                let expected_lease = admit
                    .pointer("/admit_state/lease_token")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if expected_lease != lease_token {
                    return Err("RWE lease_token mismatch".into());
                }
                let mut terminal = evidence_sorted.clone();
                if let Value::Object(ref mut m) = terminal {
                    m.insert("admit_state".into(), admit.get("admit_state").cloned().unwrap_or(Value::Null));
                    m.entry("live_baseline_sealed".to_string())
                        .or_insert(json!(false));
                    m.entry("provider_free_fixture_completion".to_string())
                        .or_insert(json!(status == "fixture_complete"));
                    m.entry("live_provider_request".to_string())
                        .or_insert(json!(false));
                }
                let terminal_s = canonical_json(&terminal)?;
                let terminal_sha = sha256_hex(terminal_s.as_bytes());
                let updated = tx
                    .execute(
                        "UPDATE rwe_runs SET status=$1, evidence_json=$2, evidence_sha256=$3, updated_at=$4 WHERE run_id=$5 AND status='admitted'",
                        &[&status, &terminal_s, &terminal_sha, &now, &run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("late RWE terminal write rejected".into());
                }
                let row = load_rwe_run_pg(&mut tx, run_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }
}

fn exact_run_replay_or_conflict(existing: &Value, run_body_sha256: &str) -> Result<Value, String> {
    let existing_sha = existing
        .pointer("/evidence_json/admit_state/run_body_sha256")
        .and_then(Value::as_str)
        .or_else(|| {
            existing
                .pointer("/evidence_json/run_body_sha256")
                .and_then(Value::as_str)
        });
    if let Some(sha) = existing_sha {
        if sha != run_body_sha256 {
            return Err("conflicting RWE run body reuse".into());
        }
    } else if existing.get("status").and_then(Value::as_str) != Some("admitted")
        && existing.get("status").and_then(Value::as_str) != Some("fixture_complete")
    {
        // Terminal without stored admit hash: only exact evidence identity allowed via complete.
        return Err("conflicting RWE run reuse without admit body identity".into());
    }
    let mut row = existing.clone();
    if let Value::Object(ref mut m) = row {
        m.insert("idempotent_replay".into(), json!(true));
        if let Some(lease) = existing
            .pointer("/evidence_json/admit_state/lease_token")
            .cloned()
        {
            m.insert("lease_token".into(), lease);
        }
    }
    Ok(row)
}

fn validate_rwe_auth_for_admit(
    auth: &Value,
    principal: &AuthenticatedPrincipal,
    run_body: &Value,
    allow_fixture: bool,
    now: &str,
) -> Result<(), String> {
    if auth.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
        return Err("RWE authorization tenant mismatch".into());
    }
    if auth.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id()) {
        return Err("RWE authorization principal mismatch".into());
    }
    if auth.get("status").and_then(Value::as_str) != Some("active") {
        return Err("RWE authorization not active".into());
    }
    if let Some(exp) = auth.get("expires_at").and_then(Value::as_str) {
        if exp < now {
            return Err("RWE authorization expired".into());
        }
    } else {
        return Err("RWE authorization missing expires_at".into());
    }
    if auth.get("revoked_at").and_then(Value::as_str).is_some() {
        return Err("RWE authorization revoked".into());
    }
    let fixture_only = auth
        .get("fixture_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if allow_fixture {
        if !fixture_only {
            return Err("fixture runner requires fixture_only authorization".into());
        }
        if !matches!(
            principal.principal_kind(),
            super::PrincipalKind::FixturePrincipal
        ) {
            return Err("fixture admit requires fixture principal".into());
        }
    } else {
        if fixture_only {
            return Err("fixture_only authorization cannot admit live RWE".into());
        }
        if !principal.may_authorize_production_live_start() {
            return Err("principal cannot admit live RWE".into());
        }
    }
    let body = auth.get("body_json").cloned().unwrap_or(Value::Null);
    // Complete envelope vs run body.
    for field in [
        "corpus_sha256",
        "target_repo",
        "target_main_sha",
        "executor_identity",
        "model_identity",
        "max_total_provider_requests",
        "max_total_tokens",
        "max_wall_time_ms",
        "golden_path_terminal_evidence_id",
        "draft_pr_only",
    ] {
        let expected = body
            .get(field)
            .cloned()
            .or_else(|| auth.get(field).cloned());
        let observed = run_body.get(field).cloned();
        match (expected, observed) {
            (Some(e), Some(o)) if e == o => {}
            (Some(_), Some(_)) => {
                return Err(format!("run body {field} mismatch vs RWE authorization"));
            }
            (Some(_), None) => {
                return Err(format!("run body missing required field {field}"));
            }
            _ => {}
        }
    }
    if let Some(tasks) = body.get("task_ids") {
        if run_body.get("task_ids").is_some() && run_body.get("task_ids") != Some(tasks) {
            return Err("run body task_ids mismatch vs authorization".into());
        }
    }
    if run_body.get("corpus_sha256").and_then(Value::as_str)
        != auth.get("corpus_sha256").and_then(Value::as_str)
    {
        return Err("corpus_sha256 mismatch".into());
    }
    Ok(())
}

fn load_rwe_auth_sqlite(conn: &rusqlite::Connection, id: &str) -> Result<Value, String> {
    conn.query_row(
        "SELECT authorization_id, tenant_id, principal_id, principal_kind, corpus_sha256,
                body_sha256, body_json, fixture_only, status, created_at, updated_at,
                expires_at, consumed_at, consumed_by_run_id, revoked_at
         FROM rwe_run_authorizations WHERE authorization_id=?1",
        params![id],
        |row| {
            let body_s: String = row.get(6)?;
            Ok(json!({
                "schema_version": "rwe_run_authorization.v1",
                "authorization_id": row.get::<_, String>(0)?,
                "tenant_id": row.get::<_, String>(1)?,
                "principal_id": row.get::<_, String>(2)?,
                "principal_kind": row.get::<_, String>(3)?,
                "corpus_sha256": row.get::<_, String>(4)?,
                "body_sha256": row.get::<_, String>(5)?,
                "body_json": serde_json::from_str::<Value>(&body_s).unwrap_or(Value::Null),
                "fixture_only": row.get::<_, i64>(7)? != 0,
                "status": row.get::<_, String>(8)?,
                "created_at": row.get::<_, String>(9)?,
                "updated_at": row.get::<_, String>(10)?,
                "expires_at": row.get::<_, String>(11)?,
                "consumed_at": row.get::<_, Option<String>>(12)?,
                "consumed_by_run_id": row.get::<_, Option<String>>(13)?,
                "revoked_at": row.get::<_, Option<String>>(14)?,
            }))
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn load_rwe_auth_pg(client: &mut impl postgres::GenericClient, id: &str) -> Result<Value, String> {
    let row = client
        .query_one(
            "SELECT authorization_id, tenant_id, principal_id, principal_kind, corpus_sha256,
                    body_sha256, body_json, fixture_only, status, created_at, updated_at,
                    expires_at, consumed_at, consumed_by_run_id, revoked_at
             FROM rwe_run_authorizations WHERE authorization_id=$1",
            &[&id],
        )
        .map_err(|e| e.to_string())?;
    let body_s: String = row.get(6);
    let fixture: i32 = row.get(7);
    Ok(json!({
        "schema_version": "rwe_run_authorization.v1",
        "authorization_id": row.get::<_, String>(0),
        "tenant_id": row.get::<_, String>(1),
        "principal_id": row.get::<_, String>(2),
        "principal_kind": row.get::<_, String>(3),
        "corpus_sha256": row.get::<_, String>(4),
        "body_sha256": row.get::<_, String>(5),
        "body_json": serde_json::from_str::<Value>(&body_s).unwrap_or(Value::Null),
        "fixture_only": fixture != 0,
        "status": row.get::<_, String>(8),
        "created_at": row.get::<_, String>(9),
        "updated_at": row.get::<_, String>(10),
        "expires_at": row.get::<_, String>(11),
        "consumed_at": row.get::<_, Option<String>>(12),
        "consumed_by_run_id": row.get::<_, Option<String>>(13),
        "revoked_at": row.get::<_, Option<String>>(14),
    }))
}

fn load_rwe_run_sqlite(conn: &rusqlite::Connection, run_id: &str) -> Result<Value, String> {
    conn.query_row(
        "SELECT run_id, tenant_id, authorization_id, corpus_sha256, principal_id,
                status, evidence_json, evidence_sha256, created_at, updated_at
         FROM rwe_runs WHERE run_id=?1",
        params![run_id],
        |row| {
            let ev: Option<String> = row.get(6)?;
            let evidence: Value = ev
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            let mut out = json!({
                "schema_version": "rwe_run.v1",
                "run_id": row.get::<_, String>(0)?,
                "tenant_id": row.get::<_, String>(1)?,
                "authorization_id": row.get::<_, String>(2)?,
                "corpus_sha256": row.get::<_, String>(3)?,
                "principal_id": row.get::<_, String>(4)?,
                "status": row.get::<_, String>(5)?,
                "evidence_json": evidence.clone(),
                "evidence_sha256": row.get::<_, Option<String>>(7)?,
                "created_at": row.get::<_, String>(8)?,
                "updated_at": row.get::<_, String>(9)?,
                "idempotent_replay": false,
            });
            if let (Value::Object(ref mut m), Value::Object(ev_map)) = (&mut out, &evidence) {
                for key in [
                    "live_baseline_sealed",
                    "provider_free_fixture_completion",
                    "live_provider_request",
                ] {
                    if let Some(v) = ev_map.get(key) {
                        m.insert(key.into(), v.clone());
                    }
                }
                if let Some(admit) = ev_map.get("admit_state") {
                    if let Some(lease) = admit.get("lease_token") {
                        m.insert("lease_token".into(), lease.clone());
                    }
                }
            }
            Ok(out)
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn load_rwe_run_pg(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
) -> Result<Value, String> {
    let row = client
        .query_one(
            "SELECT run_id, tenant_id, authorization_id, corpus_sha256, principal_id,
                    status, evidence_json, evidence_sha256, created_at, updated_at
             FROM rwe_runs WHERE run_id=$1",
            &[&run_id],
        )
        .map_err(|e| e.to_string())?;
    let ev: Option<String> = row.get(6);
    let evidence: Value = ev
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);
    let mut out = json!({
        "schema_version": "rwe_run.v1",
        "run_id": row.get::<_, String>(0),
        "tenant_id": row.get::<_, String>(1),
        "authorization_id": row.get::<_, String>(2),
        "corpus_sha256": row.get::<_, String>(3),
        "principal_id": row.get::<_, String>(4),
        "status": row.get::<_, String>(5),
        "evidence_json": evidence.clone(),
        "evidence_sha256": row.get::<_, Option<String>>(7),
        "created_at": row.get::<_, String>(8),
        "updated_at": row.get::<_, String>(9),
        "idempotent_replay": false,
    });
    if let Value::Object(ref mut m) = out {
        if let Some(obj) = evidence.as_object() {
            for key in [
                "live_baseline_sealed",
                "provider_free_fixture_completion",
                "live_provider_request",
            ] {
                if let Some(v) = obj.get(key) {
                    m.insert(key.into(), v.clone());
                }
            }
            if let Some(admit) = obj.get("admit_state") {
                if let Some(lease) = admit.get("lease_token") {
                    m.insert("lease_token".into(), lease.clone());
                }
            }
        }
    }
    Ok(out)
}
