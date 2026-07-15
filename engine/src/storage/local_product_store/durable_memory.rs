use std::collections::BTreeSet;

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};

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
    record_sha256: String,
    created_at: String,
    created_by: String,
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
        let embedding = embedding_for_text(&request.content.to_string())?;
        let memory_id = memory_id(request)?;
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
        let embedding = embedding_for_text(&revision.content.to_string())?;
        self.append_memory_version(
            memory_id,
            revision.expected_version,
            "current",
            revision,
            embedding,
            actor,
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
        embedding: Option<(Vec<f64>, String)>,
        actor: &str,
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
                    "durable_memory.revise",
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
                    "durable_memory.revise",
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
                let mut stmt = conn.prepare("SELECT memory_id, version, tenant_id, workspace_id, agent_id, run_id, task_id, source_id, source_sha256, conflict_key, state, confidence, fresh_until, expires_at, supersedes_memory_id, content_json, embedding_json, embedding_provenance, record_sha256, created_at, created_by FROM durable_memory_versions WHERE memory_id=?1 ORDER BY version")
                    .map_err(|error| error.to_string())?;
                let rows = stmt.query_map(params![memory_id], sqlite_memory_row).map_err(|error| error.to_string())?;
                rows.map(|row| row.map_err(|error| error.to_string())).collect::<Result<Vec<_>, _>>()
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client.query("SELECT memory_id, version, tenant_id, workspace_id, agent_id, run_id, task_id, source_id, source_sha256, conflict_key, state, confidence::DOUBLE PRECISION, fresh_until, expires_at, supersedes_memory_id, content_json, embedding_json, embedding_provenance, record_sha256, created_at, created_by FROM durable_memory_versions WHERE memory_id=$1 ORDER BY version", &[&memory_id])
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
        let query_embedding = embedding_for_text(&request.query)?;
        let (mode, provenance) = match &query_embedding {
            Some((_, provenance)) => ("semantic_vector", provenance.clone()),
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
            let score = if let Some((query, _)) = &query_embedding {
                let Some(candidate_embedding) = candidate.embedding.as_ref() else {
                    continue;
                };
                cosine_similarity(query, candidate_embedding)?
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
        }))?;
        let retrieval_id = format!("retrieval-{result_sha256}");
        let result = MemoryRetrievalResult {
            schema_version: MEMORY_RETRIEVAL_SCHEMA_VERSION.to_string(),
            retrieval_id: retrieval_id.clone(),
            mode: mode.to_string(),
            embedding_provenance: provenance,
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
                    "SELECT m.memory_id, m.version, m.tenant_id, m.workspace_id, m.agent_id, m.run_id, m.task_id, m.source_id, m.source_sha256, m.conflict_key, m.state, m.confidence, m.fresh_until, m.expires_at, m.supersedes_memory_id, m.content_json, m.embedding_json, m.embedding_provenance, m.record_sha256, m.created_at, m.created_by
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
                    "SELECT m.memory_id, m.version, m.tenant_id, m.workspace_id, m.agent_id, m.run_id, m.task_id, m.source_id, m.source_sha256, m.conflict_key, m.state, m.confidence::DOUBLE PRECISION, m.fresh_until, m.expires_at, m.supersedes_memory_id, m.content_json, m.embedding_json, m.embedding_provenance, m.record_sha256, m.created_at, m.created_by
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

fn embedding_for_text(text: &str) -> Result<Option<(Vec<f64>, String)>, String> {
    let mode = std::env::var("ACP_DURABLE_MEMORY_EMBEDDING_MODE")
        .unwrap_or_else(|_| "disabled".to_string());
    match mode.as_str() {
        "disabled" => Ok(None),
        "fixture" if cfg!(test) => Ok(Some((
            local_embedding(text),
            "deterministic_fixture".to_string(),
        ))),
        "fixture" => Err("fixture embeddings are forbidden outside tests".to_string()),
        "local_hash_v1" => {
            if std::env::var("CI").is_ok() {
                return Err("production embedding generation is disabled in CI".to_string());
            }
            if std::env::var("ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS").as_deref() != Ok("1") {
                return Err("durable memory embedding gate is disabled".to_string());
            }
            Ok(Some((local_embedding(text), "harness_derived".to_string())))
        }
        "provider" => Err(
            "provider embedding mode is unavailable without the managed external-provider adapter"
                .to_string(),
        ),
        _ => Err("unsupported durable memory embedding mode".to_string()),
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
    embedding: Option<(Vec<f64>, String)>,
    created_at: String,
    created_by: String,
) -> Result<StoredMemory, String> {
    let (embedding, embedding_provenance) = embedding
        .map_or((None, "unavailable".to_string()), |(values, provenance)| {
            (Some(values), provenance)
        });
    let unsigned = json!({"schema_version":DURABLE_MEMORY_SCHEMA_VERSION,"memory_id":memory_id,"version":version,"scope":scope,"run_id":run_id,"source_id":source_id,"source_sha256":source_sha256,"conflict_key":conflict_key,"state":state,"confidence":confidence,"fresh_until":fresh_until,"expires_at":expires_at,"supersedes_memory_id":supersedes_memory_id,"content":content,"embedding":embedding,"embedding_provenance":embedding_provenance,"created_at":created_at,"created_by":created_by});
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
        record_sha256,
        created_at,
        created_by,
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
        prior
            .embedding
            .clone()
            .map(|embedding| (embedding, prior.embedding_provenance.clone())),
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
        prior
            .embedding
            .clone()
            .map(|embedding| (embedding, prior.embedding_provenance.clone())),
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
        json!({"schema_version":DURABLE_MEMORY_SCHEMA_VERSION,"memory_id":memory.memory_id,"version":memory.version,"scope":memory.scope,"run_id":memory.run_id,"source_id":memory.source_id,"source_sha256":memory.source_sha256,"conflict_key":memory.conflict_key,"state":memory.state,"confidence":memory.confidence,"fresh_until":memory.fresh_until,"expires_at":memory.expires_at,"supersedes_memory_id":memory.supersedes_memory_id,"content":memory.content,"embedding":{"present":memory.embedding.is_some(),"dimensions":memory.embedding.as_ref().map(Vec::len),"provenance":memory.embedding_provenance},"record_sha256":memory.record_sha256,"created_at":memory.created_at,"created_by":memory.created_by}),
    )
}

fn memory_audit(memory: &StoredMemory) -> Value {
    json!({"memory_id":memory.memory_id,"version":memory.version,"tenant_id":memory.scope.tenant_id,"workspace_id":memory.scope.workspace_id,"agent_id":memory.scope.agent_id,"task_id":memory.scope.task_id,"source_id":memory.source_id,"source_sha256":memory.source_sha256,"record_sha256":memory.record_sha256,"state":memory.state,"confidence":memory.confidence,"embedding_provenance":memory.embedding_provenance,"content_stored_in_audit":false})
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
    conn.execute("INSERT INTO durable_memory_versions (memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,record_sha256,created_at,created_by) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)", params![memory.memory_id,memory.version,memory.scope.tenant_id,memory.scope.workspace_id,memory.scope.agent_id,memory.run_id,memory.scope.task_id,memory.source_id,memory.source_sha256,memory.conflict_key,memory.state,memory.confidence,memory.fresh_until,memory.expires_at,memory.supersedes_memory_id,content_json,embedding_json,memory.embedding_provenance,memory.record_sha256,memory.created_at,memory.created_by]).map_err(|error| error.to_string())?;
    Ok(())
}

fn sqlite_latest_memory(
    conn: &rusqlite::Connection,
    memory_id: &str,
) -> Result<Option<StoredMemory>, String> {
    conn.query_row("SELECT memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,record_sha256,created_at,created_by FROM durable_memory_versions WHERE memory_id=?1 ORDER BY version DESC LIMIT 1", params![memory_id], sqlite_memory_row).optional().map_err(|error| error.to_string())
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
        "SELECT m.memory_id,m.version,m.tenant_id,m.workspace_id,m.agent_id,m.run_id,m.task_id,m.source_id,m.source_sha256,m.conflict_key,m.state,m.confidence,m.fresh_until,m.expires_at,m.supersedes_memory_id,m.content_json,m.embedding_json,m.embedding_provenance,m.record_sha256,m.created_at,m.created_by
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
    Ok(StoredMemory {
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
        record_sha256: row.get(18)?,
        created_at: row.get(19)?,
        created_by: row.get(20)?,
    })
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
    let mut stmt=conn.prepare("SELECT m.memory_id,m.version,m.tenant_id,m.workspace_id,m.agent_id,m.run_id,m.task_id,m.source_id,m.source_sha256,m.conflict_key,m.state,m.confidence,m.fresh_until,m.expires_at,m.supersedes_memory_id,m.content_json,m.embedding_json,m.embedding_provenance,m.record_sha256,m.created_at,m.created_by FROM durable_memory_versions m JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest ON latest.memory_id=m.memory_id AND latest.version=m.version WHERE m.tenant_id=?1 AND m.workspace_id=?2 AND m.agent_id IS ?3 AND m.task_id IS ?4 AND m.conflict_key=?5 AND m.state='current' AND m.source_sha256<>?6 ORDER BY m.memory_id").map_err(|error|error.to_string())?;
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
    tx.execute("INSERT INTO durable_memory_versions (memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,record_sha256,created_at,created_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)",&[&memory.memory_id,&memory.version,&memory.scope.tenant_id,&memory.scope.workspace_id,&memory.scope.agent_id,&memory.run_id,&memory.scope.task_id,&memory.source_id,&memory.source_sha256,&memory.conflict_key,&memory.state,&memory.confidence,&memory.fresh_until,&memory.expires_at,&memory.supersedes_memory_id,&content,&embedding,&memory.embedding_provenance,&memory.record_sha256,&memory.created_at,&memory.created_by]).map_err(|error|error.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_memory_row(row: &postgres::Row) -> Result<StoredMemory, String> {
    let content: String = row.get(15);
    let embedding: Option<String> = row.get(16);
    Ok(StoredMemory {
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
        record_sha256: row.get(18),
        created_at: row.get(19),
        created_by: row.get(20),
    })
}

#[cfg(feature = "pg")]
fn pg_latest_memory(
    tx: &mut postgres::Transaction<'_>,
    memory_id: &str,
) -> Result<Option<StoredMemory>, String> {
    tx.query_opt("SELECT memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence::DOUBLE PRECISION,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,record_sha256,created_at,created_by FROM durable_memory_versions WHERE memory_id=$1 ORDER BY version DESC LIMIT 1",&[&memory_id]).map_err(|error|error.to_string())?.as_ref().map(pg_memory_row).transpose()
}

#[cfg(feature = "pg")]
fn pg_latest_memory_for_update(
    tx: &mut postgres::Transaction<'_>,
    memory_id: &str,
) -> Result<Option<StoredMemory>, String> {
    tx.query_opt("SELECT memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,source_sha256,conflict_key,state,confidence::DOUBLE PRECISION,fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,embedding_provenance,record_sha256,created_at,created_by FROM durable_memory_versions WHERE memory_id=$1 ORDER BY version DESC LIMIT 1 FOR UPDATE",&[&memory_id]).map_err(|error|error.to_string())?.as_ref().map(pg_memory_row).transpose()
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
        "SELECT m.memory_id,m.version,m.tenant_id,m.workspace_id,m.agent_id,m.run_id,m.task_id,m.source_id,m.source_sha256,m.conflict_key,m.state,m.confidence::DOUBLE PRECISION,m.fresh_until,m.expires_at,m.supersedes_memory_id,m.content_json,m.embedding_json,m.embedding_provenance,m.record_sha256,m.created_at,m.created_by
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
    let rows=tx.query("SELECT m.memory_id,m.version,m.tenant_id,m.workspace_id,m.agent_id,m.run_id,m.task_id,m.source_id,m.source_sha256,m.conflict_key,m.state,m.confidence::DOUBLE PRECISION,m.fresh_until,m.expires_at,m.supersedes_memory_id,m.content_json,m.embedding_json,m.embedding_provenance,m.record_sha256,m.created_at,m.created_by FROM durable_memory_versions m JOIN (SELECT memory_id,MAX(version) version FROM durable_memory_versions GROUP BY memory_id) latest ON latest.memory_id=m.memory_id AND latest.version=m.version WHERE m.tenant_id=$1 AND m.workspace_id=$2 AND m.agent_id IS NOT DISTINCT FROM $3 AND m.task_id IS NOT DISTINCT FROM $4 AND m.conflict_key=$5 AND m.state='current' AND m.source_sha256<>$6 ORDER BY m.memory_id FOR UPDATE OF m",&[&request.scope.tenant_id,&request.scope.workspace_id,&request.scope.agent_id,&request.scope.task_id,&request.conflict_key,&request.source_sha256]).map_err(|error|error.to_string())?;
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
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use tempfile::TempDir;

    struct EmbeddingFixtureGuard {
        _lock: MutexGuard<'static, ()>,
    }
    impl Drop for EmbeddingFixtureGuard {
        fn drop(&mut self) {
            std::env::remove_var("ACP_DURABLE_MEMORY_EMBEDDING_MODE");
        }
    }
    fn embedding_fixture() -> EmbeddingFixtureGuard {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let lock = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        std::env::set_var("ACP_DURABLE_MEMORY_EMBEDDING_MODE", "fixture");
        EmbeddingFixtureGuard { _lock: lock }
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
}
