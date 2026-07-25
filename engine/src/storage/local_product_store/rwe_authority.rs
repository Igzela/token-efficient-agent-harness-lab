//! Store-owned RWE run authorization, admission, and evidence persistence.

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

impl LocalProductStore {
    pub fn upsert_rwe_run_authorization(
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
        if corpus_sha256.len() != 64 || body_sha256.len() != 64 {
            return Err("sha256 fields must be 64 hex chars".into());
        }
        let now = self.now();
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
                    "rwe.authorization_upsert",
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

    pub fn admit_rwe_run(
        &self,
        tenant_id: &str,
        run_id: &str,
        authorization_id: &str,
        corpus_sha256: &str,
        principal_id: &str,
    ) -> Result<Value, String> {
        let now = self.now();
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
                    let mut row = load_rwe_run_sqlite(&tx, run_id)?;
                    if let Value::Object(ref mut m) = row {
                        m.insert("idempotent_replay".into(), json!(true));
                    }
                    return Ok(row);
                }
                let auth = load_rwe_auth_sqlite(&tx, authorization_id)?;
                if auth.get("status").and_then(Value::as_str) != Some("active") {
                    return Err("RWE authorization not active".into());
                }
                if auth.get("corpus_sha256").and_then(Value::as_str) != Some(corpus_sha256) {
                    return Err("corpus_sha256 mismatch".into());
                }
                let updated = tx
                    .execute(
                        "UPDATE rwe_run_authorizations SET status='consumed', consumed_at=?1, consumed_by_run_id=?2, updated_at=?1 WHERE authorization_id=?3 AND status='active'",
                        params![now, run_id, authorization_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("RWE authorization already consumed".into());
                }
                tx.execute(
                    "INSERT INTO rwe_runs (
                        run_id, tenant_id, authorization_id, corpus_sha256, principal_id,
                        status, evidence_json, evidence_sha256, created_at, updated_at
                     ) VALUES (?1,?2,?3,?4,?5,'admitted',NULL,NULL,?6,?6)",
                    params![
                        run_id,
                        tenant_id,
                        authorization_id,
                        corpus_sha256,
                        principal_id,
                        now
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    principal_id,
                    "rwe.run_admitted",
                    run_id,
                    &json!({"authorization_id": authorization_id}),
                )?;
                let row = load_rwe_run_sqlite(&tx, run_id)?;
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
                    let mut row = load_rwe_run_pg(&mut tx, run_id)?;
                    if let Value::Object(ref mut m) = row {
                        m.insert("idempotent_replay".into(), json!(true));
                    }
                    return Ok(row);
                }
                let auth = load_rwe_auth_pg(&mut tx, authorization_id)?;
                if auth.get("status").and_then(Value::as_str) != Some("active") {
                    return Err("RWE authorization not active".into());
                }
                let updated = tx
                    .execute(
                        "UPDATE rwe_run_authorizations SET status='consumed', consumed_at=$1, consumed_by_run_id=$2, updated_at=$1 WHERE authorization_id=$3 AND status='active'",
                        &[&now, &run_id, &authorization_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("RWE authorization already consumed".into());
                }
                let status = "admitted";
                tx.execute(
                    "INSERT INTO rwe_runs (
                        run_id, tenant_id, authorization_id, corpus_sha256, principal_id,
                        status, evidence_json, evidence_sha256, created_at, updated_at
                     ) VALUES ($1,$2,$3,$4,$5,$6,NULL,NULL,$7,$7)",
                    &[
                        &run_id,
                        &tenant_id,
                        &authorization_id,
                        &corpus_sha256,
                        &principal_id,
                        &status,
                        &now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                let row = load_rwe_run_pg(&mut tx, run_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

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
        let evidence_s = evidence.to_string();
        let evidence_sha = sha256_hex(evidence_s.as_bytes());
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO rwe_task_attempts (
                        task_attempt_id, run_id, task_id, definition_sha256, classification,
                        evidence_json, evidence_sha256, created_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                     ON CONFLICT(task_attempt_id) DO UPDATE SET
                        evidence_json=excluded.evidence_json,
                        evidence_sha256=excluded.evidence_sha256,
                        classification=excluded.classification",
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
                Ok(json!({
                    "task_attempt_id": task_attempt_id,
                    "run_id": run_id,
                    "evidence_sha256": evidence_sha,
                    "classification": classification,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO rwe_task_attempts (
                            task_attempt_id, run_id, task_id, definition_sha256, classification,
                            evidence_json, evidence_sha256, created_at
                         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                         ON CONFLICT(task_attempt_id) DO UPDATE SET
                            evidence_json=excluded.evidence_json,
                            evidence_sha256=excluded.evidence_sha256,
                            classification=excluded.classification",
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
                Ok(json!({
                    "task_attempt_id": task_attempt_id,
                    "run_id": run_id,
                    "evidence_sha256": evidence_sha,
                    "classification": classification,
                }))
            }),
        }
    }

    pub fn complete_rwe_run(
        &self,
        run_id: &str,
        status: &str,
        evidence: &Value,
        evidence_sha256: &str,
    ) -> Result<Value, String> {
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "UPDATE rwe_runs SET status=?1, evidence_json=?2, evidence_sha256=?3, updated_at=?4 WHERE run_id=?5",
                    params![status, evidence.to_string(), evidence_sha256, now, run_id],
                )
                .map_err(|e| e.to_string())?;
                load_rwe_run_sqlite(conn, run_id)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "UPDATE rwe_runs SET status=$1, evidence_json=$2, evidence_sha256=$3, updated_at=$4 WHERE run_id=$5",
                        &[&status, &evidence.to_string(), &evidence_sha256, &now, &run_id],
                    )
                    .map_err(|e| e.to_string())?;
                load_rwe_run_pg(client, run_id)
            }),
        }
    }
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
        }
    }
    Ok(out)
}
