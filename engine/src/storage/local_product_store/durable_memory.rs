use std::collections::BTreeSet;

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use crate::provider::embedding::{
    normalized_content_sha256, validate_inputs, EmbeddingContractEvidence, ProviderEmbeddingConfig,
    ProviderEmbeddingMetadata, OPENROUTER_EMBEDDING_MODEL_ID, OPENROUTER_EMBEDDING_PROVIDER_ID,
};
use crate::provider::ProviderAuditEvent;

use super::provider_audit::provider_embedding_operation_receipt_sha256;
use super::provider_audit::{ProviderEmbeddingOperation, ProviderEmbeddingOperationClaim};

pub const DURABLE_MEMORY_SCHEMA_VERSION: &str = "durable_memory.v1";
pub const MEMORY_RETRIEVAL_SCHEMA_VERSION: &str = "memory_retrieval_result.v1";
const MAX_CONTENT_BYTES: usize = 32 * 1024;
const MAX_VECTOR_DIMENSIONS: usize = 1_536;
const LOCAL_VECTOR_DIMENSIONS: usize = 128;
const MAX_TOP_K: usize = 20;
const MAX_CANDIDATES: usize = 500;
const MAX_RECORDED_CANDIDATE_SCORES: usize = 50;
const MAX_ACTIVE_MEMORIES_PER_WORKSPACE: i64 = 500;
const MAX_PRUNE_BATCH: i64 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryScope {
    pub tenant_id: String,
    pub workspace_id: String,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableMemoryCreate {
    pub scope: MemoryScope,
    pub run_id: Option<String>,
    pub source_id: String,
    pub source_sha256: String,
    pub conflict_key: String,
    pub content: Value,
    pub confidence: f64,
    pub fresh_until: Option<String>,
    pub expires_at: Option<String>,
    pub supersedes_memory_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableMemoryRevision {
    pub expected_version: i64,
    pub source_id: String,
    pub source_sha256: String,
    pub content: Value,
    pub confidence: f64,
    pub fresh_until: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRetrievalRequest {
    pub scope: MemoryScope,
    pub run_id: String,
    pub node_id: String,
    pub query: String,
    pub top_k: usize,
    pub max_tokens: usize,
    pub max_bytes: usize,
    #[serde(default)]
    pub allow_lexical_fallback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryReference {
    pub memory_id: String,
    pub version: i64,
    pub source_id: String,
    pub source_sha256: String,
    pub record_sha256: String,
    pub score: f64,
    pub confidence: f64,
    pub estimated_tokens: usize,
    pub content_bytes: usize,
    pub content: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRetrievalResult {
    pub schema_version: String,
    pub retrieval_id: String,
    pub mode: String,
    pub embedding_provenance: String,
    pub embedding_provider: Option<Value>,
    pub candidate_count: usize,
    pub candidate_scores: Vec<Value>,
    pub candidate_scores_truncated: bool,
    pub selected: Vec<MemoryReference>,
    pub stale_excluded: usize,
    pub state_excluded: usize,
    pub truncated: bool,
    pub estimated_tokens: usize,
    pub token_estimate_provenance: String,
    pub candidate_read_bytes: usize,
    pub read_bytes: usize,
    pub request_sha256: String,
    pub result_sha256: String,
}

#[derive(Debug, Clone)]
struct StoredMemory {
    memory_id: String,
    version: i64,
    scope: MemoryScope,
    run_id: Option<String>,
    source_id: String,
    source_sha256: String,
    conflict_key: String,
    state: String,
    confidence: f64,
    fresh_until: Option<String>,
    expires_at: Option<String>,
    supersedes_memory_id: Option<String>,
    content: Value,
    embedding: Option<Vec<f64>>,
    embedding_provenance: String,
    embedding_metadata: Option<StoredEmbeddingMetadata>,
    embedding_binding_sha256: Option<String>,
    record_sha256: String,
    created_at: String,
    created_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredEmbeddingMetadata {
    provider: ProviderEmbeddingMetadata,
    tenant_id: String,
    workspace_id: String,
    agent_id: Option<String>,
    run_id: Option<String>,
    task_id: Option<String>,
    memory_id: String,
    memory_version: i64,
    source_id: String,
    source_sha256: String,
}

#[derive(Debug, Clone)]
struct EmbeddingMaterial {
    values: Vec<f64>,
    provenance: String,
    provider: Option<ProviderEmbeddingMetadata>,
}

#[derive(Debug)]
struct EmbeddingOperationBinding<'a> {
    binding_sha256: &'a str,
    memory_id: &'a str,
    target_version: i64,
    scope: &'a MemoryScope,
    run_id: Option<&'a str>,
    source_id: &'a str,
    source_sha256: &'a str,
}

#[derive(Debug)]
struct ScopedMemoryInventory {
    candidates: Vec<StoredMemory>,
    state_excluded: usize,
    candidate_read_bytes: usize,
}

impl ScopedMemoryInventory {
    fn new(candidates: Vec<StoredMemory>, state_excluded: usize) -> Self {
        let candidate_read_bytes = candidates.iter().fold(0usize, |total, candidate| {
            let embedding_bytes = candidate
                .embedding
                .as_ref()
                .and_then(|embedding| serde_json::to_vec(embedding).ok())
                .map_or(0, |bytes| bytes.len());
            total
                .saturating_add(candidate.content.to_string().len())
                .saturating_add(embedding_bytes)
        });
        Self {
            candidates,
            state_excluded,
            candidate_read_bytes,
        }
    }
}

impl LocalProductStore {
    pub fn create_durable_memory(
        &self,
        request: &DurableMemoryCreate,
        actor: &str,
    ) -> Result<Value, String> {
        validate_create(request)?;
        validate_identifier(actor, "actor")?;
        let memory_id = memory_id(request)?;
        if let Some(existing) = self.latest_memory_preflight(&memory_id)? {
            validate_idempotent_create(&existing, request)?;
            return memory_to_value(&existing);
        }
        let operation_binding = sha256_json(&json!({
            "operation":"create",
            "memory_id":memory_id,
            "version":1,
            "source_id":request.source_id,
            "source_sha256":request.source_sha256,
            "content_sha256":sha256_json(&request.content)?,
        }))?;
        let embedding = self.embedding_for_text(
            &request.content.to_string(),
            Some(&EmbeddingOperationBinding {
                binding_sha256: &operation_binding,
                memory_id: &memory_id,
                target_version: 1,
                scope: &request.scope,
                run_id: request.run_id.as_deref(),
                source_id: &request.source_id,
                source_sha256: &request.source_sha256,
            }),
        )?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                if let Some(existing) = sqlite_latest_memory(&tx, &memory_id)? {
                    validate_idempotent_create(&existing, request)?;
                    return memory_to_value(&existing);
                }
                enforce_sqlite_workspace_retention(&tx, &request.scope)?;
                let conflict = sqlite_has_active_conflict(&tx, request)?;
                let state = if conflict { "conflicting" } else { "current" };
                if conflict {
                    require_conflict_capacity(sqlite_conflict_count_for_request(&tx, request)?)?;
                    sqlite_mark_conflicts(&tx, request, actor, &now)?;
                }
                let record = build_record(
                    memory_id.clone(),
                    1,
                    request.scope.clone(),
                    request.run_id.clone(),
                    request.source_id.clone(),
                    request.source_sha256.clone(),
                    request.conflict_key.clone(),
                    state,
                    request.confidence,
                    request.fresh_until.clone(),
                    request.expires_at.clone(),
                    request.supersedes_memory_id.clone(),
                    request.content.clone(),
                    embedding,
                    now.clone(),
                    actor.to_string(),
                )?;
                sqlite_insert_memory(&tx, &record)?;
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "durable_memory.create",
                    &format!("durable-memory/{memory_id}"),
                    &memory_audit(&record),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                memory_to_value(&record)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&request.scope.workspace_id],
                )
                .map_err(|error| error.to_string())?;
                if let Some(existing) = pg_latest_memory(&mut tx, &memory_id)? {
                    validate_idempotent_create(&existing, request)?;
                    return memory_to_value(&existing);
                }
                enforce_pg_workspace_retention(&mut tx, &request.scope)?;
                let conflict = pg_has_active_conflict(&mut tx, request)?;
                let state = if conflict { "conflicting" } else { "current" };
                if conflict {
                    require_conflict_capacity(pg_conflict_count_for_request(&mut tx, request)?)?;
                    pg_mark_conflicts(&mut tx, request, actor, &now)?;
                }
                let record = build_record(
                    memory_id.clone(),
                    1,
                    request.scope.clone(),
                    request.run_id.clone(),
                    request.source_id.clone(),
                    request.source_sha256.clone(),
                    request.conflict_key.clone(),
                    state,
                    request.confidence,
                    request.fresh_until.clone(),
                    request.expires_at.clone(),
                    request.supersedes_memory_id.clone(),
                    request.content.clone(),
                    embedding,
                    now.clone(),
                    actor.to_string(),
                )?;
                pg_insert_memory(&mut tx, &record)?;
                pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "durable_memory.create",
                    &format!("durable-memory/{memory_id}"),
                    &memory_audit(&record),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                memory_to_value(&record)
            }),
        }
    }

    pub fn revise_durable_memory(
        &self,
        memory_id: &str,
        revision: &DurableMemoryRevision,
        actor: &str,
    ) -> Result<Value, String> {
        validate_identifier(memory_id, "memory_id")?;
        validate_identifier(actor, "actor")?;
        validate_revision(revision)?;
        let prior = self
            .latest_memory_preflight(memory_id)?
            .ok_or_else(|| "durable memory not found".to_string())?;
        require_version(&prior, revision.expected_version)?;
        revisable_state(&prior)?;
        let operation_binding = sha256_json(&json!({
            "operation":"revise",
            "memory_id":memory_id,
            "expected_version":revision.expected_version,
            "next_version":revision.expected_version + 1,
            "source_id":revision.source_id,
            "source_sha256":revision.source_sha256,
            "content_sha256":sha256_json(&revision.content)?,
        }))?;
        let embedding = self.embedding_for_text(
            &revision.content.to_string(),
            Some(&EmbeddingOperationBinding {
                binding_sha256: &operation_binding,
                memory_id,
                target_version: revision.expected_version + 1,
                scope: &prior.scope,
                run_id: prior.run_id.as_deref(),
                source_id: &revision.source_id,
                source_sha256: &revision.source_sha256,
            }),
        )?;
        self.append_memory_version(
            memory_id,
            revision.expected_version,
            "current",
            revision,
            embedding,
            actor,
            "durable_memory.revise",
        )
    }

    pub fn reembed_durable_memory(
        &self,
        memory_id: &str,
        expected_version: i64,
        actor: &str,
    ) -> Result<Value, String> {
        validate_identifier(memory_id, "memory_id")?;
        validate_identifier(actor, "actor")?;
        if std::env::var("ACP_DURABLE_MEMORY_EMBEDDING_MODE").as_deref() != Ok("provider") {
            return Err("durable memory re-embedding requires provider mode".to_string());
        }
        let prior = self
            .latest_memory_preflight(memory_id)?
            .ok_or_else(|| "durable memory not found".to_string())?;
        require_version(&prior, expected_version)?;
        revisable_state(&prior)?;
        let next_version = expected_version + 1;
        let operation_binding = sha256_json(&json!({
            "operation":"reembed",
            "memory_id":memory_id,
            "expected_version":expected_version,
            "next_version":next_version,
            "previous_record_sha256":prior.record_sha256,
            "source_id":prior.source_id,
            "source_sha256":prior.source_sha256,
            "content_sha256":sha256_json(&prior.content)?,
            "target_model_id":OPENROUTER_EMBEDDING_MODEL_ID,
        }))?;
        let embedding = self.embedding_for_text(
            &prior.content.to_string(),
            Some(&EmbeddingOperationBinding {
                binding_sha256: &operation_binding,
                memory_id,
                target_version: next_version,
                scope: &prior.scope,
                run_id: prior.run_id.as_deref(),
                source_id: &prior.source_id,
                source_sha256: &prior.source_sha256,
            }),
        )?;
        let revision = DurableMemoryRevision {
            expected_version,
            source_id: prior.source_id.clone(),
            source_sha256: prior.source_sha256.clone(),
            content: prior.content.clone(),
            confidence: prior.confidence,
            fresh_until: prior.fresh_until.clone(),
            expires_at: prior.expires_at.clone(),
        };
        self.append_memory_version(
            memory_id,
            expected_version,
            "current",
            &revision,
            embedding,
            actor,
            "durable_memory.reembed",
        )
    }

    pub fn invalidate_durable_memory(
        &self,
        memory_id: &str,
        expected_version: i64,
        actor: &str,
    ) -> Result<Value, String> {
        self.transition_memory(memory_id, expected_version, "invalid", actor)
    }

    pub fn forget_durable_memory(
        &self,
        memory_id: &str,
        expected_version: i64,
        actor: &str,
    ) -> Result<Value, String> {
        self.transition_memory(memory_id, expected_version, "tombstoned", actor)
    }

    pub fn prune_expired_durable_memories(
        &self,
        scope: &MemoryScope,
        actor: &str,
    ) -> Result<Value, String> {
        validate_scope(scope)?;
        let now = self.now();
        validate_identifier(actor, "actor")?;
        let pruned = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let expired = sqlite_expired_memories(&tx, scope, &now)?;
                let mut pruned = Vec::with_capacity(expired.len());
                for prior in expired {
                    let record = clone_with_state(&prior, "expired", &now, actor)?;
                    sqlite_insert_memory(&tx, &record)?;
                    append_audit_locked(
                        &tx,
                        &now,
                        actor,
                        "durable_memory.expired",
                        &format!("durable-memory/{}", record.memory_id),
                        &memory_audit(&record),
                    )?;
                    pruned.push(memory_prune_reference(&record));
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(pruned)
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&scope.workspace_id],
                )
                .map_err(|error| error.to_string())?;
                let expired = pg_expired_memories(&mut tx, scope, &now)?;
                let mut pruned = Vec::with_capacity(expired.len());
                for prior in expired {
                    let record = clone_with_state(&prior, "expired", &now, actor)?;
                    pg_insert_memory(&mut tx, &record)?;
                    pg_append_audit(
                        &mut tx,
                        &now,
                        actor,
                        "durable_memory.expired",
                        &format!("durable-memory/{}", record.memory_id),
                        &memory_audit(&record),
                    )?;
                    pruned.push(memory_prune_reference(&record));
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(pruned)
            })?,
        };
        Ok(json!({
            "schema_version": "durable_memory_prune.v1",
            "tenant_id": scope.tenant_id,
            "workspace_id": scope.workspace_id,
            "pruned_count": pruned.len(),
            "pruned": pruned,
            "bounded_limit": MAX_PRUNE_BATCH,
        }))
    }

    pub fn supersede_durable_memory(
        &self,
        winner_memory_id: &str,
        winner_expected_version: i64,
        loser_memory_id: &str,
        loser_expected_version: i64,
        actor: &str,
    ) -> Result<Value, String> {
        validate_identifier(winner_memory_id, "winner_memory_id")?;
        validate_identifier(loser_memory_id, "loser_memory_id")?;
        validate_identifier(actor, "actor")?;
        if winner_memory_id == loser_memory_id {
            return Err("memory cannot supersede itself".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let winner_prior = sqlite_latest_memory(&tx, winner_memory_id)?
                    .ok_or_else(|| "winner durable memory not found".to_string())?;
                let loser_prior = sqlite_latest_memory(&tx, loser_memory_id)?
                    .ok_or_else(|| "loser durable memory not found".to_string())?;
                let (winner, loser) = resolve_supersede(
                    &winner_prior,
                    winner_expected_version,
                    &loser_prior,
                    loser_expected_version,
                    &now,
                    actor,
                )?;
                require_complete_conflict_pair(sqlite_active_conflict_count(&tx, &winner_prior)?)?;
                sqlite_insert_memory(&tx, &loser)?;
                sqlite_insert_memory(&tx, &winner)?;
                let evidence = supersede_audit(&winner, &loser);
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "durable_memory.supersede",
                    &format!("durable-memory/{winner_memory_id}"),
                    &evidence,
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(json!({
                    "winner": memory_to_value(&winner)?,
                    "superseded": memory_to_value(&loser)?,
                    "evidence": evidence,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let mut ids = [winner_memory_id, loser_memory_id];
                ids.sort_unstable();
                for id in ids {
                    pg_lock_memory(&mut tx, id)?;
                }
                let winner_prior = pg_latest_memory_for_update(&mut tx, winner_memory_id)?
                    .ok_or_else(|| "winner durable memory not found".to_string())?;
                let loser_prior = pg_latest_memory_for_update(&mut tx, loser_memory_id)?
                    .ok_or_else(|| "loser durable memory not found".to_string())?;
                let (winner, loser) = resolve_supersede(
                    &winner_prior,
                    winner_expected_version,
                    &loser_prior,
                    loser_expected_version,
                    &now,
                    actor,
                )?;
                require_complete_conflict_pair(pg_active_conflict_count(&mut tx, &winner_prior)?)?;
                pg_insert_memory(&mut tx, &loser)?;
                pg_insert_memory(&mut tx, &winner)?;
                let evidence = supersede_audit(&winner, &loser);
                pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "durable_memory.supersede",
                    &format!("durable-memory/{winner_memory_id}"),
                    &evidence,
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(json!({
                    "winner": memory_to_value(&winner)?,
                    "superseded": memory_to_value(&loser)?,
                    "evidence": evidence,
                }))
            }),
        }
    }

    fn transition_memory(
        &self,
        memory_id: &str,
        expected_version: i64,
        state: &str,
        actor: &str,
    ) -> Result<Value, String> {
        validate_identifier(memory_id, "memory_id")?;
        validate_identifier(actor, "actor")?;
        if !matches!(
            state,
            "current" | "superseded" | "invalid" | "tombstoned" | "expired"
        ) {
            return Err("invalid durable memory transition".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let prior = sqlite_latest_memory(&tx, memory_id)?
                    .ok_or_else(|| "durable memory not found".to_string())?;
                require_version(&prior, expected_version)?;
                let record = transition_record(&prior, state, &now, actor)?;
                if state == "tombstoned" {
                    tx.execute(
                        "DELETE FROM durable_memory_versions WHERE memory_id=?1",
                        params![memory_id],
                    )
                    .map_err(|error| error.to_string())?;
                }
                sqlite_insert_memory(&tx, &record)?;
                let mut evidence = memory_audit(&record);
                evidence["previous_record_sha256"] = json!(prior.record_sha256);
                evidence["prior_versions_erased"] = json!(state == "tombstoned");
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    &format!("durable_memory.{state}"),
                    &format!("durable-memory/{memory_id}"),
                    &evidence,
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                memory_to_value(&record)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                pg_lock_memory(&mut tx, memory_id)?;
                let prior = pg_latest_memory_for_update(&mut tx, memory_id)?
                    .ok_or_else(|| "durable memory not found".to_string())?;
                require_version(&prior, expected_version)?;
                let record = transition_record(&prior, state, &now, actor)?;
                if state == "tombstoned" {
                    tx.execute(
                        "DELETE FROM durable_memory_versions WHERE memory_id=$1",
                        &[&memory_id],
                    )
                    .map_err(|error| error.to_string())?;
                }
                pg_insert_memory(&mut tx, &record)?;
                let mut evidence = memory_audit(&record);
                evidence["previous_record_sha256"] = json!(prior.record_sha256);
                evidence["prior_versions_erased"] = json!(state == "tombstoned");
                pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    &format!("durable_memory.{state}"),
                    &format!("durable-memory/{memory_id}"),
                    &evidence,
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                memory_to_value(&record)
            }),
        }
    }

    fn append_memory_version(
        &self,
        memory_id: &str,
        expected_version: i64,
        _state: &str,
        revision: &DurableMemoryRevision,
        embedding: Option<EmbeddingMaterial>,
        actor: &str,
        audit_action: &str,
    ) -> Result<Value, String> {
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let prior = sqlite_latest_memory(&tx, memory_id)?
                    .ok_or_else(|| "durable memory not found".to_string())?;
                require_version(&prior, expected_version)?;
                let next_state = revisable_state(&prior)?;
                let record = build_record(
                    memory_id.to_string(),
                    prior.version + 1,
                    prior.scope,
                    prior.run_id,
                    revision.source_id.clone(),
                    revision.source_sha256.clone(),
                    prior.conflict_key,
                    &next_state,
                    revision.confidence,
                    revision.fresh_until.clone(),
                    revision.expires_at.clone(),
                    prior.supersedes_memory_id,
                    revision.content.clone(),
                    embedding,
                    now.clone(),
                    actor.to_string(),
                )?;
                sqlite_insert_memory(&tx, &record)?;
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    audit_action,
                    &format!("durable-memory/{memory_id}"),
                    &memory_audit(&record),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                memory_to_value(&record)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                pg_lock_memory(&mut tx, memory_id)?;
                let prior = pg_latest_memory_for_update(&mut tx, memory_id)?
                    .ok_or_else(|| "durable memory not found".to_string())?;
                require_version(&prior, expected_version)?;
                let next_state = revisable_state(&prior)?;
                let record = build_record(
                    memory_id.to_string(),
                    prior.version + 1,
                    prior.scope,
                    prior.run_id,
                    revision.source_id.clone(),
                    revision.source_sha256.clone(),
                    prior.conflict_key,
                    &next_state,
                    revision.confidence,
                    revision.fresh_until.clone(),
                    revision.expires_at.clone(),
                    prior.supersedes_memory_id,
                    revision.content.clone(),
                    embedding,
                    now.clone(),
                    actor.to_string(),
                )?;
                pg_insert_memory(&mut tx, &record)?;
                pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    audit_action,
                    &format!("durable-memory/{memory_id}"),
                    &memory_audit(&record),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                memory_to_value(&record)
            }),
        }
    }

    pub fn inspect_durable_memory(&self, memory_id: &str) -> Result<Vec<Value>, String> {
        validate_identifier(memory_id, "memory_id")?;
        let rows = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn.prepare("SELECT memory_id, version, tenant_id, workspace_id, agent_id, run_id, task_id, source_id, source_sha256, conflict_key, state, confidence, fresh_until, expires_at, supersedes_memory_id, content_json, embedding_json, embedding_provenance, embedding_metadata_json, embedding_binding_sha256, record_sha256, created_at, created_by FROM durable_memory_versions WHERE memory_id=?1 ORDER BY version")
                    .map_err(|error| error.to_string())?;
                let rows = stmt.query_map(params![memory_id], sqlite_memory_row).map_err(|error| error.to_string())?;
                rows.map(|row| row.map_err(|error| error.to_string())).collect::<Result<Vec<_>, _>>()
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client.query("SELECT memory_id, version, tenant_id, workspace_id, agent_id, run_id, task_id, source_id, source_sha256, conflict_key, state, confidence::DOUBLE PRECISION, fresh_until, expires_at, supersedes_memory_id, content_json, embedding_json, embedding_provenance, embedding_metadata_json, embedding_binding_sha256, record_sha256, created_at, created_by FROM durable_memory_versions WHERE memory_id=$1 ORDER BY version", &[&memory_id])
                    .map_err(|error| error.to_string())?.iter().map(pg_memory_row).collect()
            })?,
        };
        rows.iter().map(memory_to_value).collect()
    }

    pub fn retrieve_durable_memories(
        &self,
        request: &MemoryRetrievalRequest,
        actor: &str,
    ) -> Result<MemoryRetrievalResult, String> {
        validate_retrieval(request)?;
        validate_identifier(actor, "actor")?;
        let query_embedding = self.embedding_for_text(&request.query, None)?;
        let (mode, provenance) = match &query_embedding {
            Some(material) => ("semantic_vector", material.provenance.clone()),
            None if request.allow_lexical_fallback => {
                ("lexical_fallback", "unavailable".to_string())
            }
            None => {
                return Err(
                    "semantic retrieval unavailable and lexical fallback not allowed".to_string(),
                )
            }
        };
        let now = self.now();
        let inventory = self.scoped_current_memories(&request.scope)?;
        let mut stale_excluded = 0usize;
        let mut scored = Vec::<(f64, StoredMemory)>::new();
        for candidate in inventory.candidates {
            if is_stale(&candidate, &now) {
                stale_excluded += 1;
                continue;
            }
            let score = if let Some(query) = &query_embedding {
                let Some(candidate_embedding) = candidate.embedding.as_ref() else {
                    continue;
                };
                require_compatible_embedding(query, &candidate)?;
                cosine_similarity(&query.values, candidate_embedding)?
            } else {
                lexical_similarity(&request.query, &candidate.content.to_string())
            };
            scored.push((score, candidate));
        }
        scored.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .total_cmp(left_score)
                .then_with(|| right.confidence.total_cmp(&left.confidence))
                .then_with(|| left.memory_id.cmp(&right.memory_id))
                .then_with(|| right.version.cmp(&left.version))
        });
        let candidate_count = scored.len();
        let candidate_scores = scored
            .iter()
            .take(MAX_RECORDED_CANDIDATE_SCORES)
            .map(|(score, candidate)| candidate_score_evidence(*score, candidate))
            .collect::<Vec<_>>();
        let candidate_scores_truncated = candidate_count > candidate_scores.len();
        let mut selected = Vec::new();
        let mut used_tokens = 0usize;
        let mut used_bytes = 0usize;
        let mut truncated = candidate_count > request.top_k;
        for (score, candidate) in scored.into_iter().take(request.top_k) {
            let content_bytes = candidate.content.to_string().len();
            let estimated_tokens = content_bytes.div_ceil(4);
            if used_tokens.saturating_add(estimated_tokens) > request.max_tokens
                || used_bytes.saturating_add(content_bytes) > request.max_bytes
            {
                truncated = true;
                continue;
            }
            used_tokens += estimated_tokens;
            used_bytes += content_bytes;
            selected.push(MemoryReference {
                memory_id: candidate.memory_id,
                version: candidate.version,
                source_id: candidate.source_id,
                source_sha256: candidate.source_sha256,
                record_sha256: candidate.record_sha256,
                score,
                confidence: candidate.confidence,
                estimated_tokens,
                content_bytes,
                content: candidate.content,
            });
        }
        let request_sha256 = sha256_json(&json!({
            "scope": request.scope, "run_id": request.run_id, "node_id": request.node_id,
            "query_sha256": sha256_bytes(request.query.as_bytes()), "top_k": request.top_k,
            "max_tokens": request.max_tokens, "max_bytes": request.max_bytes, "mode": mode,
            "embedding_provider": query_embedding.as_ref().and_then(|material| material.provider.as_ref()).map(provider_retrieval_evidence),
        }))?;
        let evidence = retrieval_evidence(&selected);
        let result_sha256 = sha256_json(&json!({
            "request_sha256": request_sha256, "mode": mode, "selected": evidence,
            "candidate_scores": candidate_scores,
            "candidate_scores_truncated": candidate_scores_truncated,
            "candidate_count": candidate_count, "stale_excluded": stale_excluded,
            "state_excluded": inventory.state_excluded, "truncated": truncated,
            "estimated_tokens": used_tokens,
            "token_estimate_provenance": "harness_derived_bytes_div_4",
            "candidate_read_bytes": inventory.candidate_read_bytes,
            "read_bytes": used_bytes,
            "embedding_provider": query_embedding.as_ref().and_then(|material| material.provider.as_ref()).map(provider_retrieval_evidence),
        }))?;
        let retrieval_id = format!("retrieval-{result_sha256}");
        let result = MemoryRetrievalResult {
            schema_version: MEMORY_RETRIEVAL_SCHEMA_VERSION.to_string(),
            retrieval_id: retrieval_id.clone(),
            mode: mode.to_string(),
            embedding_provenance: provenance,
            embedding_provider: query_embedding
                .as_ref()
                .and_then(|material| material.provider.as_ref())
                .map(provider_retrieval_evidence),
            candidate_count,
            candidate_scores,
            candidate_scores_truncated,
            selected,
            stale_excluded,
            state_excluded: inventory.state_excluded,
            truncated,
            estimated_tokens: used_tokens,
            token_estimate_provenance: "harness_derived_bytes_div_4".to_string(),
            candidate_read_bytes: inventory.candidate_read_bytes,
            read_bytes: used_bytes,
            request_sha256,
            result_sha256,
        };
        self.record_retrieval_event(request, &result, actor)?;
        Ok(result)
    }

    fn scoped_current_memories(
        &self,
        scope: &MemoryScope,
    ) -> Result<ScopedMemoryInventory, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let state_excluded = sqlite_state_excluded_count(conn, scope)?;
                let mut stmt = conn.prepare(
                    "SELECT m.memory_id, m.version, m.tenant_id, m.workspace_id, m.agent_id, m.run_id, m.task_id, m.source_id, m.source_sha256, m.conflict_key, m.state, m.confidence, m.fresh_until, m.expires_at, m.supersedes_memory_id, m.content_json, m.embedding_json, m.embedding_provenance, m.embedding_metadata_json, m.embedding_binding_sha256, m.record_sha256, m.created_at, m.created_by
                     FROM durable_memory_versions m JOIN (SELECT memory_id, MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest ON latest.memory_id=m.memory_id AND latest.version=m.version
                     WHERE m.tenant_id=?1 AND m.workspace_id=?2 AND (m.agent_id IS NULL OR m.agent_id=?3) AND (m.task_id IS NULL OR m.task_id=?4)
                       AND m.state='current'
                     ORDER BY m.memory_id LIMIT 500")
                    .map_err(|error| error.to_string())?;
                let rows = stmt.query_map(params![scope.tenant_id, scope.workspace_id, scope.agent_id, scope.task_id], sqlite_memory_row).map_err(|error| error.to_string())?;
                let candidates = rows
                    .map(|row| row.map_err(|error| error.to_string()))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ScopedMemoryInventory::new(candidates, state_excluded))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let state_excluded = pg_state_excluded_count(client, scope)?;
                let candidates = client.query(
                    "SELECT m.memory_id, m.version, m.tenant_id, m.workspace_id, m.agent_id, m.run_id, m.task_id, m.source_id, m.source_sha256, m.conflict_key, m.state, m.confidence::DOUBLE PRECISION, m.fresh_until, m.expires_at, m.supersedes_memory_id, m.content_json, m.embedding_json, m.embedding_provenance, m.embedding_metadata_json, m.embedding_binding_sha256, m.record_sha256, m.created_at, m.created_by
                     FROM durable_memory_versions m JOIN (SELECT memory_id, MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest ON latest.memory_id=m.memory_id AND latest.version=m.version
                     WHERE m.tenant_id=$1 AND m.workspace_id=$2 AND (m.agent_id IS NULL OR m.agent_id=$3) AND (m.task_id IS NULL OR m.task_id=$4)
                       AND m.state='current'
                     ORDER BY m.memory_id LIMIT 500", &[&scope.tenant_id, &scope.workspace_id, &scope.agent_id, &scope.task_id])
                    .map_err(|error| error.to_string())?.iter().map(pg_memory_row).collect::<Result<Vec<_>, _>>()?;
                Ok(ScopedMemoryInventory::new(candidates, state_excluded))
            }),
        }
    }

    fn record_retrieval_event(
        &self,
        request: &MemoryRetrievalRequest,
        result: &MemoryRetrievalResult,
        actor: &str,
    ) -> Result<(), String> {
        let evidence = json!({
            "schema_version": MEMORY_RETRIEVAL_SCHEMA_VERSION,
            "retrieval_id": result.retrieval_id,
            "request_sha256": result.request_sha256,
            "result_sha256": result.result_sha256,
            "mode": result.mode,
            "embedding_provenance": result.embedding_provenance,
            "embedding_provider": result.embedding_provider,
            "candidates": result.candidate_count,
            "candidate_scores": result.candidate_scores,
            "candidate_scores_truncated": result.candidate_scores_truncated,
            "selected": retrieval_evidence(&result.selected),
            "stale_excluded": result.stale_excluded,
            "state_excluded": result.state_excluded,
            "truncated": result.truncated,
            "estimated_tokens": result.estimated_tokens,
            "token_estimate_provenance": result.token_estimate_provenance,
            "candidate_read_bytes": result.candidate_read_bytes,
            "read_bytes": result.read_bytes,
            "raw_content_stored": false,
        });
        let now = self.now();
        let truncated = i64::from(result.truncated);
        let candidate_count = result.candidate_count as i64;
        let selected_count = result.selected.len() as i64;
        let estimated_tokens = result.estimated_tokens as i64;
        let read_bytes = result.read_bytes as i64;
        let evidence_json = evidence.to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn.unchecked_transaction().map_err(|error| error.to_string())?;
                let changed = tx.execute("INSERT OR IGNORE INTO memory_retrieval_events (retrieval_id, tenant_id, workspace_id, run_id, node_id, agent_id, request_sha256, result_sha256, mode, candidate_count, selected_count, estimated_tokens, read_bytes, truncated, evidence_json, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)", params![result.retrieval_id, request.scope.tenant_id, request.scope.workspace_id, request.run_id, request.node_id, request.scope.agent_id, result.request_sha256, result.result_sha256, result.mode, candidate_count, selected_count, estimated_tokens, read_bytes, truncated, evidence_json, now]).map_err(|error| error.to_string())?;
                if changed == 1 { append_audit_locked(&tx, &now, actor, "durable_memory.retrieve", &format!("memory-retrieval/{}", result.retrieval_id), &evidence)?; }
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let changed = tx.execute("INSERT INTO memory_retrieval_events (retrieval_id, tenant_id, workspace_id, run_id, node_id, agent_id, request_sha256, result_sha256, mode, candidate_count, selected_count, estimated_tokens, read_bytes, truncated, evidence_json, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) ON CONFLICT(retrieval_id) DO NOTHING", &[&result.retrieval_id, &request.scope.tenant_id, &request.scope.workspace_id, &request.run_id, &request.node_id, &request.scope.agent_id, &result.request_sha256, &result.result_sha256, &result.mode, &candidate_count, &selected_count, &estimated_tokens, &read_bytes, &truncated, &evidence_json, &now]).map_err(|error| error.to_string())?;
                if changed == 1 { pg_append_audit(&mut tx, &now, actor, "durable_memory.retrieve", &format!("memory-retrieval/{}", result.retrieval_id), &evidence)?; }
                tx.commit().map_err(|error| error.to_string())
            }),
        }
    }
}

fn validate_create(request: &DurableMemoryCreate) -> Result<(), String> {
    validate_scope(&request.scope)?;
    validate_identifier(&request.source_id, "source_id")?;
    validate_identifier(&request.conflict_key, "conflict_key")?;
    validate_hash(&request.source_sha256, "source_sha256")?;
    validate_content(&request.content)?;
    validate_confidence(request.confidence)?;
    if request.supersedes_memory_id.is_some() {
        return Err("supersedes_memory_id requires the atomic supersede operation".to_string());
    }
    validate_time_bounds(
        request.fresh_until.as_deref(),
        request.expires_at.as_deref(),
    )
}

fn validate_revision(request: &DurableMemoryRevision) -> Result<(), String> {
    if request.expected_version < 1 {
        return Err("expected_version must be positive".to_string());
    }
    validate_identifier(&request.source_id, "source_id")?;
    validate_hash(&request.source_sha256, "source_sha256")?;
    validate_content(&request.content)?;
    validate_confidence(request.confidence)?;
    validate_time_bounds(
        request.fresh_until.as_deref(),
        request.expires_at.as_deref(),
    )
}

fn validate_retrieval(request: &MemoryRetrievalRequest) -> Result<(), String> {
    validate_scope(&request.scope)?;
    validate_identifier(&request.run_id, "run_id")?;
    validate_identifier(&request.node_id, "node_id")?;
    if request.query.is_empty() || request.query.len() > 16_384 {
        return Err("retrieval query is empty or oversized".to_string());
    }
    if request.top_k == 0 || request.top_k > MAX_TOP_K {
        return Err("retrieval top_k is outside 1..=20".to_string());
    }
    if request.max_tokens == 0 || request.max_tokens > 32_768 {
        return Err("retrieval token budget is invalid".to_string());
    }
    if request.max_bytes == 0 || request.max_bytes > 131_072 {
        return Err("retrieval byte budget is invalid".to_string());
    }
    Ok(())
}

fn validate_scope(scope: &MemoryScope) -> Result<(), String> {
    validate_identifier(&scope.tenant_id, "tenant_id")?;
    validate_identifier(&scope.workspace_id, "workspace_id")?;
    if let Some(value) = &scope.agent_id {
        validate_identifier(value, "agent_id")?;
    }
    if let Some(value) = &scope.task_id {
        validate_identifier(value, "task_id")?;
    }
    Ok(())
}

fn validate_identifier(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 256
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
    {
        return Err(format!("{field} is invalid"));
    }
    Ok(())
}

fn validate_hash(value: &str, field: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{field} must be a lowercase sha256"));
    }
    Ok(())
}

fn validate_content(content: &Value) -> Result<(), String> {
    let size = content.to_string().len();
    if content.is_null() || size == 0 || size > MAX_CONTENT_BYTES {
        return Err("durable memory content is empty or oversized".to_string());
    }
    Ok(())
}

fn validate_confidence(value: f64) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err("durable memory confidence must be in [0,1]".to_string());
    }
    Ok(())
}

fn validate_time_bounds(fresh_until: Option<&str>, expires_at: Option<&str>) -> Result<(), String> {
    let parse = |value: &str| {
        chrono::DateTime::parse_from_rfc3339(value)
            .map_err(|_| "durable memory timestamps must be RFC3339".to_string())
    };
    let fresh = fresh_until.map(parse).transpose()?;
    let expires = expires_at.map(parse).transpose()?;
    if let (Some(fresh), Some(expires)) = (fresh, expires) {
        if fresh > expires {
            return Err("fresh_until cannot be after expires_at".to_string());
        }
    }
    Ok(())
}

impl LocalProductStore {
    fn latest_memory_preflight(&self, memory_id: &str) -> Result<Option<StoredMemory>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,embedding_metadata_json,embedding_binding_sha256,record_sha256,created_at,created_by FROM durable_memory_versions WHERE memory_id=?1 ORDER BY version DESC LIMIT 1",
                    params![memory_id],
                    sqlite_memory_row,
                )
                .optional()
                .map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence::DOUBLE PRECISION,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,embedding_metadata_json,embedding_binding_sha256,record_sha256,created_at,created_by FROM durable_memory_versions WHERE memory_id=$1 ORDER BY version DESC LIMIT 1",
                        &[&memory_id],
                    )
                    .map_err(|error| error.to_string())?
                    .as_ref()
                    .map(pg_memory_row)
                    .transpose()
            }),
        }
    }

    fn embedding_for_text(
        &self,
        text: &str,
        operation_binding: Option<&EmbeddingOperationBinding<'_>>,
    ) -> Result<Option<EmbeddingMaterial>, String> {
        let mode = std::env::var("ACP_DURABLE_MEMORY_EMBEDDING_MODE")
            .unwrap_or_else(|_| "disabled".to_string());
        match mode.as_str() {
            "disabled" => Ok(None),
            "fixture" if cfg!(test) => Ok(Some(EmbeddingMaterial {
                values: local_embedding(text),
                provenance: "deterministic_fixture".to_string(),
                provider: None,
            })),
            "fixture" => Err("fixture embeddings are forbidden outside tests".to_string()),
            "local_hash_v1" => {
                if std::env::var("CI").is_ok() {
                    return Err("production embedding generation is disabled in CI".to_string());
                }
                if std::env::var("ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS").as_deref() != Ok("1") {
                    return Err("durable memory embedding gate is disabled".to_string());
                }
                Ok(Some(EmbeddingMaterial {
                    values: local_embedding(text),
                    provenance: "harness_derived".to_string(),
                    provider: None,
                }))
            }
            "provider" => self
                .provider_embedding_for_text(text, operation_binding)
                .map(Some),
            _ => Err("unsupported durable memory embedding mode".to_string()),
        }
    }

    fn provider_embedding_for_text(
        &self,
        text: &str,
        operation_binding: Option<&EmbeddingOperationBinding<'_>>,
    ) -> Result<EmbeddingMaterial, String> {
        let config = ProviderEmbeddingConfig::from_env()?;
        let inputs = [text.to_string()];
        validate_inputs(&inputs)?;
        let mut dispatch_suffix = operation_binding
            .map(|binding| binding.binding_sha256.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
        let mut dispatch_id = format!("memory-embedding-{dispatch_suffix}");
        let now = self.now();
        let contract = match self.embedding_client.verify_contract(&config) {
            Ok(contract) => contract,
            Err(error) => {
                self.record_embedding_error(&dispatch_id, &error, None)?;
                return Err(error);
            }
        };
        let reservation = ProviderAuditEvent {
            schema_version: "provider_audit_event.v1".to_string(),
            event_id: format!("paudit-reservation-{dispatch_suffix}"),
            dispatch_id: dispatch_id.clone(),
            provider_id: OPENROUTER_EMBEDDING_PROVIDER_ID.to_string(),
            event_type: "request_reserved".to_string(),
            input_token_count: None,
            output_token_count: None,
            cost: Some(0.0),
            currency: Some("USD".to_string()),
            latency_ms: None,
            error_domain: None,
            redaction_status: "redacted".to_string(),
            created_at: now.clone(),
        };
        let request_sent = ProviderAuditEvent {
            schema_version: "provider_audit_event.v1".to_string(),
            event_id: format!("paudit-request-{dispatch_suffix}"),
            dispatch_id: dispatch_id.clone(),
            provider_id: OPENROUTER_EMBEDDING_PROVIDER_ID.to_string(),
            event_type: "request_sent".to_string(),
            input_token_count: None,
            output_token_count: None,
            cost: Some(0.0),
            currency: Some("USD".to_string()),
            latency_ms: None,
            error_domain: None,
            redaction_status: "redacted".to_string(),
            created_at: now.clone(),
        };
        let contract_evidence = contract.evidence();
        validate_contract_evidence(&contract_evidence)?;
        let contract_json =
            serde_json::to_string(&contract_evidence).map_err(|error| error.to_string())?;
        let contract_sha256 = sha256_bytes(contract_json.as_bytes());
        let operation = operation_binding
            .map(|binding| {
                let mut operation = ProviderEmbeddingOperation {
                    operation_id: format!("embedding-operation-{}", binding.binding_sha256),
                    target_memory_id: binding.memory_id.to_string(),
                    target_version: binding.target_version,
                    tenant_id: binding.scope.tenant_id.clone(),
                    workspace_id: binding.scope.workspace_id.clone(),
                    agent_id: binding.scope.agent_id.clone(),
                    run_id: binding.run_id.map(str::to_string),
                    task_id: binding.scope.task_id.clone(),
                    source_id: binding.source_id.to_string(),
                    source_sha256: binding.source_sha256.to_string(),
                    operation_binding_sha256: binding.binding_sha256.to_string(),
                    content_sha256: normalized_content_sha256(text),
                    contract_json: contract_json.clone(),
                    contract_sha256: contract_sha256.clone(),
                    receipt_sha256: String::new(),
                    provider_id: OPENROUTER_EMBEDDING_PROVIDER_ID.to_string(),
                    model_id: OPENROUTER_EMBEDDING_MODEL_ID.to_string(),
                    created_at: now.clone(),
                };
                operation.receipt_sha256 = provider_embedding_operation_receipt_sha256(&operation)?;
                Ok::<ProviderEmbeddingOperation, String>(operation)
            })
            .transpose()?;
        if let Some(operation) = &operation {
            match self.claim_verified_free_embedding_operation(
                operation,
                &reservation,
                &request_sent,
                config.per_call_cap_usd,
                config.daily_cap_usd,
                &contract.pricing,
            )? {
                ProviderEmbeddingOperationClaim::Claimed { attempt_count } => {
                    if attempt_count > 1 {
                        dispatch_suffix = format!("{dispatch_suffix}-attempt-{attempt_count}");
                        dispatch_id = format!("{dispatch_id}-attempt-{attempt_count}");
                    }
                }
                ProviderEmbeddingOperationClaim::RetryAuthorized { .. } => {
                    return Err("provider embedding retry authorization was not claimed".to_string())
                }
                ProviderEmbeddingOperationClaim::Completed {
                    vector_json,
                    metadata_json,
                } => {
                    let values = serde_json::from_str(&vector_json).map_err(|_| {
                        "completed embedding vector receipt is malformed".to_string()
                    })?;
                    let metadata = serde_json::from_str(&metadata_json).map_err(|_| {
                        "completed embedding metadata receipt is malformed".to_string()
                    })?;
                    return Ok(EmbeddingMaterial {
                        values,
                        provenance: "provider_reported".to_string(),
                        provider: Some(metadata),
                    });
                }
            }
        } else {
            let claimed = self.reserve_verified_free_embedding_cost(
                &reservation,
                config.per_call_cap_usd,
                config.daily_cap_usd,
                &contract.pricing,
            )?;
            if !claimed {
                return Err(
                    "provider embedding operation is already reserved; refresh authoritative state"
                        .to_string(),
                );
            }
            self.record_provider_audit_event(&request_sent)?;
        }
        let started = std::time::Instant::now();
        let result = self
            .embedding_client
            .embed_verified(&inputs, &config, &contract);
        match result {
            Ok(mut output) => {
                let values = output
                    .vectors
                    .pop()
                    .ok_or_else(|| "embedding provider outcome has no vector".to_string())?;
                let metadata = output
                    .metadata
                    .pop()
                    .ok_or_else(|| "embedding provider outcome has no metadata".to_string())?;
                let event = ProviderAuditEvent {
                    schema_version: "provider_audit_event.v1".to_string(),
                    event_id: format!("paudit-response-{dispatch_suffix}"),
                    dispatch_id,
                    provider_id: OPENROUTER_EMBEDDING_PROVIDER_ID.to_string(),
                    event_type: "response_received".to_string(),
                    input_token_count: metadata.input_tokens,
                    output_token_count: None,
                    cost: metadata.cost_usd,
                    currency: Some(metadata.pricing.currency.clone()),
                    latency_ms: Some(started.elapsed().as_millis().min(i64::MAX as u128) as i64),
                    error_domain: None,
                    redaction_status: "redacted".to_string(),
                    created_at: self.now(),
                };
                if let Some(operation) = &operation {
                    let vector_json =
                        serde_json::to_string(&values).map_err(|error| error.to_string())?;
                    let metadata_json =
                        serde_json::to_string(&metadata).map_err(|error| error.to_string())?;
                    self.complete_provider_embedding_operation(
                        operation,
                        &vector_json,
                        &metadata_json,
                        &event,
                    )?;
                } else {
                    self.record_provider_audit_event(&event)?;
                }
                Ok(EmbeddingMaterial {
                    values,
                    provenance: "provider_reported".to_string(),
                    provider: Some(metadata),
                })
            }
            Err(error) => {
                let error_event = ProviderAuditEvent {
                    schema_version: "provider_audit_event.v1".to_string(),
                    event_id: format!("paudit-error-{dispatch_suffix}"),
                    dispatch_id: dispatch_id.clone(),
                    provider_id: OPENROUTER_EMBEDDING_PROVIDER_ID.to_string(),
                    event_type: "error".to_string(),
                    input_token_count: None,
                    output_token_count: None,
                    cost: None,
                    currency: Some("USD".to_string()),
                    latency_ms: Some(started.elapsed().as_millis().min(i64::MAX as u128) as i64),
                    error_domain: Some(provider_error_domain(&error).to_string()),
                    redaction_status: "redacted".to_string(),
                    created_at: self.now(),
                };
                if let Some(operation) = &operation {
                    self.fail_provider_embedding_operation(
                        operation,
                        error.contains("outcome unknown"),
                        &error_event,
                    )?;
                } else {
                    self.record_provider_audit_event(&error_event)?;
                }
                Err(error)
            }
        }
    }

    fn record_embedding_error(
        &self,
        dispatch_id: &str,
        error: &str,
        latency_ms: Option<i64>,
    ) -> Result<(), String> {
        self.record_provider_audit_event(&ProviderAuditEvent {
            schema_version: "provider_audit_event.v1".to_string(),
            event_id: format!("paudit-error-{}", uuid::Uuid::new_v4().simple()),
            dispatch_id: dispatch_id.to_string(),
            provider_id: OPENROUTER_EMBEDDING_PROVIDER_ID.to_string(),
            event_type: "error".to_string(),
            input_token_count: None,
            output_token_count: None,
            cost: None,
            currency: Some("USD".to_string()),
            latency_ms,
            error_domain: Some(provider_error_domain(error).to_string()),
            redaction_status: "redacted".to_string(),
            created_at: self.now(),
        })
    }
}

fn provider_error_domain(error: &str) -> &'static str {
    if error.contains("authentication") || error.contains("Credential") {
        "provider_auth"
    } else if error.contains("timed out") {
        "provider_timeout"
    } else if error.contains("outcome unknown") {
        "provider_outcome_unknown"
    } else if error.contains("circuit") {
        "provider_circuit_open"
    } else if error.contains("kill switch") {
        "provider_kill_switch"
    } else if error.contains("pricing") {
        "provider_pricing"
    } else {
        "provider_error"
    }
}

fn local_embedding(text: &str) -> Vec<f64> {
    let mut vector = vec![0.0; LOCAL_VECTOR_DIMENSIONS];
    for token in text
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
    {
        let digest = Sha256::digest(token.to_lowercase().as_bytes());
        let index = u16::from_be_bytes([digest[0], digest[1]]) as usize % LOCAL_VECTOR_DIMENSIONS;
        let sign = if digest[2] & 1 == 0 { 1.0 } else { -1.0 };
        vector[index] += sign;
    }
    let norm = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

fn cosine_similarity(left: &[f64], right: &[f64]) -> Result<f64, String> {
    if left.is_empty() || left.len() != right.len() || left.len() > MAX_VECTOR_DIMENSIONS {
        return Err("embedding dimensions do not match".to_string());
    }
    let dot = left.iter().zip(right).map(|(a, b)| a * b).sum::<f64>();
    let left_norm = left.iter().map(|value| value * value).sum::<f64>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f64>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        Ok(0.0)
    } else {
        Ok(dot / (left_norm * right_norm))
    }
}

fn lexical_similarity(query: &str, content: &str) -> f64 {
    let tokens = |value: &str| {
        value
            .to_lowercase()
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|token| !token.is_empty())
            .map(str::to_string)
            .collect::<BTreeSet<_>>()
    };
    let query = tokens(query);
    let content = tokens(content);
    if query.is_empty() || content.is_empty() {
        return 0.0;
    }
    let intersection = query.intersection(&content).count() as f64;
    intersection / query.union(&content).count() as f64
}

fn is_stale(memory: &StoredMemory, now: &str) -> bool {
    let Ok(now) = chrono::DateTime::parse_from_rfc3339(now) else {
        return true;
    };
    let reached = |value: &str, inclusive: bool| {
        chrono::DateTime::parse_from_rfc3339(value)
            .map(|value| if inclusive { value <= now } else { value < now })
            .unwrap_or(true)
    };
    memory
        .fresh_until
        .as_deref()
        .is_some_and(|value| reached(value, false))
        || memory
            .expires_at
            .as_deref()
            .is_some_and(|value| reached(value, true))
}

fn timestamp_at_or_before(value: &str, now: &str) -> bool {
    match (
        chrono::DateTime::parse_from_rfc3339(value),
        chrono::DateTime::parse_from_rfc3339(now),
    ) {
        (Ok(value), Ok(now)) => value <= now,
        _ => true,
    }
}

fn memory_id(request: &DurableMemoryCreate) -> Result<String, String> {
    let hash = sha256_json(
        &json!({"scope":request.scope,"source_id":request.source_id,"source_sha256":request.source_sha256}),
    )?;
    Ok(format!("mem-{}", &hash[..32]))
}

#[allow(clippy::too_many_arguments)]
fn build_record(
    memory_id: String,
    version: i64,
    scope: MemoryScope,
    run_id: Option<String>,
    source_id: String,
    source_sha256: String,
    conflict_key: String,
    state: &str,
    confidence: f64,
    fresh_until: Option<String>,
    expires_at: Option<String>,
    supersedes_memory_id: Option<String>,
    content: Value,
    embedding: Option<EmbeddingMaterial>,
    created_at: String,
    created_by: String,
) -> Result<StoredMemory, String> {
    let (embedding, embedding_provenance, embedding_metadata, embedding_binding_sha256) =
        if let Some(material) = embedding {
            validate_embedding_material(&material, &content)?;
            let metadata = material.provider.map(|provider| StoredEmbeddingMetadata {
                provider,
                tenant_id: scope.tenant_id.clone(),
                workspace_id: scope.workspace_id.clone(),
                agent_id: scope.agent_id.clone(),
                run_id: run_id.clone(),
                task_id: scope.task_id.clone(),
                memory_id: memory_id.clone(),
                memory_version: version,
                source_id: source_id.clone(),
                source_sha256: source_sha256.clone(),
            });
            let binding = metadata
                .as_ref()
                .map(embedding_binding_sha256)
                .transpose()?;
            (
                Some(material.values),
                material.provenance,
                metadata,
                binding,
            )
        } else {
            (None, "unavailable".to_string(), None, None)
        };
    let unsigned = json!({"schema_version":DURABLE_MEMORY_SCHEMA_VERSION,"memory_id":memory_id,"version":version,"scope":scope,"run_id":run_id,"source_id":source_id,"source_sha256":source_sha256,"conflict_key":conflict_key,"state":state,"confidence":confidence,"fresh_until":fresh_until,"expires_at":expires_at,"supersedes_memory_id":supersedes_memory_id,"content":content,"embedding":embedding,"embedding_provenance":embedding_provenance,"embedding_metadata":embedding_metadata,"embedding_binding_sha256":embedding_binding_sha256,"created_at":created_at,"created_by":created_by});
    let record_sha256 = sha256_json(&unsigned)?;
    Ok(StoredMemory {
        memory_id,
        version,
        scope,
        run_id,
        source_id,
        source_sha256,
        conflict_key,
        state: state.to_string(),
        confidence,
        fresh_until,
        expires_at,
        supersedes_memory_id,
        content,
        embedding,
        embedding_provenance,
        embedding_metadata,
        embedding_binding_sha256,
        record_sha256,
        created_at,
        created_by,
    })
}

fn stored_embedding_material(memory: &StoredMemory) -> Option<EmbeddingMaterial> {
    memory.embedding.clone().map(|values| EmbeddingMaterial {
        values,
        provenance: memory.embedding_provenance.clone(),
        provider: memory
            .embedding_metadata
            .as_ref()
            .map(|metadata| metadata.provider.clone()),
    })
}

fn validate_embedding_material(
    material: &EmbeddingMaterial,
    content: &Value,
) -> Result<(), String> {
    if material.values.is_empty() || material.values.len() > MAX_VECTOR_DIMENSIONS {
        return Err("embedding dimensions are invalid".to_string());
    }
    if material.values.iter().any(|value| !value.is_finite()) {
        return Err("embedding contains non-finite values".to_string());
    }
    match (&material.provider, material.provenance.as_str()) {
        (Some(provider), "provider_reported") => {
            if !crate::provider::embedding::is_supported_durable_embedding_identity(provider) {
                return Err("embedding provider identity mismatch".to_string());
            }
            if provider.dimensions != MAX_VECTOR_DIMENSIONS
                || material.values.len() != MAX_VECTOR_DIMENSIONS
            {
                return Err("embedding provider metadata dimension mismatch".to_string());
            }
            let normalized_content = content
                .to_string()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if provider.normalized_content_sha256 != sha256_bytes(normalized_content.as_bytes()) {
                return Err("embedding normalized content binding mismatch".to_string());
            }
            if provider.vector_sha256
                != sha256_json(
                    &serde_json::to_value(&material.values).map_err(|error| error.to_string())?,
                )?
            {
                return Err("embedding vector binding mismatch".to_string());
            }
            if provider.pricing.currency != "USD"
                || provider.pricing.source != "provider_catalog_reported"
                || provider.pricing.prompt_cost_per_token_usd != 0.0
                || provider.pricing.completion_cost_per_token_usd != 0.0
                || chrono::NaiveDate::parse_from_str(&provider.pricing.effective_date, "%Y-%m-%d")
                    .is_err()
                || provider.measurement_provenance != "provider_reported"
                || provider.input_tokens.is_some_and(|value| value < 0)
                || provider
                    .cost_usd
                    .is_some_and(|value| !value.is_finite() || value != 0.0)
            {
                return Err("embedding provider pricing binding is invalid".to_string());
            }
        }
        (None, "deterministic_fixture" | "harness_derived") => {}
        (Some(_), _) => return Err("provider embedding provenance is invalid".to_string()),
        (None, _) => return Err("embedding provider metadata is missing".to_string()),
    }
    Ok(())
}

fn validate_contract_evidence(contract: &EmbeddingContractEvidence) -> Result<(), String> {
    if contract.provider_id != OPENROUTER_EMBEDDING_PROVIDER_ID
        || contract.requested_model_id != OPENROUTER_EMBEDDING_MODEL_ID
        || contract.canonical_model_slug
            != crate::provider::embedding::OPENROUTER_EMBEDDING_CANONICAL_SLUG
        || contract.resolved_model_id
            != crate::provider::embedding::OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID
        || contract.dimensions != crate::provider::embedding::OPENROUTER_EMBEDDING_DIMENSIONS
        || contract.context_length
            != crate::provider::embedding::OPENROUTER_EMBEDDING_CONTEXT_LENGTH
        || contract.pricing.currency != "USD"
        || contract.pricing.source != "provider_catalog_reported"
        || contract.pricing.prompt_cost_per_token_usd != 0.0
        || contract.pricing.completion_cost_per_token_usd != 0.0
        || chrono::NaiveDate::parse_from_str(&contract.pricing.effective_date, "%Y-%m-%d").is_err()
    {
        return Err("provider embedding contract evidence is invalid".to_string());
    }
    Ok(())
}

fn embedding_binding_sha256(metadata: &StoredEmbeddingMetadata) -> Result<String, String> {
    sha256_json(&serde_json::to_value(metadata).map_err(|error| error.to_string())?)
}

fn validate_stored_embedding_binding(memory: &StoredMemory) -> Result<(), String> {
    match (
        &memory.embedding,
        &memory.embedding_metadata,
        &memory.embedding_binding_sha256,
    ) {
        (Some(values), Some(metadata), Some(binding)) => {
            if metadata.tenant_id != memory.scope.tenant_id
                || metadata.workspace_id != memory.scope.workspace_id
                || metadata.agent_id != memory.scope.agent_id
                || metadata.run_id != memory.run_id
                || metadata.task_id != memory.scope.task_id
                || metadata.memory_id != memory.memory_id
                || metadata.memory_version != memory.version
                || metadata.source_id != memory.source_id
                || metadata.source_sha256 != memory.source_sha256
            {
                return Err(
                    "stored provider embedding scope/source/version binding mismatch".to_string(),
                );
            }
            let material = EmbeddingMaterial {
                values: values.clone(),
                provenance: memory.embedding_provenance.clone(),
                provider: Some(metadata.provider.clone()),
            };
            validate_embedding_material(&material, &memory.content)?;
            if embedding_binding_sha256(metadata)? != *binding {
                return Err("stored provider embedding identity hash mismatch".to_string());
            }
            Ok(())
        }
        (Some(_), None, None)
            if matches!(
                memory.embedding_provenance.as_str(),
                "deterministic_fixture" | "harness_derived"
            ) =>
        {
            Ok(())
        }
        (None, None, None) if memory.embedding_provenance == "unavailable" => Ok(()),
        _ => Err("stored embedding metadata is incomplete".to_string()),
    }
}

fn require_compatible_embedding(
    query: &EmbeddingMaterial,
    candidate: &StoredMemory,
) -> Result<(), String> {
    validate_stored_embedding_binding(candidate)?;
    if query.provenance != candidate.embedding_provenance {
        return Err("retrieval embedding provenance mismatch".to_string());
    }
    match (&query.provider, &candidate.embedding_metadata) {
        (Some(query), Some(candidate))
            if query.provider_id == candidate.provider.provider_id
                && query.requested_model_id == candidate.provider.requested_model_id
                && query.canonical_model_slug == candidate.provider.canonical_model_slug
                && query.resolved_model_id == candidate.provider.resolved_model_id
                && query.dimensions == candidate.provider.dimensions =>
        {
            Ok(())
        }
        (None, None) => Ok(()),
        _ => Err("retrieval embedding model identity mismatch".to_string()),
    }
}

fn provider_retrieval_evidence(metadata: &ProviderEmbeddingMetadata) -> Value {
    json!({
        "provider_id": metadata.provider_id,
        "requested_model_id": metadata.requested_model_id,
        "canonical_model_slug": metadata.canonical_model_slug,
        "resolved_model_id": metadata.resolved_model_id,
        "dimensions": metadata.dimensions,
        "input_tokens": metadata.input_tokens,
        "cost_usd": metadata.cost_usd,
        "pricing": metadata.pricing,
        "measurement_provenance": metadata.measurement_provenance,
        "raw_content_stored": false,
    })
}

fn clone_with_state(
    prior: &StoredMemory,
    state: &str,
    now: &str,
    actor: &str,
) -> Result<StoredMemory, String> {
    build_record(
        prior.memory_id.clone(),
        prior.version + 1,
        prior.scope.clone(),
        prior.run_id.clone(),
        prior.source_id.clone(),
        prior.source_sha256.clone(),
        prior.conflict_key.clone(),
        state,
        prior.confidence,
        prior.fresh_until.clone(),
        prior.expires_at.clone(),
        prior.supersedes_memory_id.clone(),
        prior.content.clone(),
        stored_embedding_material(prior),
        now.to_string(),
        actor.to_string(),
    )
}

fn transition_record(
    prior: &StoredMemory,
    state: &str,
    now: &str,
    actor: &str,
) -> Result<StoredMemory, String> {
    match state {
        "invalid" if matches!(prior.state.as_str(), "current" | "conflicting") => {
            clone_with_state(prior, state, now, actor)
        }
        "expired" if prior.state == "current" => clone_with_state(prior, state, now, actor),
        "superseded" if matches!(prior.state.as_str(), "current" | "conflicting") => {
            clone_with_state(prior, state, now, actor)
        }
        "tombstoned" if prior.state != "tombstoned" => build_record(
            prior.memory_id.clone(),
            prior.version + 1,
            prior.scope.clone(),
            prior.run_id.clone(),
            prior.source_id.clone(),
            prior.source_sha256.clone(),
            prior.conflict_key.clone(),
            "tombstoned",
            prior.confidence,
            None,
            None,
            prior.supersedes_memory_id.clone(),
            json!({"forgotten": true}),
            None,
            now.to_string(),
            actor.to_string(),
        ),
        _ => Err(format!(
            "durable memory transition from {} to {state} is not allowed",
            prior.state
        )),
    }
}

fn revisable_state(prior: &StoredMemory) -> Result<String, String> {
    match prior.state.as_str() {
        "current" => Ok("current".to_string()),
        "conflicting" => Ok("conflicting".to_string()),
        state => Err(format!(
            "durable memory in terminal state {state} cannot be revised"
        )),
    }
}

fn clone_with_state_and_supersedes(
    prior: &StoredMemory,
    state: &str,
    supersedes_memory_id: Option<String>,
    now: &str,
    actor: &str,
) -> Result<StoredMemory, String> {
    build_record(
        prior.memory_id.clone(),
        prior.version + 1,
        prior.scope.clone(),
        prior.run_id.clone(),
        prior.source_id.clone(),
        prior.source_sha256.clone(),
        prior.conflict_key.clone(),
        state,
        prior.confidence,
        prior.fresh_until.clone(),
        prior.expires_at.clone(),
        supersedes_memory_id,
        prior.content.clone(),
        stored_embedding_material(prior),
        now.to_string(),
        actor.to_string(),
    )
}

fn resolve_supersede(
    winner: &StoredMemory,
    winner_expected_version: i64,
    loser: &StoredMemory,
    loser_expected_version: i64,
    now: &str,
    actor: &str,
) -> Result<(StoredMemory, StoredMemory), String> {
    require_version(winner, winner_expected_version)?;
    require_version(loser, loser_expected_version)?;
    if winner.scope != loser.scope || winner.conflict_key != loser.conflict_key {
        return Err(
            "durable memory supersede requires identical scope and conflict_key".to_string(),
        );
    }
    if !matches!(winner.state.as_str(), "current" | "conflicting")
        || !matches!(loser.state.as_str(), "current" | "conflicting")
    {
        return Err("durable memory supersede requires active facts".to_string());
    }
    let loser_next = clone_with_state(loser, "superseded", now, actor)?;
    let winner_next = clone_with_state_and_supersedes(
        winner,
        "current",
        Some(loser.memory_id.clone()),
        now,
        actor,
    )?;
    Ok((winner_next, loser_next))
}

fn supersede_audit(winner: &StoredMemory, loser: &StoredMemory) -> Value {
    json!({
        "winner_memory_id": winner.memory_id,
        "winner_version": winner.version,
        "winner_record_sha256": winner.record_sha256,
        "superseded_memory_id": loser.memory_id,
        "superseded_version": loser.version,
        "superseded_record_sha256": loser.record_sha256,
        "tenant_id": winner.scope.tenant_id,
        "workspace_id": winner.scope.workspace_id,
        "conflict_key": winner.conflict_key,
    })
}

fn require_version(memory: &StoredMemory, expected: i64) -> Result<(), String> {
    if memory.version != expected {
        return Err(format!(
            "durable memory version conflict: expected {expected}, current {}",
            memory.version
        ));
    }
    Ok(())
}

fn validate_idempotent_create(
    existing: &StoredMemory,
    request: &DurableMemoryCreate,
) -> Result<(), String> {
    if existing.source_id == request.source_id
        && existing.source_sha256 == request.source_sha256
        && existing.content == request.content
        && existing.scope == request.scope
        && existing.run_id == request.run_id
        && existing.conflict_key == request.conflict_key
        && existing.confidence == request.confidence
        && existing.fresh_until == request.fresh_until
        && existing.expires_at == request.expires_at
        && existing.supersedes_memory_id == request.supersedes_memory_id
    {
        Ok(())
    } else {
        Err("durable memory id collision".to_string())
    }
}

fn memory_to_value(memory: &StoredMemory) -> Result<Value, String> {
    Ok(
        json!({"schema_version":DURABLE_MEMORY_SCHEMA_VERSION,"memory_id":memory.memory_id,"version":memory.version,"scope":memory.scope,"run_id":memory.run_id,"source_id":memory.source_id,"source_sha256":memory.source_sha256,"conflict_key":memory.conflict_key,"state":memory.state,"confidence":memory.confidence,"fresh_until":memory.fresh_until,"expires_at":memory.expires_at,"supersedes_memory_id":memory.supersedes_memory_id,"content":memory.content,"embedding":{"present":memory.embedding.is_some(),"dimensions":memory.embedding.as_ref().map(Vec::len),"provenance":memory.embedding_provenance,"provider":memory.embedding_metadata.as_ref().map(|metadata| provider_retrieval_evidence(&metadata.provider)),"binding_sha256":memory.embedding_binding_sha256},"record_sha256":memory.record_sha256,"created_at":memory.created_at,"created_by":memory.created_by}),
    )
}

fn memory_audit(memory: &StoredMemory) -> Value {
    json!({"memory_id":memory.memory_id,"version":memory.version,"tenant_id":memory.scope.tenant_id,"workspace_id":memory.scope.workspace_id,"agent_id":memory.scope.agent_id,"task_id":memory.scope.task_id,"source_id":memory.source_id,"source_sha256":memory.source_sha256,"record_sha256":memory.record_sha256,"state":memory.state,"confidence":memory.confidence,"embedding_provenance":memory.embedding_provenance,"embedding_provider":memory.embedding_metadata.as_ref().map(|metadata| provider_retrieval_evidence(&metadata.provider)),"embedding_binding_sha256":memory.embedding_binding_sha256,"content_stored_in_audit":false})
}

fn memory_prune_reference(memory: &StoredMemory) -> Value {
    json!({
        "memory_id": memory.memory_id,
        "version": memory.version,
        "record_sha256": memory.record_sha256,
    })
}

fn retrieval_evidence(selected: &[MemoryReference]) -> Vec<Value> {
    selected.iter().map(|reference| json!({"memory_id":reference.memory_id,"version":reference.version,"source_id":reference.source_id,"source_sha256":reference.source_sha256,"record_sha256":reference.record_sha256,"score":reference.score,"confidence":reference.confidence,"estimated_tokens":reference.estimated_tokens,"content_bytes":reference.content_bytes})).collect()
}

fn candidate_score_evidence(score: f64, candidate: &StoredMemory) -> Value {
    json!({
        "memory_id": candidate.memory_id,
        "version": candidate.version,
        "source_id": candidate.source_id,
        "source_sha256": candidate.source_sha256,
        "record_sha256": candidate.record_sha256,
        "score": score,
        "confidence": candidate.confidence,
        "raw_content_stored": false,
    })
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn sha256_json(value: &Value) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|error| error.to_string())
}

fn sqlite_insert_memory(conn: &rusqlite::Connection, memory: &StoredMemory) -> Result<(), String> {
    let content_json = memory.content.to_string();
    let embedding_json = memory
        .embedding
        .as_ref()
        .map(|value| serde_json::to_string(value).unwrap());
    let embedding_metadata_json = memory
        .embedding_metadata
        .as_ref()
        .map(|value| serde_json::to_string(value).unwrap());
    conn.execute("INSERT INTO durable_memory_versions (memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,embedding_metadata_json,embedding_binding_sha256,record_sha256,created_at,created_by) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)", params![memory.memory_id,memory.version,memory.scope.tenant_id,memory.scope.workspace_id,memory.scope.agent_id,memory.run_id,memory.scope.task_id,memory.source_id,memory.source_sha256,memory.conflict_key,memory.state,memory.confidence,memory.fresh_until,memory.expires_at,memory.supersedes_memory_id,content_json,embedding_json,memory.embedding_provenance,embedding_metadata_json,memory.embedding_binding_sha256,memory.record_sha256,memory.created_at,memory.created_by]).map_err(|error| error.to_string())?;
    Ok(())
}

fn sqlite_latest_memory(
    conn: &rusqlite::Connection,
    memory_id: &str,
) -> Result<Option<StoredMemory>, String> {
    conn.query_row("SELECT memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,embedding_metadata_json,embedding_binding_sha256,record_sha256,created_at,created_by FROM durable_memory_versions WHERE memory_id=?1 ORDER BY version DESC LIMIT 1", params![memory_id], sqlite_memory_row).optional().map_err(|error| error.to_string())
}

fn enforce_sqlite_workspace_retention(
    conn: &rusqlite::Connection,
    scope: &MemoryScope,
) -> Result<(), String> {
    let active: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM durable_memory_versions m
             JOIN (SELECT memory_id, MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest
               ON latest.memory_id=m.memory_id AND latest.version=m.version
             WHERE m.tenant_id=?1 AND m.workspace_id=?2 AND m.state IN ('current','conflicting')",
            params![scope.tenant_id, scope.workspace_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active >= MAX_ACTIVE_MEMORIES_PER_WORKSPACE {
        return Err(format!(
            "durable memory workspace retention limit reached ({MAX_ACTIVE_MEMORIES_PER_WORKSPACE})"
        ));
    }
    Ok(())
}

fn sqlite_state_excluded_count(
    conn: &rusqlite::Connection,
    scope: &MemoryScope,
) -> Result<usize, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM durable_memory_versions m
             JOIN (SELECT memory_id, MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest
               ON latest.memory_id=m.memory_id AND latest.version=m.version
             WHERE m.tenant_id=?1 AND m.workspace_id=?2
               AND (m.agent_id IS NULL OR m.agent_id=?3)
               AND (m.task_id IS NULL OR m.task_id=?4)
               AND m.state<>'current'",
            params![
                scope.tenant_id,
                scope.workspace_id,
                scope.agent_id,
                scope.task_id
            ],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    usize::try_from(count).map_err(|_| "durable memory exclusion count overflow".to_string())
}

fn sqlite_expired_memories(
    conn: &rusqlite::Connection,
    scope: &MemoryScope,
    now: &str,
) -> Result<Vec<StoredMemory>, String> {
    let mut stmt = conn.prepare(
        "SELECT m.memory_id,m.version,m.tenant_id,m.workspace_id,m.agent_id,m.run_id,m.task_id,m.source_id,m.source_sha256,m.conflict_key,m.state,m.confidence,m.fresh_until,m.expires_at,m.supersedes_memory_id,m.content_json,m.embedding_json,m.embedding_provenance,m.embedding_metadata_json,m.embedding_binding_sha256,m.record_sha256,m.created_at,m.created_by
         FROM durable_memory_versions m
         JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest
           ON latest.memory_id=m.memory_id AND latest.version=m.version
         WHERE m.tenant_id=?1 AND m.workspace_id=?2
           AND m.agent_id IS ?3 AND m.task_id IS ?4
           AND m.state='current' AND m.expires_at IS NOT NULL
         ORDER BY m.memory_id LIMIT ?5",
    ).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(
            params![
                scope.tenant_id,
                scope.workspace_id,
                scope.agent_id,
                scope.task_id,
                MAX_CANDIDATES as i64
            ],
            sqlite_memory_row,
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .filter(|memory| {
            memory
                .expires_at
                .as_deref()
                .is_some_and(|value| timestamp_at_or_before(value, now))
        })
        .take(MAX_PRUNE_BATCH as usize)
        .collect())
}

fn sqlite_memory_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredMemory> {
    let content: String = row.get(15)?;
    let embedding: Option<String> = row.get(16)?;
    let embedding_metadata: Option<String> = row.get(18)?;
    let content = serde_json::from_str(&content).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(15, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let embedding = embedding
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    16,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()?;
    let embedding_metadata = embedding_metadata
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    18,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()?;
    let memory = StoredMemory {
        memory_id: row.get(0)?,
        version: row.get(1)?,
        scope: MemoryScope {
            tenant_id: row.get(2)?,
            workspace_id: row.get(3)?,
            agent_id: row.get(4)?,
            task_id: row.get(6)?,
        },
        run_id: row.get(5)?,
        source_id: row.get(7)?,
        source_sha256: row.get(8)?,
        conflict_key: row.get(9)?,
        state: row.get(10)?,
        confidence: row.get(11)?,
        fresh_until: row.get(12)?,
        expires_at: row.get(13)?,
        supersedes_memory_id: row.get(14)?,
        content,
        embedding,
        embedding_provenance: row.get(17)?,
        embedding_metadata,
        embedding_binding_sha256: row.get(19)?,
        record_sha256: row.get(20)?,
        created_at: row.get(21)?,
        created_by: row.get(22)?,
    };
    validate_stored_embedding_binding(&memory).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            18,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        )
    })?;
    Ok(memory)
}

fn sqlite_has_active_conflict(
    conn: &rusqlite::Connection,
    request: &DurableMemoryCreate,
) -> Result<bool, String> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM durable_memory_versions m JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest ON latest.memory_id=m.memory_id AND latest.version=m.version WHERE m.tenant_id=?1 AND m.workspace_id=?2 AND m.agent_id IS ?3 AND m.task_id IS ?4 AND m.conflict_key=?5 AND m.state IN ('current','conflicting') AND m.source_sha256<>?6)", params![request.scope.tenant_id,request.scope.workspace_id,request.scope.agent_id,request.scope.task_id,request.conflict_key,request.source_sha256], |row| row.get(0)).map_err(|error| error.to_string())
}

fn sqlite_conflict_count_for_request(
    conn: &rusqlite::Connection,
    request: &DurableMemoryCreate,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM durable_memory_versions m
         JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest
           ON latest.memory_id=m.memory_id AND latest.version=m.version
         WHERE m.tenant_id=?1 AND m.workspace_id=?2 AND m.agent_id IS ?3 AND m.task_id IS ?4
           AND m.conflict_key=?5 AND m.state IN ('current','conflicting')",
        params![
            request.scope.tenant_id,
            request.scope.workspace_id,
            request.scope.agent_id,
            request.scope.task_id,
            request.conflict_key
        ],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

fn require_conflict_capacity(active_count: i64) -> Result<(), String> {
    if active_count < 2 {
        Ok(())
    } else {
        Err("durable memory conflict set is full; resolve the existing pair before adding another fact".to_string())
    }
}

fn sqlite_active_conflict_count(
    conn: &rusqlite::Connection,
    memory: &StoredMemory,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM durable_memory_versions m
         JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest
           ON latest.memory_id=m.memory_id AND latest.version=m.version
         WHERE m.tenant_id=?1 AND m.workspace_id=?2 AND m.agent_id IS ?3 AND m.task_id IS ?4
           AND m.conflict_key=?5 AND m.state IN ('current','conflicting')",
        params![
            memory.scope.tenant_id,
            memory.scope.workspace_id,
            memory.scope.agent_id,
            memory.scope.task_id,
            memory.conflict_key
        ],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

fn require_complete_conflict_pair(active_count: i64) -> Result<(), String> {
    if active_count == 2 {
        Ok(())
    } else {
        Err(format!(
            "durable memory supersede requires exactly two active conflicting facts; found {active_count}"
        ))
    }
}

fn sqlite_mark_conflicts(
    conn: &rusqlite::Connection,
    request: &DurableMemoryCreate,
    actor: &str,
    now: &str,
) -> Result<(), String> {
    let mut stmt=conn.prepare("SELECT m.memory_id,m.version,m.tenant_id,m.workspace_id,m.agent_id,m.run_id,m.task_id,m.source_id,m.source_sha256,m.conflict_key,m.state,m.confidence,m.fresh_until,m.expires_at,m.supersedes_memory_id,m.content_json,m.embedding_json,m.embedding_provenance,m.embedding_metadata_json,m.embedding_binding_sha256,m.record_sha256,m.created_at,m.created_by FROM durable_memory_versions m JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest ON latest.memory_id=m.memory_id AND latest.version=m.version WHERE m.tenant_id=?1 AND m.workspace_id=?2 AND m.agent_id IS ?3 AND m.task_id IS ?4 AND m.conflict_key=?5 AND m.state='current' AND m.source_sha256<>?6 ORDER BY m.memory_id").map_err(|error|error.to_string())?;
    let rows = stmt
        .query_map(
            params![
                request.scope.tenant_id,
                request.scope.workspace_id,
                request.scope.agent_id,
                request.scope.task_id,
                request.conflict_key,
                request.source_sha256
            ],
            sqlite_memory_row,
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for prior in rows {
        sqlite_insert_memory(conn, &clone_with_state(&prior, "conflicting", now, actor)?)?;
    }
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_lock_memory(tx: &mut postgres::Transaction<'_>, memory_id: &str) -> Result<(), String> {
    tx.execute("SELECT pg_advisory_xact_lock(hashtext($1))", &[&memory_id])
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(feature = "pg")]
fn pg_insert_memory(
    tx: &mut postgres::Transaction<'_>,
    memory: &StoredMemory,
) -> Result<(), String> {
    let content = memory.content.to_string();
    let embedding = memory
        .embedding
        .as_ref()
        .map(|value| serde_json::to_string(value).unwrap());
    let embedding_metadata = memory
        .embedding_metadata
        .as_ref()
        .map(|value| serde_json::to_string(value).unwrap());
    tx.execute("INSERT INTO durable_memory_versions (memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,embedding_metadata_json,embedding_binding_sha256,record_sha256,created_at,created_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23)",&[&memory.memory_id,&memory.version,&memory.scope.tenant_id,&memory.scope.workspace_id,&memory.scope.agent_id,&memory.run_id,&memory.scope.task_id,&memory.source_id,&memory.source_sha256,&memory.conflict_key,&memory.state,&memory.confidence,&memory.fresh_until,&memory.expires_at,&memory.supersedes_memory_id,&content,&embedding,&memory.embedding_provenance,&embedding_metadata,&memory.embedding_binding_sha256,&memory.record_sha256,&memory.created_at,&memory.created_by]).map_err(|error|error.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_memory_row(row: &postgres::Row) -> Result<StoredMemory, String> {
    let content: String = row.get(15);
    let embedding: Option<String> = row.get(16);
    let embedding_metadata: Option<String> = row.get(18);
    let memory = StoredMemory {
        memory_id: row.get(0),
        version: row.get(1),
        scope: MemoryScope {
            tenant_id: row.get(2),
            workspace_id: row.get(3),
            agent_id: row.get(4),
            task_id: row.get(6),
        },
        run_id: row.get(5),
        source_id: row.get(7),
        source_sha256: row.get(8),
        conflict_key: row.get(9),
        state: row.get(10),
        confidence: row.get(11),
        fresh_until: row.get(12),
        expires_at: row.get(13),
        supersedes_memory_id: row.get(14),
        content: serde_json::from_str(&content).map_err(|error| error.to_string())?,
        embedding: embedding
            .map(|value| serde_json::from_str(&value).map_err(|error| error.to_string()))
            .transpose()?,
        embedding_provenance: row.get(17),
        embedding_metadata: embedding_metadata
            .map(|value| serde_json::from_str(&value).map_err(|error| error.to_string()))
            .transpose()?,
        embedding_binding_sha256: row.get(19),
        record_sha256: row.get(20),
        created_at: row.get(21),
        created_by: row.get(22),
    };
    validate_stored_embedding_binding(&memory)?;
    Ok(memory)
}

#[cfg(feature = "pg")]
fn pg_latest_memory(
    tx: &mut postgres::Transaction<'_>,
    memory_id: &str,
) -> Result<Option<StoredMemory>, String> {
    tx.query_opt("SELECT memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence::DOUBLE PRECISION,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,embedding_metadata_json,embedding_binding_sha256,record_sha256,created_at,created_by FROM durable_memory_versions WHERE memory_id=$1 ORDER BY version DESC LIMIT 1",&[&memory_id]).map_err(|error|error.to_string())?.as_ref().map(pg_memory_row).transpose()
}

#[cfg(feature = "pg")]
fn pg_latest_memory_for_update(
    tx: &mut postgres::Transaction<'_>,
    memory_id: &str,
) -> Result<Option<StoredMemory>, String> {
    tx.query_opt("SELECT memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence::DOUBLE PRECISION,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,embedding_metadata_json,embedding_binding_sha256,record_sha256,created_at,created_by FROM durable_memory_versions WHERE memory_id=$1 ORDER BY version DESC LIMIT 1 FOR UPDATE",&[&memory_id]).map_err(|error|error.to_string())?.as_ref().map(pg_memory_row).transpose()
}

#[cfg(feature = "pg")]
fn enforce_pg_workspace_retention(
    tx: &mut postgres::Transaction<'_>,
    scope: &MemoryScope,
) -> Result<(), String> {
    let active: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM durable_memory_versions m
             JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest
               ON latest.memory_id=m.memory_id AND latest.version=m.version
             WHERE m.tenant_id=$1 AND m.workspace_id=$2 AND m.state IN ('current','conflicting')",
            &[&scope.tenant_id, &scope.workspace_id],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    if active >= MAX_ACTIVE_MEMORIES_PER_WORKSPACE {
        return Err(format!(
            "durable memory workspace retention limit reached ({MAX_ACTIVE_MEMORIES_PER_WORKSPACE})"
        ));
    }
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_state_excluded_count(
    client: &mut postgres::Client,
    scope: &MemoryScope,
) -> Result<usize, String> {
    let count: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM durable_memory_versions m
             JOIN (SELECT memory_id, MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest
               ON latest.memory_id=m.memory_id AND latest.version=m.version
             WHERE m.tenant_id=$1 AND m.workspace_id=$2
               AND (m.agent_id IS NULL OR m.agent_id=$3)
               AND (m.task_id IS NULL OR m.task_id=$4)
               AND m.state<>'current'",
            &[
                &scope.tenant_id,
                &scope.workspace_id,
                &scope.agent_id,
                &scope.task_id,
            ],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    usize::try_from(count).map_err(|_| "durable memory exclusion count overflow".to_string())
}

#[cfg(feature = "pg")]
fn pg_expired_memories(
    tx: &mut postgres::Transaction<'_>,
    scope: &MemoryScope,
    now: &str,
) -> Result<Vec<StoredMemory>, String> {
    tx.query(
        "SELECT m.memory_id,m.version,m.tenant_id,m.workspace_id,m.agent_id,m.run_id,m.task_id,m.source_id,m.source_sha256,m.conflict_key,m.state,m.confidence::DOUBLE PRECISION,m.fresh_until,m.expires_at,m.supersedes_memory_id,m.content_json,m.embedding_json,m.embedding_provenance,m.embedding_metadata_json,m.embedding_binding_sha256,m.record_sha256,m.created_at,m.created_by
         FROM durable_memory_versions m
         JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest
           ON latest.memory_id=m.memory_id AND latest.version=m.version
         WHERE m.tenant_id=$1 AND m.workspace_id=$2
           AND m.agent_id IS NOT DISTINCT FROM $3 AND m.task_id IS NOT DISTINCT FROM $4
           AND m.state='current' AND m.expires_at IS NOT NULL
         ORDER BY m.memory_id LIMIT $5 FOR UPDATE OF m",
        &[
            &scope.tenant_id,
            &scope.workspace_id,
            &scope.agent_id,
            &scope.task_id,
            &(MAX_CANDIDATES as i64),
        ],
    )
    .map_err(|error| error.to_string())?
    .iter()
    .map(pg_memory_row)
    .collect::<Result<Vec<_>, _>>()
    .map(|rows| {
        rows.into_iter()
            .filter(|memory| {
                memory
                    .expires_at
                    .as_deref()
                    .is_some_and(|value| timestamp_at_or_before(value, now))
            })
            .take(MAX_PRUNE_BATCH as usize)
            .collect()
    })
}

#[cfg(feature = "pg")]
fn pg_has_active_conflict(
    tx: &mut postgres::Transaction<'_>,
    request: &DurableMemoryCreate,
) -> Result<bool, String> {
    Ok(tx.query_one("SELECT EXISTS(SELECT 1 FROM durable_memory_versions m JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest ON latest.memory_id=m.memory_id AND latest.version=m.version WHERE m.tenant_id=$1 AND m.workspace_id=$2 AND m.agent_id IS NOT DISTINCT FROM $3 AND m.task_id IS NOT DISTINCT FROM $4 AND m.conflict_key=$5 AND m.state IN ('current','conflicting') AND m.source_sha256<>$6)",&[&request.scope.tenant_id,&request.scope.workspace_id,&request.scope.agent_id,&request.scope.task_id,&request.conflict_key,&request.source_sha256]).map_err(|error|error.to_string())?.get(0))
}

#[cfg(feature = "pg")]
fn pg_conflict_count_for_request(
    tx: &mut postgres::Transaction<'_>,
    request: &DurableMemoryCreate,
) -> Result<i64, String> {
    Ok(tx
        .query_one(
            "SELECT COUNT(*) FROM durable_memory_versions m
             JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest
               ON latest.memory_id=m.memory_id AND latest.version=m.version
             WHERE m.tenant_id=$1 AND m.workspace_id=$2
               AND m.agent_id IS NOT DISTINCT FROM $3 AND m.task_id IS NOT DISTINCT FROM $4
               AND m.conflict_key=$5 AND m.state IN ('current','conflicting')",
            &[
                &request.scope.tenant_id,
                &request.scope.workspace_id,
                &request.scope.agent_id,
                &request.scope.task_id,
                &request.conflict_key,
            ],
        )
        .map_err(|error| error.to_string())?
        .get(0))
}

#[cfg(feature = "pg")]
fn pg_active_conflict_count(
    tx: &mut postgres::Transaction<'_>,
    memory: &StoredMemory,
) -> Result<i64, String> {
    Ok(tx
        .query_one(
            "SELECT COUNT(*) FROM durable_memory_versions m
             JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest
               ON latest.memory_id=m.memory_id AND latest.version=m.version
             WHERE m.tenant_id=$1 AND m.workspace_id=$2
               AND m.agent_id IS NOT DISTINCT FROM $3 AND m.task_id IS NOT DISTINCT FROM $4
               AND m.conflict_key=$5 AND m.state IN ('current','conflicting')",
            &[
                &memory.scope.tenant_id,
                &memory.scope.workspace_id,
                &memory.scope.agent_id,
                &memory.scope.task_id,
                &memory.conflict_key,
            ],
        )
        .map_err(|error| error.to_string())?
        .get(0))
}

#[cfg(feature = "pg")]
fn pg_mark_conflicts(
    tx: &mut postgres::Transaction<'_>,
    request: &DurableMemoryCreate,
    actor: &str,
    now: &str,
) -> Result<(), String> {
    let rows=tx.query("SELECT m.memory_id,m.version,m.tenant_id,m.workspace_id,m.agent_id,m.run_id,m.task_id,m.source_id,m.source_sha256,m.conflict_key,m.state,m.confidence::DOUBLE PRECISION,m.fresh_until,m.expires_at,m.supersedes_memory_id,m.content_json,m.embedding_json,m.embedding_provenance,m.embedding_metadata_json,m.embedding_binding_sha256,m.record_sha256,m.created_at,m.created_by FROM durable_memory_versions m JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest ON latest.memory_id=m.memory_id AND latest.version=m.version WHERE m.tenant_id=$1 AND m.workspace_id=$2 AND m.agent_id IS NOT DISTINCT FROM $3 AND m.task_id IS NOT DISTINCT FROM $4 AND m.conflict_key=$5 AND m.state='current' AND m.source_sha256<>$6 ORDER BY m.memory_id FOR UPDATE OF m",&[&request.scope.tenant_id,&request.scope.workspace_id,&request.scope.agent_id,&request.scope.task_id,&request.conflict_key,&request.source_sha256]).map_err(|error|error.to_string())?;
    for row in &rows {
        pg_insert_memory(
            tx,
            &clone_with_state(&pg_memory_row(row)?, "conflicting", now, actor)?,
        )?;
    }
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_append_audit(
    tx: &mut postgres::Transaction<'_>,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> Result<(), String> {
    tx.execute("INSERT INTO audit_log (created_at,actor,action,resource,details_json) VALUES ($1,$2,$3,$4,$5)",&[&now,&actor,&action,&resource,&details.to_string()]).map_err(|error|error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::embedding::{
        OPENROUTER_EMBEDDING_CANONICAL_SLUG, OPENROUTER_EMBEDDING_CONTEXT_LENGTH,
        OPENROUTER_EMBEDDING_DIMENSIONS, OPENROUTER_EMBEDDING_MODEL_ID,
        OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,
    };
    use crate::provider::transport::{
        HttpError, HttpRequest, HttpResponse, HttpTransport, MockTransport,
    };
    use crate::storage::backup_manager::BackupManager;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::sync::MutexGuard;
    use tempfile::TempDir;

    struct EmbeddingFixtureGuard {
        _lock: MutexGuard<'static, ()>,
    }
    fn embedding_env_lock() -> MutexGuard<'static, ()> {
        crate::provider::embedding::EMBEDDING_TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    impl Drop for EmbeddingFixtureGuard {
        fn drop(&mut self) {
            std::env::remove_var("ACP_DURABLE_MEMORY_EMBEDDING_MODE");
        }
    }
    fn embedding_fixture() -> EmbeddingFixtureGuard {
        let lock = embedding_env_lock();
        std::env::set_var("ACP_DURABLE_MEMORY_EMBEDDING_MODE", "fixture");
        EmbeddingFixtureGuard { _lock: lock }
    }
    fn embedding_disabled() -> EmbeddingFixtureGuard {
        let lock = embedding_env_lock();
        std::env::set_var("ACP_DURABLE_MEMORY_EMBEDDING_MODE", "disabled");
        EmbeddingFixtureGuard { _lock: lock }
    }

    struct ProviderEnvGuard {
        _lock: MutexGuard<'static, ()>,
        prior: Vec<(&'static str, Option<String>)>,
    }
    impl ProviderEnvGuard {
        fn enabled() -> Self {
            let lock = embedding_env_lock();
            let keys = [
                "CI",
                "OPENROUTER_API_KEY",
                "ACP_DURABLE_MEMORY_EMBEDDING_MODE",
                "ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS",
                "ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH",
                "ACP_ENABLE_PROVIDER_EXECUTION",
                "ACP_REQUIRE_AUTH",
            ];
            let prior = keys
                .into_iter()
                .map(|key| (key, std::env::var(key).ok()))
                .collect();
            std::env::remove_var("CI");
            std::env::set_var("OPENROUTER_API_KEY", "fixture-credential");
            std::env::set_var("ACP_DURABLE_MEMORY_EMBEDDING_MODE", "provider");
            std::env::set_var("ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS", "1");
            std::env::set_var("ACP_ENABLE_PROVIDER_EXECUTION", "1");
            std::env::set_var("ACP_REQUIRE_AUTH", "1");
            std::env::remove_var("ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH");
            Self { _lock: lock, prior }
        }
    }
    impl Drop for ProviderEnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.prior.drain(..) {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }

    fn provider_catalog_response() -> HttpResponse {
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"data":[{
                "id":OPENROUTER_EMBEDDING_MODEL_ID,
                "canonical_slug":OPENROUTER_EMBEDDING_CANONICAL_SLUG,
                "context_length":OPENROUTER_EMBEDDING_CONTEXT_LENGTH,
                "pricing":{"prompt":"0","completion":"0"},
                "architecture":{"input_modalities":["text","image"],"output_modalities":["embeddings"]}
            }]}))
            .unwrap(),
        }
    }

    fn provider_vector_response() -> HttpResponse {
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({
                "model":OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,
                "data":[{"index":0,"embedding":vec![0.125;OPENROUTER_EMBEDDING_DIMENSIONS]}],
                "usage":{"prompt_tokens":5}
            }))
            .unwrap(),
        }
    }

    fn provider_responses(
        call_count: usize,
    ) -> Vec<Result<HttpResponse, crate::provider::transport::HttpError>> {
        (0..call_count)
            .flat_map(|_| {
                [
                    Ok(provider_catalog_response()),
                    Ok(provider_vector_response()),
                ]
            })
            .collect()
    }

    struct CountingEmbeddingTransport {
        posts: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl HttpTransport for CountingEmbeddingTransport {
        async fn send(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
            if request.url.ends_with("/embeddings/models") {
                Ok(provider_catalog_response())
            } else if request.url.ends_with("/embeddings") && request.method == "POST" {
                self.posts.fetch_add(1, Ordering::SeqCst);
                Ok(provider_vector_response())
            } else {
                Err(HttpError::Connection(
                    "unexpected fixture endpoint".to_string(),
                ))
            }
        }
    }

    fn store() -> (TempDir, LocalProductStore) {
        let dir = TempDir::new().unwrap();
        let store = LocalProductStore::new(dir.path().join("memory.db")).unwrap();
        (dir, store)
    }
    fn scope(workspace: &str) -> MemoryScope {
        MemoryScope {
            tenant_id: "local".into(),
            workspace_id: workspace.into(),
            agent_id: Some("agent-1".into()),
            task_id: None,
        }
    }
    fn create(workspace: &str, source: &str, content: &str) -> DurableMemoryCreate {
        DurableMemoryCreate {
            scope: scope(workspace),
            run_id: Some("run-1".into()),
            source_id: source.into(),
            source_sha256: sha256_bytes(content.as_bytes()),
            conflict_key: "fact-1".into(),
            content: json!({"text":content}),
            confidence: 0.9,
            fresh_until: None,
            expires_at: None,
            supersedes_memory_id: None,
        }
    }

    #[test]
    fn version_conflict_and_tombstone_are_fail_closed() {
        let _embedding = embedding_fixture();
        let (_dir, store) = store();
        let first = store
            .create_durable_memory(&create("ws-a", "source-a", "alpha fact"), "test")
            .unwrap();
        let id = first["memory_id"].as_str().unwrap();
        assert!(store
            .revise_durable_memory(
                id,
                &DurableMemoryRevision {
                    expected_version: 2,
                    source_id: "source-b".into(),
                    source_sha256: sha256_bytes(b"beta"),
                    content: json!({"text":"beta"}),
                    confidence: 1.0,
                    fresh_until: None,
                    expires_at: None
                },
                "test"
            )
            .unwrap_err()
            .contains("version conflict"));
        let tombstone = store.forget_durable_memory(id, 1, "test").unwrap();
        assert_eq!(tombstone["state"], "tombstoned");
        assert_eq!(tombstone["content"], json!({"forgotten": true}));
        assert_eq!(tombstone["embedding"]["present"], false);
        let history = store.inspect_durable_memory(id).unwrap();
        assert_eq!(
            history.len(),
            1,
            "forget must erase all prior payload versions"
        );
        assert_eq!(history[0]["content"], json!({"forgotten": true}));
        assert!(store
            .revise_durable_memory(
                id,
                &DurableMemoryRevision {
                    expected_version: 2,
                    source_id: "source-c".into(),
                    source_sha256: sha256_bytes(b"gamma"),
                    content: json!({"text":"gamma"}),
                    confidence: 1.0,
                    fresh_until: None,
                    expires_at: None,
                },
                "test",
            )
            .unwrap_err()
            .contains("terminal state"));
    }

    #[test]
    fn semantic_retrieval_is_bounded_and_scope_isolated() {
        let _embedding = embedding_fixture();
        let (_dir, store) = store();
        store
            .create_durable_memory(&create("ws-a", "source-a", "rust scheduler lease"), "test")
            .unwrap();
        store
            .create_durable_memory(
                &create("ws-b", "source-b", "secret other workspace"),
                "test",
            )
            .unwrap();
        let result = store
            .retrieve_durable_memories(
                &MemoryRetrievalRequest {
                    scope: scope("ws-a"),
                    run_id: "run-2".into(),
                    node_id: "node-1".into(),
                    query: "scheduler lease".into(),
                    top_k: 5,
                    max_tokens: 100,
                    max_bytes: 1000,
                    allow_lexical_fallback: false,
                },
                "test",
            )
            .unwrap();
        assert_eq!(result.mode, "semantic_vector");
        assert_eq!(result.selected.len(), 1);
        assert_eq!(result.selected[0].source_id, "source-a");
        assert!(result.estimated_tokens <= 100);
    }

    #[test]
    fn conflicts_are_not_retrieved_as_truth() {
        let _embedding = embedding_fixture();
        let (_dir, store) = store();
        store
            .create_durable_memory(&create("ws-a", "source-a", "value one"), "test")
            .unwrap();
        store
            .create_durable_memory(&create("ws-a", "source-b", "value two"), "test")
            .unwrap();
        let third = store
            .create_durable_memory(&create("ws-a", "source-c", "value three"), "test")
            .unwrap_err();
        assert!(third.contains("conflict set is full"));
        let result = store
            .retrieve_durable_memories(
                &MemoryRetrievalRequest {
                    scope: scope("ws-a"),
                    run_id: "run-2".into(),
                    node_id: "node-1".into(),
                    query: "value".into(),
                    top_k: 5,
                    max_tokens: 100,
                    max_bytes: 1000,
                    allow_lexical_fallback: false,
                },
                "test",
            )
            .unwrap();
        assert!(result.selected.is_empty());
        assert_eq!(result.state_excluded, 2);
    }

    #[test]
    fn supersede_is_atomic_scope_bound_and_restart_safe() {
        let _embedding = embedding_disabled();
        let (dir, store) = store();
        let first = store
            .create_durable_memory(&create("ws-a", "source-a", "value one"), "test")
            .unwrap();
        let second = store
            .create_durable_memory(&create("ws-a", "source-b", "value two"), "test")
            .unwrap();
        let first_id = first["memory_id"].as_str().unwrap();
        let second_id = second["memory_id"].as_str().unwrap();
        let resolved = store
            .supersede_durable_memory(second_id, 1, first_id, 2, "test")
            .unwrap();
        assert_eq!(resolved["winner"]["state"], "current");
        assert_eq!(resolved["winner"]["supersedes_memory_id"], first_id);
        assert_eq!(resolved["superseded"]["state"], "superseded");

        let stale = store
            .supersede_durable_memory(second_id, 1, first_id, 2, "test")
            .unwrap_err();
        assert!(stale.contains("version conflict"));
        assert_eq!(store.inspect_durable_memory(first_id).unwrap().len(), 3);
        assert_eq!(store.inspect_durable_memory(second_id).unwrap().len(), 2);

        drop(store);
        let reopened = LocalProductStore::new(dir.path().join("memory.db")).unwrap();
        let first_history = reopened.inspect_durable_memory(first_id).unwrap();
        let second_history = reopened.inspect_durable_memory(second_id).unwrap();
        assert_eq!(first_history.last().unwrap()["state"], "superseded");
        assert_eq!(second_history.last().unwrap()["state"], "current");
    }

    #[test]
    fn provider_embedding_is_source_scope_version_and_restart_bound() {
        let _env = ProviderEnvGuard::enabled();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("provider-memory.db");
        let store = LocalProductStore::new_with_embedding_transport(
            &path,
            || "2026-07-15T00:00:00Z".to_string(),
            Arc::new(MockTransport::new(provider_responses(1))),
        )
        .unwrap();
        let created = store
            .create_durable_memory(
                &create("ws-provider", "provider-source", "provider memory fact"),
                "test",
            )
            .unwrap();
        let memory_id = created["memory_id"].as_str().unwrap().to_string();
        assert_eq!(created["embedding"]["provenance"], "provider_reported");
        assert_eq!(created["embedding"]["dimensions"], 1536);
        assert_eq!(
            created["embedding"]["provider"]["provider_id"],
            "openrouter"
        );
        assert!(created["embedding"]["provider"]["cost_usd"].is_null());
        assert_eq!(
            created["embedding"]["provider"]["pricing"]["source"],
            "provider_catalog_reported"
        );
        assert_eq!(
            created["embedding"]["binding_sha256"]
                .as_str()
                .unwrap()
                .len(),
            64
        );
        let audit_contains_raw: bool = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM audit_log WHERE details_json LIKE '%provider memory fact%')",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        assert!(!audit_contains_raw);
        assert_eq!(store.check_integrity().unwrap().status, "ok");
        store.checkpoint_wal().unwrap();
        let backup = BackupManager::new(&dir.path().join("backups")).unwrap();
        let backup_record = backup
            .create_backup(
                &path,
                "provider embedding metadata",
                "provider-memory",
                "2026-07-15T00:00:30Z",
            )
            .unwrap();
        backup.save_metadata(&[backup_record]).unwrap();
        let restored_path = dir.path().join("provider-memory-restored.db");
        assert!(
            backup
                .restore_backup_with_verify("provider-memory", &restored_path, 1.0)
                .unwrap()
                .success
        );
        let restored = LocalProductStore::new(&restored_path).unwrap();
        assert_eq!(
            restored.inspect_durable_memory(&memory_id).unwrap()[0]["embedding"]["binding_sha256"],
            created["embedding"]["binding_sha256"]
        );
        drop(restored);
        let original_source_sha256 = created["source_sha256"].as_str().unwrap().to_string();
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE durable_memory_versions SET source_sha256=?1 WHERE memory_id=?2",
                    params!["cc".repeat(32), memory_id],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
            })
            .unwrap();
        assert!(store
            .inspect_durable_memory(&memory_id)
            .unwrap_err()
            .contains("scope/source/version binding mismatch"));
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE durable_memory_versions SET source_sha256=?1 WHERE memory_id=?2",
                    params![original_source_sha256, memory_id],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
            })
            .unwrap();
        drop(store);

        let reopened = LocalProductStore::new_with_embedding_transport(
            &path,
            || "2026-07-15T00:01:00Z".to_string(),
            Arc::new(MockTransport::new(provider_responses(4))),
        )
        .unwrap();
        let history = reopened.inspect_durable_memory(&memory_id).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(
            history[0]["embedding"]["binding_sha256"],
            created["embedding"]["binding_sha256"]
        );
        let result = reopened
            .retrieve_durable_memories(
                &MemoryRetrievalRequest {
                    scope: scope("ws-provider"),
                    run_id: "run-retrieval".into(),
                    node_id: "node-retrieval".into(),
                    query: "provider fact".into(),
                    top_k: 5,
                    max_tokens: 100,
                    max_bytes: 1000,
                    allow_lexical_fallback: false,
                },
                "test",
            )
            .unwrap();
        assert_eq!(result.selected.len(), 1);
        assert_eq!(
            result.embedding_provider.as_ref().unwrap()["provider_id"],
            "openrouter"
        );

        let isolated = reopened
            .retrieve_durable_memories(
                &MemoryRetrievalRequest {
                    scope: scope("other-workspace"),
                    run_id: "run-isolated".into(),
                    node_id: "node-isolated".into(),
                    query: "provider fact".into(),
                    top_k: 5,
                    max_tokens: 100,
                    max_bytes: 1000,
                    allow_lexical_fallback: false,
                },
                "test",
            )
            .unwrap();
        assert!(isolated.selected.is_empty());

        let stale = reopened
            .revise_durable_memory(
                &memory_id,
                &DurableMemoryRevision {
                    expected_version: 2,
                    source_id: "provider-source-v2".into(),
                    source_sha256: sha256_bytes(b"provider memory v2"),
                    content: json!({"text":"provider memory v2"}),
                    confidence: 0.9,
                    fresh_until: None,
                    expires_at: None,
                },
                "test",
            )
            .unwrap_err();
        assert!(stale.contains("version conflict"));
        let revised = reopened
            .revise_durable_memory(
                &memory_id,
                &DurableMemoryRevision {
                    expected_version: 1,
                    source_id: "provider-source-v2".into(),
                    source_sha256: sha256_bytes(b"provider memory v2"),
                    content: json!({"text":"provider memory v2"}),
                    confidence: 0.95,
                    fresh_until: None,
                    expires_at: None,
                },
                "test",
            )
            .unwrap();
        assert_eq!(revised["version"], 2);
        assert_eq!(revised["source_id"], "provider-source-v2");
        assert_ne!(
            revised["embedding"]["binding_sha256"],
            created["embedding"]["binding_sha256"]
        );
        assert_eq!(
            reopened.inspect_durable_memory(&memory_id).unwrap().len(),
            2
        );
    }

    #[test]
    fn provider_embedding_create_retry_and_concurrent_revision_call_once() {
        let _env = ProviderEnvGuard::enabled();
        let dir = TempDir::new().unwrap();
        let transport = Arc::new(CountingEmbeddingTransport {
            posts: AtomicUsize::new(0),
        });
        let store = Arc::new(
            LocalProductStore::new_with_embedding_transport(
                dir.path().join("provider-idempotency.db"),
                || "2026-07-15T00:00:00Z".to_string(),
                transport.clone(),
            )
            .unwrap(),
        );
        let request = create(
            "ws-provider-idempotency",
            "provider-source-idempotency",
            "bounded provider memory",
        );
        let created = store.create_durable_memory(&request, "test").unwrap();
        let memory_id = created["memory_id"].as_str().unwrap().to_string();
        let duplicate = store.create_durable_memory(&request, "test").unwrap();
        assert_eq!(duplicate["record_sha256"], created["record_sha256"]);
        assert_eq!(transport.posts.load(Ordering::SeqCst), 1);

        let revisions = [
            "bounded provider memory v2-a",
            "bounded provider memory v2-b",
        ]
        .map(|content| DurableMemoryRevision {
            expected_version: 1,
            source_id: format!("provider-source-{}", &sha256_bytes(content.as_bytes())[..8]),
            source_sha256: sha256_bytes(content.as_bytes()),
            content: json!({"text":content}),
            confidence: 0.95,
            fresh_until: None,
            expires_at: None,
        });
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let results = std::thread::scope(|scope| {
            let handles = revisions
                .into_iter()
                .map(|revision| {
                    let store = Arc::clone(&store);
                    let barrier = Arc::clone(&barrier);
                    let memory_id = memory_id.clone();
                    scope.spawn(move || {
                        barrier.wait();
                        store.revise_durable_memory(&memory_id, &revision, "test")
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        assert!(results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .all(|error| {
                error.contains("outcome is unknown")
                    || error.contains("version conflict")
                    || error.contains("competing provider embedding mutation")
            }));
        assert_eq!(transport.posts.load(Ordering::SeqCst), 2);
        assert_eq!(store.inspect_durable_memory(&memory_id).unwrap().len(), 2);
        let request_events = store
            .provider_audit_events(100)
            .unwrap()
            .into_iter()
            .filter(|event| event["event_type"] == "request_sent")
            .count();
        assert_eq!(request_events, 2);
    }

    #[test]
    fn completed_provider_embedding_receipt_recovers_after_restart_without_another_post() {
        let _env = ProviderEnvGuard::enabled();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("provider-recovery.db");
        let transport = Arc::new(CountingEmbeddingTransport {
            posts: AtomicUsize::new(0),
        });
        let request = create(
            "ws-provider-recovery",
            "provider-source-recovery",
            "recover completed provider result",
        );
        let store = LocalProductStore::new_with_embedding_transport(
            &path,
            || "2026-07-15T00:00:00Z".to_string(),
            transport.clone(),
        )
        .unwrap();
        let created = store.create_durable_memory(&request, "test").unwrap();
        assert_eq!(transport.posts.load(Ordering::SeqCst), 1);

        // Simulate the only relevant crash window: the provider result receipt
        // committed, but the durable-memory version did not.
        store
            .with_conn(|conn| {
                conn.execute(
                    "DELETE FROM durable_memory_versions WHERE memory_id=?1",
                    [created["memory_id"].as_str().unwrap()],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
            })
            .unwrap();
        drop(store);

        let reopened = LocalProductStore::new_with_embedding_transport(
            &path,
            || "2026-07-16T00:00:00Z".to_string(),
            transport.clone(),
        )
        .unwrap();
        let recovered = reopened.create_durable_memory(&request, "test").unwrap();
        assert_eq!(
            recovered["embedding"]["binding_sha256"],
            created["embedding"]["binding_sha256"]
        );
        assert_eq!(transport.posts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn provider_embedding_gates_and_unknown_outcome_fail_closed() {
        let _env = ProviderEnvGuard::enabled();
        let dir = TempDir::new().unwrap();
        std::env::remove_var("ACP_ENABLE_PROVIDER_EXECUTION");
        let gated = LocalProductStore::new_with_embedding_transport(
            dir.path().join("provider-gated.db"),
            || "2026-07-15T00:00:00Z".to_string(),
            Arc::new(MockTransport::empty()),
        )
        .unwrap();
        assert!(gated
            .create_durable_memory(
                &create("ws-provider-gated", "source-gated", "bounded fact"),
                "test",
            )
            .unwrap_err()
            .contains("provider execution gate"));
        std::env::set_var("ACP_ENABLE_PROVIDER_EXECUTION", "1");
        std::env::remove_var("ACP_REQUIRE_AUTH");
        assert!(gated
            .create_durable_memory(
                &create("ws-provider-auth", "source-auth", "bounded fact"),
                "test",
            )
            .unwrap_err()
            .contains("authenticated runtime"));
        std::env::set_var("ACP_REQUIRE_AUTH", "1");

        let unknown = LocalProductStore::new_with_embedding_transport(
            dir.path().join("provider-unknown.db"),
            || "2026-07-15T00:00:00Z".to_string(),
            Arc::new(MockTransport::new(vec![
                Ok(provider_catalog_response()),
                Err(HttpError::Timeout("fixture timeout".to_string())),
                Ok(provider_vector_response()),
            ])),
        )
        .unwrap();
        let error = unknown
            .create_durable_memory(
                &create("ws-provider-unknown", "source-unknown", "bounded fact"),
                "test",
            )
            .unwrap_err();
        assert!(error.contains("outcome unknown"));
        let events = unknown.provider_audit_events(10).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| event["event_type"] == "request_sent")
                .count(),
            1
        );
        assert!(events.iter().any(|event| {
            event["event_type"] == "error" && event["error_domain"] == "provider_outcome_unknown"
        }));

        let capped = LocalProductStore::new_with_embedding_transport(
            dir.path().join("provider-capped.db"),
            || "2026-07-15T00:00:00Z".to_string(),
            Arc::new(MockTransport::new(vec![Ok(provider_catalog_response())])),
        )
        .unwrap();
        let prior_cost = ProviderAuditEvent {
            schema_version: "provider_audit_event.v1".to_string(),
            event_id: "fixture-prior-reservation".to_string(),
            dispatch_id: "fixture-prior-dispatch".to_string(),
            provider_id: "fixture-provider".to_string(),
            event_type: "request_reserved".to_string(),
            input_token_count: None,
            output_token_count: None,
            cost: Some(0.2),
            currency: Some("USD".to_string()),
            latency_ms: None,
            error_domain: None,
            redaction_status: "redacted".to_string(),
            created_at: "2026-07-15T00:00:00Z".to_string(),
        };
        capped
            .reserve_provider_audit_cost(&prior_cost, 1.0, 1.0)
            .unwrap();
        assert!(capped
            .create_durable_memory(
                &create("ws-provider-capped", "source-capped", "bounded fact"),
                "test",
            )
            .unwrap_err()
            .contains("daily cost cap exceeded"));
        assert!(!capped
            .provider_audit_events(10)
            .unwrap()
            .iter()
            .any(|event| event["event_type"] == "request_sent"));
    }

    #[test]
    fn provider_embedding_failure_reconciliation_and_reembedding_are_owned() {
        use super::super::provider_audit::{
            ProviderEmbeddingResolutionAction, ProviderEmbeddingResolutionRequest,
        };
        let _env = ProviderEnvGuard::enabled();
        let dir = TempDir::new().unwrap();
        let failed_request = create(
            "ws-provider-resolution",
            "source-resolution",
            "retry bounded fact",
        );
        let transport = Arc::new(MockTransport::new(vec![
            Ok(provider_catalog_response()),
            Ok(HttpResponse {
                status: 401,
                body: br#"{"error":"fixture refusal"}"#.to_vec(),
            }),
            Ok(provider_catalog_response()),
            Ok(provider_vector_response()),
            Ok(provider_catalog_response()),
            Ok(provider_vector_response()),
        ]));
        let store = LocalProductStore::new_with_embedding_transport(
            dir.path().join("provider-resolution.db"),
            || "2026-07-15T00:00:00Z".to_string(),
            transport,
        )
        .unwrap();
        let failure = store
            .create_durable_memory(&failed_request, "test")
            .unwrap_err();
        assert!(
            !failure.contains("outcome unknown"),
            "unexpected ambiguous failure: {failure}"
        );
        let memory_id = memory_id(&failed_request).unwrap();
        let resolution = ProviderEmbeddingResolutionRequest {
            target_version: 1,
            expected_attempt_count: 1,
            scope: failed_request.scope.clone(),
            run_id: failed_request.run_id.clone(),
            action: ProviderEmbeddingResolutionAction::RetryFailed,
            evidence_source_id: None,
            evidence_sha256: None,
            confirm_resolution: true,
        };
        let authorized = store
            .reconcile_provider_embedding_operation(&memory_id, &resolution, "operator")
            .unwrap();
        assert_eq!(authorized["state"], "retry_authorized");
        assert_eq!(authorized["attempt_count"], 2);
        assert_eq!(
            store
                .reconcile_provider_embedding_operation(&memory_id, &resolution, "operator")
                .unwrap()["idempotent"],
            true
        );
        let created = store
            .create_durable_memory(&failed_request, "test")
            .unwrap();
        assert_eq!(created["version"], 1);
        assert_eq!(
            store
                .provider_audit_events(20)
                .unwrap()
                .iter()
                .filter(|event| event["event_type"] == "request_sent")
                .count(),
            2
        );
        let retry_audit = store
            .with_conn(|conn| {
                conn.query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action='provider_embedding.retry_authorized'",
            [],|row|row.get::<_,i64>(0)).map_err(|error|error.to_string())
            })
            .unwrap();
        assert_eq!(retry_audit, 1);

        let reembedded = store
            .reembed_durable_memory(&memory_id, 1, "operator")
            .unwrap();
        assert_eq!(reembedded["version"], 2);
        assert_eq!(reembedded["source_id"], failed_request.source_id);
        assert_ne!(
            reembedded["embedding"]["binding_sha256"],
            created["embedding"]["binding_sha256"]
        );
        assert!(store
            .with_conn(|conn| conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM audit_log WHERE action='durable_memory.reembed')",
                    [],
                    |row| row.get::<_, bool>(0)
                )
                .map_err(|error| error.to_string()))
            .unwrap());
    }

    #[test]
    fn unknown_embedding_outcome_requires_hash_bound_operator_evidence() {
        use super::super::provider_audit::{
            ProviderEmbeddingResolutionAction, ProviderEmbeddingResolutionRequest,
        };
        let _env = ProviderEnvGuard::enabled();
        let dir = TempDir::new().unwrap();
        let request = create(
            "ws-provider-unknown-resolution",
            "source-unknown",
            "unknown bounded fact",
        );
        let store = LocalProductStore::new_with_embedding_transport(
            dir.path().join("provider-unknown-resolution.db"),
            || "2026-07-15T00:00:00Z".to_string(),
            Arc::new(MockTransport::new(vec![
                Ok(provider_catalog_response()),
                Err(HttpError::Timeout("fixture timeout".into())),
                Ok(provider_catalog_response()),
                Ok(provider_vector_response()),
            ])),
        )
        .unwrap();
        assert!(store
            .create_durable_memory(&request, "test")
            .unwrap_err()
            .contains("outcome unknown"));
        let memory_id = memory_id(&request).unwrap();
        let mut resolution = ProviderEmbeddingResolutionRequest {
            target_version: 1,
            expected_attempt_count: 1,
            scope: request.scope.clone(),
            run_id: request.run_id.clone(),
            action: ProviderEmbeddingResolutionAction::ConfirmUnknownNoEffectAndRetry,
            evidence_source_id: None,
            evidence_sha256: None,
            confirm_resolution: true,
        };
        assert!(store
            .reconcile_provider_embedding_operation(&memory_id, &resolution, "operator")
            .unwrap_err()
            .contains("evidence source"));
        resolution.evidence_source_id = Some("openrouter-request-status-20260715".into());
        resolution.evidence_sha256 = Some("e".repeat(64));
        store
            .reconcile_provider_embedding_operation(&memory_id, &resolution, "operator")
            .unwrap();
        assert_eq!(
            store.create_durable_memory(&request, "test").unwrap()["version"],
            1
        );
    }

    #[test]
    fn corrupted_completed_provider_receipt_is_rejected_before_memory_commit() {
        let _env = ProviderEnvGuard::enabled();
        let dir = TempDir::new().unwrap();
        let request = create(
            "ws-provider-corrupt-receipt",
            "source-corrupt",
            "corrupt receipt fact",
        );
        let store = LocalProductStore::new_with_embedding_transport(
            dir.path().join("provider-corrupt-receipt.db"),
            || "2026-07-15T00:00:00Z".to_string(),
            Arc::new(MockTransport::new(provider_responses(2))),
        )
        .unwrap();
        let created = store.create_durable_memory(&request, "test").unwrap();
        let memory_id = created["memory_id"].as_str().unwrap();
        store.with_conn(|conn| {
            conn.execute("DELETE FROM durable_memory_versions WHERE memory_id=?1",[memory_id])
                .map_err(|error|error.to_string())?;
            let raw:String=conn.query_row(
                "SELECT metadata_json FROM provider_embedding_operations WHERE target_memory_id=?1",
                [memory_id],|row|row.get(0)).map_err(|error|error.to_string())?;
            let mut metadata:Value=serde_json::from_str(&raw).map_err(|error|error.to_string())?;
            metadata["measurement_provenance"]=json!("estimated");
            conn.execute("UPDATE provider_embedding_operations SET metadata_json=?1 WHERE target_memory_id=?2",
                params![metadata.to_string(),memory_id]).map_err(|error|error.to_string())?;
            Ok(())
        }).unwrap();
        let error = store.create_durable_memory(&request, "test").unwrap_err();
        assert!(
            error.contains("pricing binding") || error.contains("provenance"),
            "unexpected corrupted-receipt refusal: {error}"
        );
        assert!(store.inspect_durable_memory(memory_id).unwrap().is_empty());
    }

    #[test]
    fn provider_embedded_lifecycle_preserves_explicit_state_semantics() {
        let _env = ProviderEnvGuard::enabled();
        let dir = TempDir::new().unwrap();
        let transport = Arc::new(CountingEmbeddingTransport {
            posts: AtomicUsize::new(0),
        });
        let store = LocalProductStore::new_with_embedding_transport(
            dir.path().join("provider-lifecycle.db"),
            || "2026-07-15T00:00:00Z".to_string(),
            transport.clone(),
        )
        .unwrap();
        let mut expiring = create(
            "ws-provider-lifecycle",
            "source-expiring",
            "expiring provider fact",
        );
        expiring.expires_at = Some("2020-01-01T00:00:00Z".into());
        let expiring = store.create_durable_memory(&expiring, "test").unwrap();
        assert_eq!(
            store
                .prune_expired_durable_memories(&scope("ws-provider-lifecycle"), "test")
                .unwrap()["pruned_count"],
            1
        );
        let expiring_history = store
            .inspect_durable_memory(expiring["memory_id"].as_str().unwrap())
            .unwrap();
        assert_eq!(expiring_history.last().unwrap()["state"], "expired");

        let first = store
            .create_durable_memory(
                &create("ws-provider-conflict", "source-a", "provider value a"),
                "test",
            )
            .unwrap();
        let second = store
            .create_durable_memory(
                &create("ws-provider-conflict", "source-b", "provider value b"),
                "test",
            )
            .unwrap();
        let first_id = first["memory_id"].as_str().unwrap();
        let second_id = second["memory_id"].as_str().unwrap();
        assert_eq!(
            store
                .supersede_durable_memory(second_id, 1, first_id, 2, "test")
                .unwrap()["winner"]["state"],
            "current"
        );
        let tombstoned = store.forget_durable_memory(second_id, 2, "test").unwrap();
        assert_eq!(tombstoned["state"], "tombstoned");
        assert_eq!(tombstoned["embedding"]["present"], false);
        assert_eq!(transport.posts.load(Ordering::SeqCst), 3);
        assert_eq!(store.check_integrity().unwrap().status, "ok");
    }
}
