//! Store-owned managed-acceptance decision, risk acknowledgement, spend authorization,
//! and exactly-once attempt admission.
//!
//! Production principals are derived only from verified `api_key_metadata` records.
//! Free-form strings never create authority. Fixture principals are test-only and cannot
//! authorize production live starts.

#![allow(unused_imports)]

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
use crate::provider::managed_deepseek::DEEPSEEK_PROVIDER_KIND;
use crate::provider::redaction::contains_sensitive_patterns;

mod contracts;

pub use contracts::*;
pub(in crate::storage::local_product_store) use contracts::{canonical_json, sha256_hex};

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
        self.complete_delegated_attempt_with_cost_evidence(
            delegation_id,
            attempt_id,
            lease_token,
            status,
            receipt,
            Some(realized_cost_usd),
            false,
        )
    }

    fn complete_delegated_attempt_with_cost_evidence(
        &self,
        delegation_id: &str,
        attempt_id: &str,
        lease_token: &str,
        status: &str,
        receipt: &Value,
        realized_cost_usd: Option<f64>,
        cost_unavailable: bool,
    ) -> Result<Value, String> {
        validate_attempt_terminal_status(status)?;
        if (!cost_unavailable && realized_cost_usd.is_none())
            || (cost_unavailable && realized_cost_usd.is_some())
            || realized_cost_usd.is_some_and(|cost| !cost.is_finite() || cost < 0.0)
        {
            return Err("realized delegated cost is invalid".into());
        }
        let stored_cost_usd = realized_cost_usd.unwrap_or(0.0);
        let mut receipt = sort_value(receipt);
        let receipt_object = receipt
            .as_object_mut()
            .ok_or("delegated terminal receipt must be an object")?;
        if receipt_object.contains_key("terminal_class")
            || receipt_object.contains_key("spend_authorization_state")
            || receipt_object.contains_key("attempt_lease_state")
            || receipt_object.contains_key("delegation_state")
            || receipt_object.contains_key("realized_cost_usd")
            || receipt_object.contains_key("cost_unavailable")
            || receipt_object.contains_key("cost_evidence")
        {
            return Err("delegated terminal receipt contains store-owned state fields".into());
        }
        receipt_object.insert("terminal_class".into(), json!(status));
        receipt_object.insert("spend_authorization_state".into(), json!("expired"));
        receipt_object.insert("attempt_lease_state".into(), json!("closed"));
        receipt_object.insert("delegation_state".into(), json!("expired"));
        receipt_object.insert(
            "realized_cost_usd".into(),
            realized_cost_usd.map_or(Value::Null, |cost| json!(cost)),
        );
        receipt_object.insert("cost_unavailable".into(), json!(cost_unavailable));
        receipt_object.insert(
            "cost_evidence".into(),
            json!(if status == "outcome_unknown" {
                "conservative_reservation"
            } else if cost_unavailable {
                "unavailable"
            } else {
                "reconciled"
            }),
        );
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
                    let existing_cost_unavailable = serde_json::from_str::<Value>(&existing)
                        .ok()
                        .and_then(|value| value.get("cost_unavailable").and_then(Value::as_bool))
                        .unwrap_or(false);
                    if existing == receipt_json
                        && row.3 == "expired"
                        && (row.5 - stored_cost_usd).abs() <= 1e-12
                        && existing_cost_unavailable == cost_unavailable
                    {
                        return Ok(json!({"status":"closed","terminal_class":status,"spend_authorization_state":"expired","attempt_lease_state":"closed","delegation_state":"expired","receipt_sha256":receipt_sha256,"replayed":true}));
                    }
                    return Err("late or conflicting delegated terminal write".into());
                }
                if stored_cost_usd > row.4 {
                    return Err("delegated cumulative cost ceiling exceeded".into());
                }
                tx.execute(
                    "UPDATE managed_acceptance_delegations SET status=CASE WHEN status='revoked' THEN 'revoked' ELSE 'expired' END, total_cost_usd=?1, spend_status=CASE WHEN status='revoked' THEN 'revoked' ELSE 'expired' END, attempt_status='closed', terminal_receipt_json=?2, terminal_at=?3, updated_at=?3 WHERE delegation_id=?4 AND attempt_id=?5 AND attempt_lease_token=?6",
                    params![stored_cost_usd, receipt_json, now, delegation_id, attempt_id, lease_token],
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
                    let existing_cost_unavailable = serde_json::from_str::<Value>(&existing)
                        .ok()
                        .and_then(|value| value.get("cost_unavailable").and_then(Value::as_bool))
                        .unwrap_or(false);
                    if existing == receipt_json
                        && stored_status == "expired"
                        && ((stored_cost - stored_cost_usd).abs() <= 1e-12)
                        && existing_cost_unavailable == cost_unavailable
                    {
                        tx.commit().map_err(|e| e.to_string())?;
                        return Ok(json!({"status":"closed","terminal_class":status,"spend_authorization_state":"expired","attempt_lease_state":"closed","delegation_state":"expired","receipt_sha256":receipt_sha256,"replayed":true}));
                    }
                    return Err("late or conflicting delegated terminal write".into());
                }
                if stored_cost_usd > max_cost { return Err("delegated cumulative cost ceiling exceeded".into()); }
                tx.execute("UPDATE managed_acceptance_delegations SET status=CASE WHEN status='revoked' THEN 'revoked' ELSE 'expired' END, total_cost_usd=$1, spend_status=CASE WHEN status='revoked' THEN 'revoked' ELSE 'expired' END, attempt_status='closed', terminal_receipt_json=$2, terminal_at=$3, updated_at=$3 WHERE delegation_id=$4 AND attempt_id=$5 AND attempt_lease_token=$6", &[&stored_cost_usd, &receipt_json, &now, &delegation_id, &attempt_id, &lease_token]).map_err(|e| e.to_string())?;
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
                if terminal_evidence
                    .pointer("/output/receipt_id")
                    .is_some_and(|value| !value.is_null())
                {
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
        let provider_binding = manifest
            .get("provider_execution_binding")
            .map(crate::rwe::campaign_package::FrozenProviderExecutionBinding::from_json)
            .transpose()?
            .unwrap_or_else(crate::rwe::campaign_package::canonical_deepseek_provider_binding);
        let cost_unavailable = provider_binding.cost_unavailable;
        let realized_cost_usd = match confirmation.get("realized_cost_usd") {
            Some(Value::Null) if cost_unavailable => None,
            Some(Value::Number(_)) if cost_unavailable => {
                return Err("delegated artifact confirmation cost must remain unavailable".into())
            }
            Some(value) => Some(
                value
                    .as_f64()
                    .ok_or("delegated artifact confirmation realized cost is invalid")?,
            ),
            None => return Err("delegated artifact confirmation realized cost is missing".into()),
        };
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
        let terminal = self.complete_delegated_attempt_with_cost_evidence(
            delegation_id,
            attempt_id,
            &lease_token,
            "succeeded",
            &receipt,
            realized_cost_usd,
            cost_unavailable,
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
        let provider_binding = manifest
            .get("provider_execution_binding")
            .map(crate::rwe::campaign_package::FrozenProviderExecutionBinding::from_json)
            .transpose()?
            .unwrap_or_else(crate::rwe::campaign_package::canonical_deepseek_provider_binding);
        let cost_unavailable = provider_binding.cost_unavailable;
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
            let cost = match entry.get("effective_cost_usd") {
                Some(Value::Null) if cost_unavailable => None,
                Some(value) => {
                    let cost = value
                        .as_f64()
                        .ok_or("delegated failure provider journal cost is invalid")?;
                    if !cost.is_finite() || cost < 0.0 {
                        return Err("delegated failure provider journal cost is invalid".into());
                    }
                    Some(cost)
                }
                None => return Err("delegated failure provider journal cost is missing".into()),
            };
            realized_or_reserved_cost_usd += cost.unwrap_or(0.0);
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
        });
        let terminal = self.complete_delegated_attempt_with_cost_evidence(
            &delegation_id,
            &attempt_id,
            &lease_token,
            terminal_class,
            &receipt,
            if cost_unavailable {
                None
            } else {
                Some(realized_or_reserved_cost_usd)
            },
            cost_unavailable,
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

    /// Read-only identity projection for a completed delegated attempt. The
    /// manifest remains owned by Store; callers receive only the hash-bound
    /// provider/model/target fields needed to validate a frozen evidence
    /// consumer, never prompts, outputs, lease tokens, or raw credentials.
    pub(crate) fn project_delegated_execution_identity_for_attempt(
        &self,
        attempt_id: &str,
    ) -> Result<Value, String> {
        let manifest_json: Option<String> = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                connection
                    .query_row(
                        "SELECT manifest_json FROM managed_acceptance_delegations
                         WHERE attempt_id=?1",
                        params![attempt_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT manifest_json FROM managed_acceptance_delegations
                         WHERE attempt_id=$1",
                        &[&attempt_id],
                    )
                    .map(|row| row.map(|row| row.get(0)))
                    .map_err(|error| error.to_string())
            })?,
        };
        let manifest: Value =
            serde_json::from_str(&manifest_json.ok_or("delegated execution manifest is missing")?)
                .map_err(|error| format!("delegated execution manifest is invalid: {error}"))?;
        if manifest
            .pointer("/execution/attempt_id")
            .and_then(Value::as_str)
            != Some(attempt_id)
        {
            return Err("delegated execution manifest attempt identity is mismatched".into());
        }
        let computed_manifest_sha256 = compute_attempt_manifest_sha256(&manifest)?;
        if manifest.get("manifest_sha256").and_then(Value::as_str)
            != Some(computed_manifest_sha256.as_str())
        {
            return Err("delegated execution manifest hash is invalid".into());
        }
        Ok(json!({
            "manifest_sha256": manifest.get("manifest_sha256"),
            "campaign_package_id": manifest.get("campaign_package_id"),
            "provider_execution_binding": manifest.get("provider_execution_binding"),
            "models": manifest.get("models"),
            "target": manifest.get("target"),
            "execution": manifest.get("execution").map(|execution| json!({
                "product_task_id": execution.get("product_task_id"),
                "attempt_id": execution.get("attempt_id"),
                "workflow_id": execution.get("workflow_id"),
            })),
            "output": manifest.get("output"),
        }))
    }
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

    /// Delegated ProductTask attempts are owned by the delegation row, not by
    /// the legacy managed-acceptance attempt table. Keep this lookup separate
    /// from `current_attempt_lease_token`: the latter remains for the legacy
    /// Codex CLI acceptance path.
    fn current_delegated_attempt_lease_token(&self, attempt_id: &str) -> Result<String, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT attempt_lease_token FROM managed_acceptance_delegations WHERE attempt_id=?1 AND attempt_status='admitted'",
                    params![attempt_id],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|e| e.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT attempt_lease_token FROM managed_acceptance_delegations WHERE attempt_id=$1 AND attempt_status='admitted'",
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
    manifest: &Value,
) -> Result<(), String> {
    let expected_provider_identity = manifest
        .pointer("/provider/provider_identity")
        .and_then(Value::as_str)
        .ok_or("managed workspace manifest provider identity is missing")?;
    let expected_provider_kind = manifest
        .pointer("/provider/provider_kind")
        .and_then(Value::as_str)
        .ok_or("managed workspace manifest provider kind is missing")?;
    let expected_model = manifest
        .pointer("/provider/admitted_model")
        .and_then(Value::as_str)
        .ok_or("managed workspace manifest admitted model is missing")?;
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
        || entry.get("provider_identity").and_then(Value::as_str)
            != Some(expected_provider_identity)
        || entry.get("provider_kind").and_then(Value::as_str) != Some(expected_provider_kind)
        || entry.get("requested_model").and_then(Value::as_str) != Some(expected_model)
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
    validate_workspace_provider_journal(journal_json, binding, scheduler, &manifest)
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

    /// Accept the exact action object bare, inside one markdown fence, or
    /// extracted from surrounding conversational prose when exactly one
    /// unambiguous workspace action is present. The downstream owner still
    /// validates the parsed action against every persisted boundary.
    pub(crate) fn parse_managed_workspace_action(
        model_output: &str,
    ) -> Result<Value, serde_json::Error> {
        let trimmed = model_output.trim();

        fn strip_code_fence(s: &str) -> Option<&str> {
            let s = s.trim();
            if s.starts_with("```") && s.ends_with("```") && s.len() >= 6 {
                let inner = &s[3..s.len() - 3];
                if let Some(first_newline) = inner.find('\n') {
                    let first_line = inner[..first_newline].trim();
                    if first_line.is_empty() || first_line.eq_ignore_ascii_case("json") {
                        Some(inner[first_newline + 1..].trim())
                    } else {
                        None
                    }
                } else {
                    let inner_trimmed = inner.trim();
                    if let Some(rest) = inner_trimmed
                        .strip_prefix("json")
                        .or_else(|| inner_trimmed.strip_prefix("JSON"))
                    {
                        Some(rest.trim())
                    } else {
                        Some(inner_trimmed)
                    }
                }
            } else {
                None
            }
        }

        // 1. Direct parse of entire trimmed input
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            return Ok(value);
        }

        // 2. Direct parse of single outer markdown fence
        if let Some(inner) = strip_code_fence(trimmed) {
            return serde_json::from_str(inner);
        }

        // 3. Extract from markdown fences embedded in conversational text
        let mut fence_candidates = Vec::new();
        let mut search_idx = 0;
        while let Some(start_offset) = trimmed[search_idx..].find("```") {
            let start = search_idx + start_offset;
            let after_fence_start = start + 3;
            if let Some(end_offset) = trimmed[after_fence_start..].find("```") {
                let fence_end = after_fence_start + end_offset + 3;
                let block = &trimmed[start..fence_end];
                if let Some(inner) = strip_code_fence(block) {
                    if let Ok(val) = serde_json::from_str::<Value>(inner) {
                        fence_candidates.push(val);
                    }
                }
                search_idx = fence_end;
            } else {
                break;
            }
        }

        if fence_candidates.len() == 1 {
            return Ok(fence_candidates.remove(0));
        } else if fence_candidates.len() > 1 {
            return Err(serde::de::Error::custom(
                "model output contained multiple markdown fenced workspace actions",
            ));
        }

        // 4. Search for an embedded JSON object with schema_version managed_workspace_action.v1
        let mut json_candidates = Vec::new();
        let bytes = trimmed.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'{' {
                let mut depth = 0;
                let mut in_str = false;
                let mut escape = false;
                let mut j = i;
                while j < bytes.len() {
                    let b = bytes[j];
                    if in_str {
                        if escape {
                            escape = false;
                        } else if b == b'\\' {
                            escape = true;
                        } else if b == b'"' {
                            in_str = false;
                        }
                    } else if b == b'"' {
                        in_str = true;
                    } else if b == b'{' {
                        depth += 1;
                    } else if b == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            let span = &trimmed[i..=j];
                            if let Ok(val) = serde_json::from_str::<Value>(span) {
                                if val.get("schema_version").and_then(Value::as_str)
                                    == Some("managed_workspace_action.v1")
                                {
                                    json_candidates.push(val);
                                }
                            }
                            break;
                        }
                    }
                    j += 1;
                }
            }
            i += 1;
        }

        if json_candidates.len() == 1 {
            return Ok(json_candidates.remove(0));
        } else if json_candidates.len() > 1 {
            return Err(serde::de::Error::custom(
                "model output contained multiple JSON workspace actions",
            ));
        }

        serde_json::from_str(trimmed)
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

    /// Mint the opaque Codex gateway start capability from the current
    /// delegated attempt lease.  The raw lease token never leaves this Store
    /// owner; callers receive only a capability whose execution identity is
    /// derived from the exact persisted attempt/lease binding.
    pub(crate) fn codex_gateway_start_permit_for_delegated_attempt(
        &self,
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    ) -> Result<crate::cli::codex_budget_authority::CodexGatewayStartPermit, String> {
        // `current_delegated_provider_authority` below admits only the three
        // provider positions in the frozen four-stage route (planner,
        // implementer, and reviewer); the deterministic verifier is rejected
        // by `delegated_execution_contract`. Do not narrow this capability to
        // the implementer: the H0 route makes one managed provider call for
        // each of those three roles.
        self.current_delegated_provider_authority(binding)?
            .ok_or("delegated provider authority is missing")?;
        let lease_token = self.current_delegated_attempt_lease_token(&binding.attempt_id)?;
        if lease_token.trim().is_empty()
            || crate::provider::managed_deepseek::managed_attempt_lease_id(&lease_token)
                != binding.attempt_lease_id
        {
            return Err("delegated attempt lease is stale or mismatched".into());
        }
        let execution_id = format!(
            "codex-attempt-{}",
            &sha256_hex(
                format!(
                    "codex-gateway-execution.v2|{}|{}|{}|{}|{}",
                    binding.product_task_id,
                    binding.workflow_id,
                    binding.node_id,
                    binding.attempt_id,
                    binding.attempt_lease_id
                )
                .as_bytes()
            )[..32]
        );
        Ok(
            crate::cli::codex_budget_authority::CodexGatewayStartPermit::managed_store_lease(
                &execution_id,
                &lease_token,
            ),
        )
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
    fn durable_token_reservation(
        &self,
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
    ) -> Result<u64, String> {
        let journal_json: String = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT provider_request_journal_json
                     FROM managed_acceptance_delegations
                     WHERE attempt_id=?1 AND spend_authorization_id=?2
                       AND attempt_status='admitted'",
                    params![binding.attempt_id, binding.spend_authorization_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations
                         WHERE attempt_id=$1 AND spend_authorization_id=$2
                           AND attempt_status='admitted'",
                        &[&binding.attempt_id, &binding.spend_authorization_id],
                    )
                    .map(|row| row.get(0))
                    .map_err(|error| error.to_string())
            })?,
        };
        let journal: Vec<Value> = serde_json::from_str(&journal_json)
            .map_err(|error| format!("durable provider request journal is invalid: {error}"))?;
        journal.iter().try_fold(0_u64, |total, entry| {
            let tokens = entry
                .get("effective_tokens")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    "durable provider journal token reservation is missing".to_string()
                })?;
            total
                .checked_add(tokens)
                .ok_or_else(|| "durable provider journal token reservation overflow".to_string())
        })
    }

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
mod tests;
