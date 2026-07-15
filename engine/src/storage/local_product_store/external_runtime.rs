use chrono::DateTime;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};

pub const EXTERNAL_RUNTIME_CHECKPOINT_SCHEMA_VERSION: &str = "external_runtime_checkpoint.v1";
pub const EXTERNAL_RUNTIME_INVOCATION_SCHEMA_VERSION: &str = "external_runtime_invocation.v1";
const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_CHECKPOINT_SUMMARY_BYTES: usize = 16 * 1024;
const MAX_RESULT_SUMMARY_BYTES: usize = 32 * 1024;
type ExistingInvocationRow = (String, String, String, Option<String>, Option<String>);

pub const MEMORY_STRATEGIES: [&str; 4] = [
    "full_history",
    "summary_memory",
    "retrieval_memory",
    "durable_state_bounded_recent",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalRuntimeScope {
    pub tenant_id: String,
    pub workspace_id: String,
    pub run_id: String,
    pub node_id: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExternalRuntimeInvocationClaim {
    Claimed {
        invocation_id: String,
        checkpoint_id: String,
        checkpoint: Option<Value>,
        resumed: bool,
    },
    Completed {
        invocation_id: String,
        checkpoint_id: String,
        result_summary: Value,
        artifact_id: Option<String>,
    },
    Busy {
        invocation_id: String,
        updated_at: String,
    },
    Blocked {
        invocation_id: String,
        failure_code: String,
    },
}

impl ExternalRuntimeScope {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("tenant_id", self.tenant_id.as_str()),
            ("workspace_id", self.workspace_id.as_str()),
            ("run_id", self.run_id.as_str()),
            ("node_id", self.node_id.as_str()),
            ("thread_id", self.thread_id.as_str()),
        ] {
            validate_identifier(name, value)?;
        }
        Ok(())
    }

    pub fn checkpoint_id(&self) -> String {
        let digest = sha256_text(&format!(
            "{}\0{}\0{}\0{}\0{}",
            self.tenant_id, self.workspace_id, self.run_id, self.node_id, self.thread_id
        ));
        format!("lgcp-{}", &digest[..32])
    }

    fn invocation_id(&self, idempotency_sha256: &str) -> String {
        let digest = sha256_text(&format!(
            "{}\0{}\0{}\0{}\0{}\0{}",
            self.tenant_id,
            self.workspace_id,
            self.run_id,
            self.node_id,
            self.thread_id,
            idempotency_sha256
        ));
        format!("lginv-{}", &digest[..32])
    }
}

impl LocalProductStore {
    pub fn external_runtime_scope_for_node(
        &self,
        run_id: &str,
        node_id: &str,
        thread_id: &str,
    ) -> Result<ExternalRuntimeScope, String> {
        let run = self
            .get_workflow_run(run_id)?
            .ok_or_else(|| format!("workflow run not found: {run_id}"))?;
        let tenant_id = run
            .get("tenant_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "workflow run is missing tenant_id".to_string())?;
        let workspace_id = run
            .get("workspace_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "workflow run is missing workspace_id".to_string())?;
        let node_exists = run
            .get("nodes")
            .and_then(Value::as_array)
            .is_some_and(|nodes| {
                nodes
                    .iter()
                    .any(|node| node.get("node_id").and_then(Value::as_str) == Some(node_id))
            });
        if !node_exists {
            return Err(format!(
                "workflow node not found in authoritative run: {run_id}/{node_id}"
            ));
        }
        let scope = ExternalRuntimeScope {
            tenant_id: tenant_id.to_string(),
            workspace_id: workspace_id.to_string(),
            run_id: run_id.to_string(),
            node_id: node_id.to_string(),
            thread_id: thread_id.to_string(),
        };
        scope.validate()?;
        Ok(scope)
    }

    pub fn claim_external_runtime_invocation(
        &self,
        scope: &ExternalRuntimeScope,
        idempotency_sha256: &str,
        lease_token: &str,
        stale_after_seconds: i64,
        actor: &str,
    ) -> Result<ExternalRuntimeInvocationClaim, String> {
        scope.validate()?;
        validate_sha256("idempotency_sha256", idempotency_sha256)?;
        validate_identifier("lease_token", lease_token)?;
        validate_identifier("actor", actor)?;
        if !(1..=3600).contains(&stale_after_seconds) {
            return Err("external runtime stale timeout must be between 1 and 3600 seconds".into());
        }
        let invocation_id = scope.invocation_id(idempotency_sha256);
        let checkpoint_id = scope.checkpoint_id();
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    TransactionBehavior::Immediate,
                )
                    .map_err(|error| error.to_string())?;
                let existing: Option<ExistingInvocationRow> = tx
                    .query_row(
                        "SELECT invocation_id, status, updated_at, result_summary_json, artifact_id
                         FROM external_runtime_invocations
                         WHERE tenant_id=?1 AND workspace_id=?2 AND run_id=?3 AND node_id=?4
                           AND idempotency_sha256=?5",
                        params![
                            scope.tenant_id,
                            scope.workspace_id,
                            scope.run_id,
                            scope.node_id,
                            idempotency_sha256
                        ],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;

                let outcome = match existing {
                    Some((id, status, _updated_at, result_json, artifact_id))
                        if status == "completed" =>
                    {
                        let result_summary = parse_json_summary(
                            result_json.as_deref().unwrap_or("null"),
                            "stored external runtime result",
                        )?;
                        ExternalRuntimeInvocationClaim::Completed {
                            invocation_id: id,
                            checkpoint_id: checkpoint_id.clone(),
                            result_summary,
                            artifact_id,
                        }
                    }
                    Some((id, status, _updated_at, _, _)) if status == "blocked" => {
                        let failure_code: String = tx
                            .query_row(
                                "SELECT COALESCE(failure_code, 'external_runtime_blocked')
                                 FROM external_runtime_invocations WHERE invocation_id=?1",
                                params![id],
                                |row| row.get(0),
                            )
                            .map_err(|error| error.to_string())?;
                        ExternalRuntimeInvocationClaim::Blocked {
                            invocation_id: id,
                            failure_code,
                        }
                    }
                    Some((id, status, updated_at, _, _))
                        if status == "started"
                            && !timestamp_is_stale(&updated_at, &now, stale_after_seconds) =>
                    {
                        ExternalRuntimeInvocationClaim::Busy {
                            invocation_id: id,
                            updated_at,
                        }
                    }
                    Some((id, _, _, _, _)) => {
                        let changed = tx
                            .execute(
                                "UPDATE external_runtime_invocations
                                 SET status='started', lease_token=?1, failure_code=NULL,
                                     result_summary_json=NULL, artifact_id=NULL, updated_at=?2,
                                     completed_at=NULL
                                 WHERE invocation_id=?3",
                                params![lease_token, now, id],
                            )
                            .map_err(|error| error.to_string())?;
                        if changed != 1 {
                            return Err("external runtime invocation reclaim lost".into());
                        }
                        ExternalRuntimeInvocationClaim::Claimed {
                            invocation_id: id,
                            checkpoint_id: checkpoint_id.clone(),
                            checkpoint: sqlite_checkpoint(&tx, scope)?,
                            resumed: true,
                        }
                    }
                    None => {
                        tx.execute(
                            "INSERT INTO external_runtime_invocations
                             (invocation_id,tenant_id,workspace_id,run_id,node_id,thread_id,
                              idempotency_sha256,checkpoint_id,lease_token,status,created_at,updated_at)
                             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'started',?10,?10)",
                            params![
                                invocation_id,
                                scope.tenant_id,
                                scope.workspace_id,
                                scope.run_id,
                                scope.node_id,
                                scope.thread_id,
                                idempotency_sha256,
                                checkpoint_id,
                                lease_token,
                                now
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                        ExternalRuntimeInvocationClaim::Claimed {
                            invocation_id: invocation_id.clone(),
                            checkpoint_id: checkpoint_id.clone(),
                            checkpoint: sqlite_checkpoint(&tx, scope)?,
                            resumed: false,
                        }
                    }
                };
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "external_runtime.invocation_claim",
                    &format!("workflow/{}/node/{}", scope.run_id, scope.node_id),
                    &json!({
                        "invocation_id": invocation_id,
                        "checkpoint_id": checkpoint_id,
                        "idempotency_sha256": idempotency_sha256,
                        "outcome": claim_kind(&outcome),
                        "lease_token_sha256": sha256_text(lease_token),
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(outcome)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                // invocation_id is already the SHA-derived canonical binding of
                // the complete scope plus idempotency key. PostgreSQL text
                // cannot carry the NUL separators used in the hash preimage.
                let claim_lock = invocation_id.clone();
                tx.query_one(
                    "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                    &[&claim_lock],
                )
                .map_err(|error| error.to_string())?;
                let rows = tx
                    .query(
                        "SELECT invocation_id,status,updated_at,result_summary_json,artifact_id,failure_code
                         FROM external_runtime_invocations
                         WHERE tenant_id=$1 AND workspace_id=$2 AND run_id=$3 AND node_id=$4
                           AND idempotency_sha256=$5 FOR UPDATE",
                        &[&scope.tenant_id,&scope.workspace_id,&scope.run_id,&scope.node_id,&idempotency_sha256],
                    )
                    .map_err(|error| error.to_string())?;
                let outcome = if let Some(row) = rows.first() {
                    let id: String = row.get(0);
                    let status: String = row.get(1);
                    let updated_at: String = row.get(2);
                    if status == "completed" {
                        let result_json: Option<String> = row.get(3);
                        ExternalRuntimeInvocationClaim::Completed {
                            invocation_id: id,
                            checkpoint_id: checkpoint_id.clone(),
                            result_summary: parse_json_summary(
                                result_json.as_deref().unwrap_or("null"),
                                "stored external runtime result",
                            )?,
                            artifact_id: row.get(4),
                        }
                    } else if status == "blocked" {
                        ExternalRuntimeInvocationClaim::Blocked {
                            invocation_id: id,
                            failure_code: row
                                .get::<_, Option<String>>(5)
                                .unwrap_or_else(|| "external_runtime_blocked".to_string()),
                        }
                    } else if status == "started"
                        && !timestamp_is_stale(&updated_at, &now, stale_after_seconds)
                    {
                        ExternalRuntimeInvocationClaim::Busy {
                            invocation_id: id,
                            updated_at,
                        }
                    } else {
                        tx.execute(
                            "UPDATE external_runtime_invocations SET status='started',lease_token=$1,
                             failure_code=NULL,result_summary_json=NULL,artifact_id=NULL,updated_at=$2,
                             completed_at=NULL WHERE invocation_id=$3",
                            &[&lease_token,&now,&id],
                        )
                        .map_err(|error| error.to_string())?;
                        ExternalRuntimeInvocationClaim::Claimed {
                            invocation_id: id,
                            checkpoint_id: checkpoint_id.clone(),
                            checkpoint: pg_checkpoint_tx(&mut tx, scope)?,
                            resumed: true,
                        }
                    }
                } else {
                    tx.execute(
                        "INSERT INTO external_runtime_invocations
                         (invocation_id,tenant_id,workspace_id,run_id,node_id,thread_id,
                          idempotency_sha256,checkpoint_id,lease_token,status,created_at,updated_at)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'started',$10,$10)",
                        &[&invocation_id,&scope.tenant_id,&scope.workspace_id,&scope.run_id,
                          &scope.node_id,&scope.thread_id,&idempotency_sha256,&checkpoint_id,&lease_token,&now],
                    )
                    .map_err(|error| error.to_string())?;
                    ExternalRuntimeInvocationClaim::Claimed {
                        invocation_id: invocation_id.clone(),
                        checkpoint_id: checkpoint_id.clone(),
                        checkpoint: pg_checkpoint_tx(&mut tx, scope)?,
                        resumed: false,
                    }
                };
                pg_audit(
                    &mut tx,
                    &now,
                    actor,
                    "external_runtime.invocation_claim",
                    &format!("workflow/{}/node/{}", scope.run_id, scope.node_id),
                    &json!({
                        "invocation_id": invocation_id,
                        "checkpoint_id": checkpoint_id,
                        "idempotency_sha256": idempotency_sha256,
                        "outcome": claim_kind(&outcome),
                        "lease_token_sha256": sha256_text(lease_token),
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(outcome)
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_external_runtime_invocation(
        &self,
        scope: &ExternalRuntimeScope,
        invocation_id: &str,
        lease_token: &str,
        adapter_version: &str,
        runtime_version: &str,
        memory_strategy: &str,
        checkpoint: &Value,
        result_summary: &Value,
        artifact_id: &str,
        actor: &str,
    ) -> Result<Value, String> {
        scope.validate()?;
        validate_identifier("invocation_id", invocation_id)?;
        validate_identifier("lease_token", lease_token)?;
        validate_identifier("adapter_version", adapter_version)?;
        validate_identifier("runtime_version", runtime_version)?;
        validate_identifier("artifact_id", artifact_id)?;
        validate_identifier("actor", actor)?;
        validate_memory_strategy(memory_strategy)?;
        let checkpoint_id = checkpoint
            .get("checkpoint_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "checkpoint_id is required".to_string())?;
        validate_identifier("checkpoint_id", checkpoint_id)?;
        let version = checkpoint
            .get("version")
            .and_then(Value::as_i64)
            .filter(|version| *version > 0)
            .ok_or_else(|| "checkpoint version must be positive".to_string())?;
        let checkpoint_summary = checkpoint
            .get("state_summary")
            .ok_or_else(|| "checkpoint state_summary is required".to_string())?;
        let state_sha256 = checkpoint
            .get("state_sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| "checkpoint state_sha256 is required".to_string())?;
        validate_sha256("checkpoint state_sha256", state_sha256)?;
        validate_summary(
            checkpoint,
            MAX_CHECKPOINT_SUMMARY_BYTES,
            "checkpoint summary",
        )?;
        validate_summary(result_summary, MAX_RESULT_SUMMARY_BYTES, "result summary")?;
        let state_json = canonical_json(checkpoint_summary)?;
        if sha256_text(&state_json) != state_sha256 {
            return Err("checkpoint state hash does not match its bounded summary".into());
        }
        let checkpoint_json = canonical_json(checkpoint)?;
        let result_json = canonical_json(result_summary)?;
        let checkpoint_status = "active";
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    TransactionBehavior::Immediate,
                )
                    .map_err(|error| error.to_string())?;
                let existing_version: Option<i64> = tx.query_row(
                    "SELECT version FROM external_runtime_checkpoints
                     WHERE tenant_id=?1 AND workspace_id=?2 AND run_id=?3 AND node_id=?4 AND thread_id=?5",
                    params![scope.tenant_id,scope.workspace_id,scope.run_id,scope.node_id,scope.thread_id],
                    |row| row.get(0),
                ).optional().map_err(|error| error.to_string())?;
                let expected_version = existing_version.map_or(1, |current| current + 1);
                if version != expected_version {
                    return Err(format!("checkpoint version must advance exactly to {expected_version}"));
                }
                tx.execute(
                    "INSERT INTO external_runtime_checkpoints
                     (checkpoint_id,tenant_id,workspace_id,run_id,node_id,thread_id,runtime_kind,
                      adapter_version,runtime_version,memory_strategy,checkpoint_summary_json,
                      state_sha256,status,version,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,'langgraph',?7,?8,?9,?10,?11,?12,?13,?14,?14)
                     ON CONFLICT(tenant_id,workspace_id,run_id,node_id,thread_id) DO UPDATE SET
                      checkpoint_id=excluded.checkpoint_id,adapter_version=excluded.adapter_version,
                      runtime_version=excluded.runtime_version,
                      memory_strategy=excluded.memory_strategy,
                      checkpoint_summary_json=excluded.checkpoint_summary_json,
                      state_sha256=excluded.state_sha256,status=excluded.status,
                      version=excluded.version,updated_at=excluded.updated_at",
                    params![checkpoint_id,scope.tenant_id,scope.workspace_id,scope.run_id,scope.node_id,
                        scope.thread_id,adapter_version,runtime_version,memory_strategy,checkpoint_json,
                        state_sha256,checkpoint_status,version,now],
                ).map_err(|error| error.to_string())?;
                let changed = tx.execute(
                    "UPDATE external_runtime_invocations SET status='completed',result_summary_json=?1,
                     artifact_id=?2,checkpoint_id=?3,failure_code=NULL,updated_at=?4,completed_at=?4
                     WHERE invocation_id=?5 AND tenant_id=?6 AND workspace_id=?7 AND run_id=?8
                       AND node_id=?9 AND thread_id=?10 AND status='started' AND lease_token=?11",
                    params![result_json,artifact_id,checkpoint_id,now,invocation_id,scope.tenant_id,
                        scope.workspace_id,scope.run_id,scope.node_id,scope.thread_id,lease_token],
                ).map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("external runtime completion lost its exact invocation lease".into());
                }
                append_audit_locked(&tx,&now,actor,"external_runtime.invocation_complete",
                    &format!("workflow/{}/node/{}",scope.run_id,scope.node_id),
                    &json!({"invocation_id":invocation_id,"checkpoint_id":checkpoint_id,
                        "checkpoint_version":version,"state_sha256":state_sha256,
                        "artifact_id":artifact_id,"raw_content_persisted":false}))?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(checkpoint_value(scope,checkpoint_id,adapter_version,runtime_version,
                    memory_strategy,checkpoint,checkpoint_status,version,state_sha256,&now))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let rows = tx.query(
                    "SELECT version FROM external_runtime_checkpoints
                     WHERE tenant_id=$1 AND workspace_id=$2 AND run_id=$3 AND node_id=$4 AND thread_id=$5 FOR UPDATE",
                    &[&scope.tenant_id,&scope.workspace_id,&scope.run_id,&scope.node_id,&scope.thread_id],
                ).map_err(|error| error.to_string())?;
                let expected_version = rows.first().map_or(1, |row| row.get::<_,i64>(0) + 1);
                if version != expected_version {
                    return Err(format!("checkpoint version must advance exactly to {expected_version}"));
                }
                tx.execute(
                    "INSERT INTO external_runtime_checkpoints
                     (checkpoint_id,tenant_id,workspace_id,run_id,node_id,thread_id,runtime_kind,
                      adapter_version,runtime_version,memory_strategy,checkpoint_summary_json,
                      state_sha256,status,version,created_at,updated_at)
                     VALUES ($1,$2,$3,$4,$5,$6,'langgraph',$7,$8,$9,$10,$11,$12,$13,$14,$14)
                     ON CONFLICT(tenant_id,workspace_id,run_id,node_id,thread_id) DO UPDATE SET
                      checkpoint_id=EXCLUDED.checkpoint_id,adapter_version=EXCLUDED.adapter_version,
                      runtime_version=EXCLUDED.runtime_version,
                      memory_strategy=EXCLUDED.memory_strategy,
                      checkpoint_summary_json=EXCLUDED.checkpoint_summary_json,
                      state_sha256=EXCLUDED.state_sha256,status=EXCLUDED.status,
                      version=EXCLUDED.version,updated_at=EXCLUDED.updated_at",
                    &[&checkpoint_id,&scope.tenant_id,&scope.workspace_id,&scope.run_id,&scope.node_id,
                      &scope.thread_id,&adapter_version,&runtime_version,&memory_strategy,&checkpoint_json,
                      &state_sha256,&checkpoint_status,&version,&now],
                ).map_err(|error| error.to_string())?;
                let changed = tx.execute(
                    "UPDATE external_runtime_invocations SET status='completed',result_summary_json=$1,
                     artifact_id=$2,checkpoint_id=$3,failure_code=NULL,updated_at=$4,completed_at=$4
                     WHERE invocation_id=$5 AND tenant_id=$6 AND workspace_id=$7 AND run_id=$8
                       AND node_id=$9 AND thread_id=$10 AND status='started' AND lease_token=$11",
                    &[&result_json,&artifact_id,&checkpoint_id,&now,&invocation_id,&scope.tenant_id,
                      &scope.workspace_id,&scope.run_id,&scope.node_id,&scope.thread_id,&lease_token],
                ).map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("external runtime completion lost its exact invocation lease".into());
                }
                pg_audit(&mut tx,&now,actor,"external_runtime.invocation_complete",
                    &format!("workflow/{}/node/{}",scope.run_id,scope.node_id),
                    &json!({"invocation_id":invocation_id,"checkpoint_id":checkpoint_id,
                        "checkpoint_version":version,"state_sha256":state_sha256,
                        "artifact_id":artifact_id,"raw_content_persisted":false}))?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(checkpoint_value(scope,checkpoint_id,adapter_version,runtime_version,
                    memory_strategy,checkpoint,checkpoint_status,version,state_sha256,&now))
            }),
        }
    }

    pub fn fail_external_runtime_invocation(
        &self,
        scope: &ExternalRuntimeScope,
        invocation_id: &str,
        lease_token: &str,
        failure_code: &str,
        blocked: bool,
        actor: &str,
    ) -> Result<(), String> {
        scope.validate()?;
        for (field, value) in [
            ("invocation_id", invocation_id),
            ("lease_token", lease_token),
            ("failure_code", failure_code),
            ("actor", actor),
        ] {
            validate_identifier(field, value)?;
        }
        let status = if blocked { "blocked" } else { "failed" };
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn.unchecked_transaction().map_err(|error| error.to_string())?;
                let changed = tx.execute(
                    "UPDATE external_runtime_invocations SET status=?1,failure_code=?2,updated_at=?3,
                     completed_at=?3 WHERE invocation_id=?4 AND tenant_id=?5 AND workspace_id=?6
                     AND run_id=?7 AND node_id=?8 AND thread_id=?9 AND status='started'
                     AND lease_token=?10",
                    params![status,failure_code,now,invocation_id,scope.tenant_id,scope.workspace_id,
                        scope.run_id,scope.node_id,scope.thread_id,lease_token],
                ).map_err(|error| error.to_string())?;
                if changed != 1 { return Err("external runtime failure lost its exact invocation lease".into()); }
                append_audit_locked(&tx,&now,actor,"external_runtime.invocation_failed",
                    &format!("workflow/{}/node/{}",scope.run_id,scope.node_id),
                    &json!({"invocation_id":invocation_id,"status":status,"failure_code":failure_code}))?;
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let changed = tx.execute(
                    "UPDATE external_runtime_invocations SET status=$1,failure_code=$2,updated_at=$3,
                     completed_at=$3 WHERE invocation_id=$4 AND tenant_id=$5 AND workspace_id=$6
                     AND run_id=$7 AND node_id=$8 AND thread_id=$9 AND status='started'
                     AND lease_token=$10",
                    &[&status,&failure_code,&now,&invocation_id,&scope.tenant_id,&scope.workspace_id,
                      &scope.run_id,&scope.node_id,&scope.thread_id,&lease_token],
                ).map_err(|error| error.to_string())?;
                if changed != 1 { return Err("external runtime failure lost its exact invocation lease".into()); }
                pg_audit(&mut tx,&now,actor,"external_runtime.invocation_failed",
                    &format!("workflow/{}/node/{}",scope.run_id,scope.node_id),
                    &json!({"invocation_id":invocation_id,"status":status,"failure_code":failure_code}))?;
                tx.commit().map_err(|error| error.to_string())
            }),
        }
    }

    pub fn external_runtime_checkpoint(
        &self,
        scope: &ExternalRuntimeScope,
    ) -> Result<Option<Value>, String> {
        scope.validate()?;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| sqlite_checkpoint(conn, scope)),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                self.with_pg_conn(|client| pg_checkpoint_client(client, scope))
            }
        }
    }
}

fn validate_identifier(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(format!(
            "{field} must contain 1..={MAX_IDENTIFIER_BYTES} bytes"
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(format!("{field} contains unsupported characters"));
    }
    Ok(())
}

pub fn validate_memory_strategy(value: &str) -> Result<(), String> {
    if MEMORY_STRATEGIES.contains(&value) {
        Ok(())
    } else {
        Err(format!("unsupported memory strategy: {value}"))
    }
}

fn validate_sha256(field: &str, value: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(format!("{field} must be 64 lowercase hex characters"))
    }
}

fn validate_summary(value: &Value, max_bytes: usize, label: &str) -> Result<(), String> {
    let encoded = canonical_json(value)?;
    if encoded.len() > max_bytes {
        return Err(format!("{label} exceeds {max_bytes} byte cap"));
    }
    reject_sensitive_summary(value, 0, label)
}

fn reject_sensitive_summary(value: &Value, depth: usize, label: &str) -> Result<(), String> {
    if depth > 12 {
        return Err(format!("{label} exceeds 12 nesting levels"));
    }
    match value {
        Value::Object(map) => {
            if map.len() > 64 {
                return Err(format!("{label} object exceeds 64 fields"));
            }
            for (key, child) in map {
                if matches!(
                    key.as_str(),
                    "prompt"
                        | "raw_prompt"
                        | "output"
                        | "raw_output"
                        | "transcript"
                        | "messages"
                        | "checkpoint_content"
                        | "repository_content"
                        | "credential"
                        | "api_key"
                        | "private_path"
                ) {
                    return Err(format!("{label} contains forbidden raw field {key}"));
                }
                reject_sensitive_summary(child, depth + 1, label)?;
            }
        }
        Value::Array(items) => {
            if items.len() > 128 {
                return Err(format!("{label} array exceeds 128 items"));
            }
            for item in items {
                reject_sensitive_summary(item, depth + 1, label)?;
            }
        }
        Value::String(text) => {
            if text.len() > 1024 {
                return Err(format!("{label} string exceeds 1024 bytes"));
            }
            if crate::provider::redaction::contains_sensitive_patterns(text) {
                return Err(format!("{label} contains secret-shaped text"));
            }
            if text.starts_with('/') || text.contains("\\Users\\") {
                return Err(format!("{label} contains a private path"));
            }
        }
        _ => {}
    }
    Ok(())
}

fn canonical_json(value: &Value) -> Result<String, String> {
    crate::event_schema::canonical_event_json(value).map_err(|error| error.to_string())
}

fn sha256_text(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn timestamp_is_stale(updated_at: &str, now: &str, stale_after_seconds: i64) -> bool {
    DateTime::parse_from_rfc3339(updated_at)
        .and_then(|updated| {
            DateTime::parse_from_rfc3339(now)
                .map(|current| current.signed_duration_since(updated).num_seconds())
        })
        .map(|age_seconds| age_seconds >= stale_after_seconds)
        .unwrap_or(true)
}

fn claim_kind(claim: &ExternalRuntimeInvocationClaim) -> &'static str {
    match claim {
        ExternalRuntimeInvocationClaim::Claimed { resumed: false, .. } => "claimed",
        ExternalRuntimeInvocationClaim::Claimed { resumed: true, .. } => "resumed",
        ExternalRuntimeInvocationClaim::Completed { .. } => "completed",
        ExternalRuntimeInvocationClaim::Busy { .. } => "busy",
        ExternalRuntimeInvocationClaim::Blocked { .. } => "blocked",
    }
}

fn parse_json_summary(text: &str, label: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("invalid {label}: {error}"))
}

fn checkpoint_value(
    scope: &ExternalRuntimeScope,
    checkpoint_id: &str,
    adapter_version: &str,
    runtime_version: &str,
    memory_strategy: &str,
    summary: &Value,
    status: &str,
    version: i64,
    state_sha256: &str,
    updated_at: &str,
) -> Value {
    json!({
        "schema_version": EXTERNAL_RUNTIME_CHECKPOINT_SCHEMA_VERSION,
        "checkpoint_id": checkpoint_id,
        "tenant_id": scope.tenant_id,
        "workspace_id": scope.workspace_id,
        "run_id": scope.run_id,
        "node_id": scope.node_id,
        "thread_id": scope.thread_id,
        "runtime_kind": "langgraph",
        "adapter_version": adapter_version,
        "runtime_version": runtime_version,
        "memory_strategy": memory_strategy,
        "checkpoint_summary": summary,
        "state_sha256": state_sha256,
        "status": status,
        "version": version,
        "updated_at": updated_at,
        "raw_content_persisted": false,
    })
}

fn sqlite_checkpoint(
    conn: &rusqlite::Connection,
    scope: &ExternalRuntimeScope,
) -> Result<Option<Value>, String> {
    conn.query_row(
        "SELECT checkpoint_id,adapter_version,runtime_version,memory_strategy,checkpoint_summary_json,
                state_sha256,status,version,updated_at
         FROM external_runtime_checkpoints
         WHERE tenant_id=?1 AND workspace_id=?2 AND run_id=?3
           AND node_id=?4 AND thread_id=?5 AND status!='tombstoned'",
        params![
            scope.tenant_id,
            scope.workspace_id,
            scope.run_id,
            scope.node_id,
            scope.thread_id
        ],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
            ))
        },
    )
    .optional()
    .map_err(|error| error.to_string())?
    .map(
        |(checkpoint_id, adapter, runtime, strategy, summary, state, status, version, updated)| {
            let summary = parse_json_summary(&summary, "checkpoint summary")?;
            Ok(checkpoint_value(
                scope,
                &checkpoint_id,
                &adapter,
                &runtime,
                &strategy,
                &summary,
                &status,
                version,
                &state,
                &updated,
            ))
        },
    )
    .transpose()
}

#[cfg(feature = "pg")]
fn pg_checkpoint_tx(
    client: &mut postgres::Transaction<'_>,
    scope: &ExternalRuntimeScope,
) -> Result<Option<Value>, String> {
    let rows = client
        .query(
            "SELECT checkpoint_id,adapter_version,runtime_version,memory_strategy,checkpoint_summary_json,
                state_sha256,status,version,updated_at
         FROM external_runtime_checkpoints
         WHERE tenant_id=$1 AND workspace_id=$2 AND run_id=$3
           AND node_id=$4 AND thread_id=$5 AND status!='tombstoned'",
            &[
                &scope.tenant_id,
                &scope.workspace_id,
                &scope.run_id,
                &scope.node_id,
                &scope.thread_id,
            ],
        )
        .map_err(|error| error.to_string())?;
    rows.first()
        .map(|row| {
            let checkpoint_id: String = row.get(0);
            let summary_text: String = row.get(4);
            let summary = parse_json_summary(&summary_text, "checkpoint summary")?;
            Ok(checkpoint_value(
                scope,
                &checkpoint_id,
                &row.get::<_, String>(1),
                &row.get::<_, String>(2),
                &row.get::<_, String>(3),
                &summary,
                &row.get::<_, String>(6),
                row.get::<_, i64>(7),
                &row.get::<_, String>(5),
                &row.get::<_, String>(8),
            ))
        })
        .transpose()
}

#[cfg(feature = "pg")]
fn pg_checkpoint_client(
    client: &mut postgres::Client,
    scope: &ExternalRuntimeScope,
) -> Result<Option<Value>, String> {
    let rows = client
        .query(
            "SELECT checkpoint_id,adapter_version,runtime_version,memory_strategy,checkpoint_summary_json,
                state_sha256,status,version,updated_at
         FROM external_runtime_checkpoints
         WHERE tenant_id=$1 AND workspace_id=$2 AND run_id=$3
           AND node_id=$4 AND thread_id=$5 AND status!='tombstoned'",
            &[
                &scope.tenant_id,
                &scope.workspace_id,
                &scope.run_id,
                &scope.node_id,
                &scope.thread_id,
            ],
        )
        .map_err(|error| error.to_string())?;
    rows.first()
        .map(|row| {
            let checkpoint_id: String = row.get(0);
            let summary_text: String = row.get(4);
            let summary = parse_json_summary(&summary_text, "checkpoint summary")?;
            Ok(checkpoint_value(
                scope,
                &checkpoint_id,
                &row.get::<_, String>(1),
                &row.get::<_, String>(2),
                &row.get::<_, String>(3),
                &summary,
                &row.get::<_, String>(6),
                row.get::<_, i64>(7),
                &row.get::<_, String>(5),
                &row.get::<_, String>(8),
            ))
        })
        .transpose()
}

#[cfg(feature = "pg")]
fn pg_audit(
    tx: &mut postgres::Transaction<'_>,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO audit_log (created_at,actor,action,resource,details_json)
         VALUES ($1,$2,$3,$4,$5)",
        &[&now, &actor, &action, &resource, &details.to_string()],
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}
