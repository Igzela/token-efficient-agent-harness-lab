use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{count_table, DatabaseConnection, LocalProductStore};
use crate::provider::embedding::{
    ProviderEmbeddingMetadata, OPENROUTER_EMBEDDING_DIMENSIONS, OPENROUTER_EMBEDDING_PROVIDER_ID,
};

#[derive(Debug, Clone, PartialEq)]
pub struct TableIntegrity {
    pub name: String,
    pub row_count: i64,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntegrityReport {
    pub status: String,
    pub tables: Vec<TableIntegrity>,
    pub schema_version: i64,
}

const INTEGRITY_TABLES: &[&str] = &[
    "dispatch_history",
    "local_config",
    "team_members",
    "api_key_metadata",
    "audit_log",
    "provider_audit_events",
    "workflow_plans",
    "workflow_runs",
    "workflow_run_nodes",
    "workflow_run_edges",
    "workflow_run_events",
    "workflow_run_approvals",
    "agent_action_receipts",
    "tool_allowlist_profiles",
    "tool_execution_authorizations",
    "supervised_patch_workspaces",
    "supervised_patch_artifacts",
    "native_scorecard_artifacts",
    "regression_report_artifacts",
    "budget_evidence_artifacts",
    "budget_pause_decisions",
    "scheduler_feedback",
    "tool_capabilities",
    "tool_allowlists",
    "tool_hooks",
    "agent_profiles",
    "orchestration_decisions",
    "executor_pool",
    "scheduler_heartbeat",
    "controlled_loop_policy_proposals",
    "controlled_loop_policy_snapshots",
    "agent_state",
    "agent_mailbox",
    "agent_proposals",
    "offline_replay_artifacts",
    "durable_memory_versions",
    "memory_retrieval_events",
    "production_jobs",
    "normalized_usage_observations",
    "replay_producer_bindings",
    "operator_acknowledgements",
    "external_runtime_checkpoints",
    "external_runtime_invocations",
];

#[derive(Debug)]
struct DurableMemoryIntegrityRow {
    memory_id: String,
    version: i64,
    tenant_id: String,
    workspace_id: String,
    agent_id: Option<String>,
    run_id: Option<String>,
    task_id: Option<String>,
    source_id: String,
    source_sha256: String,
    conflict_key: String,
    state: String,
    confidence: f64,
    fresh_until: Option<String>,
    expires_at: Option<String>,
    supersedes_memory_id: Option<String>,
    content_json: String,
    embedding_json: Option<String>,
    embedding_provenance: String,
    embedding_metadata_json: Option<String>,
    embedding_binding_sha256: Option<String>,
    record_sha256: String,
    created_at: String,
    created_by: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableMemoryEmbeddingBinding {
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

impl LocalProductStore {
    pub fn check_integrity(&self) -> Result<IntegrityReport, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let status: String = conn
                    .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                validate_sqlite_durable_memory_rows(conn)?;

                let mut table_reports = Vec::new();
                for table in INTEGRITY_TABLES {
                    let row_count = count_table(conn, table)?;
                    table_reports.push(TableIntegrity {
                        name: table.to_string(),
                        row_count,
                        status: if status == "ok" {
                            "ok".to_string()
                        } else {
                            "corrupt".to_string()
                        },
                    });
                }

                Ok(IntegrityReport {
                    status,
                    tables: table_reports,
                    schema_version: conn
                        .query_row("PRAGMA user_version", [], |row| row.get(0))
                        .unwrap_or(0),
                })
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute("SELECT 1", &[])
                    .map_err(|e| format!("PG basic check failed: {e}"))?;
                validate_pg_durable_memory_rows(client)?;

                let mut table_reports = Vec::new();
                for table in INTEGRITY_TABLES {
                    let row = client
                        .query_one(&format!("SELECT COUNT(*) FROM {table}"), &[])
                        .map_err(|e| e.to_string())?;
                    let row_count: i64 = row.get(0);
                    table_reports.push(TableIntegrity {
                        name: table.to_string(),
                        row_count,
                        status: "ok".to_string(),
                    });
                }

                let schema_version: i64 = client
                    .query_one(
                        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                        &[],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);

                Ok(IntegrityReport {
                    status: "ok".to_string(),
                    tables: table_reports,
                    schema_version,
                })
            }),
        }
    }
}

fn validate_sqlite_durable_memory_rows(conn: &rusqlite::Connection) -> Result<(), String> {
    let mut statement = conn
        .prepare(
            "SELECT memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,
                    source_id,source_sha256,conflict_key,state,confidence,fresh_until,expires_at,
                    supersedes_memory_id,content_json,embedding_json,embedding_provenance,
                    embedding_metadata_json,embedding_binding_sha256,record_sha256,created_at,created_by
             FROM durable_memory_versions ORDER BY memory_id,version",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], sqlite_durable_memory_integrity_row)
        .map_err(|error| error.to_string())?;
    for row in rows {
        validate_durable_memory_integrity_row(&row.map_err(|error| error.to_string())?)?;
    }
    Ok(())
}

fn sqlite_durable_memory_integrity_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<DurableMemoryIntegrityRow> {
    Ok(DurableMemoryIntegrityRow {
        memory_id: row.get(0)?,
        version: row.get(1)?,
        tenant_id: row.get(2)?,
        workspace_id: row.get(3)?,
        agent_id: row.get(4)?,
        run_id: row.get(5)?,
        task_id: row.get(6)?,
        source_id: row.get(7)?,
        source_sha256: row.get(8)?,
        conflict_key: row.get(9)?,
        state: row.get(10)?,
        confidence: row.get(11)?,
        fresh_until: row.get(12)?,
        expires_at: row.get(13)?,
        supersedes_memory_id: row.get(14)?,
        content_json: row.get(15)?,
        embedding_json: row.get(16)?,
        embedding_provenance: row.get(17)?,
        embedding_metadata_json: row.get(18)?,
        embedding_binding_sha256: row.get(19)?,
        record_sha256: row.get(20)?,
        created_at: row.get(21)?,
        created_by: row.get(22)?,
    })
}

#[cfg(feature = "pg")]
fn validate_pg_durable_memory_rows(client: &mut postgres::Client) -> Result<(), String> {
    let rows = client
        .query(
            "SELECT memory_id,version,tenant_id,workspace_id,agent_id,run_id,task_id,
                    source_id,source_sha256,conflict_key,state,confidence::DOUBLE PRECISION,
                    fresh_until,expires_at,supersedes_memory_id,content_json,embedding_json,
                    embedding_provenance,embedding_metadata_json,embedding_binding_sha256,
                    record_sha256,created_at,created_by
             FROM durable_memory_versions ORDER BY memory_id,version",
            &[],
        )
        .map_err(|error| error.to_string())?;
    for row in rows {
        validate_durable_memory_integrity_row(&DurableMemoryIntegrityRow {
            memory_id: row.get(0),
            version: row.get(1),
            tenant_id: row.get(2),
            workspace_id: row.get(3),
            agent_id: row.get(4),
            run_id: row.get(5),
            task_id: row.get(6),
            source_id: row.get(7),
            source_sha256: row.get(8),
            conflict_key: row.get(9),
            state: row.get(10),
            confidence: row.get(11),
            fresh_until: row.get(12),
            expires_at: row.get(13),
            supersedes_memory_id: row.get(14),
            content_json: row.get(15),
            embedding_json: row.get(16),
            embedding_provenance: row.get(17),
            embedding_metadata_json: row.get(18),
            embedding_binding_sha256: row.get(19),
            record_sha256: row.get(20),
            created_at: row.get(21),
            created_by: row.get(22),
        })?;
    }
    Ok(())
}

fn validate_durable_memory_integrity_row(row: &DurableMemoryIntegrityRow) -> Result<(), String> {
    let failure = |detail: &str| {
        format!(
            "durable memory integrity failure for {}/{}: {detail}",
            row.memory_id, row.version
        )
    };
    let content: Value = serde_json::from_str(&row.content_json)
        .map_err(|_| failure("content JSON is malformed"))?;
    let embedding = row
        .embedding_json
        .as_deref()
        .map(|value| {
            serde_json::from_str::<Vec<f64>>(value)
                .map_err(|_| failure("embedding vector JSON is malformed"))
        })
        .transpose()?;
    if embedding
        .as_ref()
        .is_some_and(|values| values.is_empty() || values.iter().any(|value| !value.is_finite()))
    {
        return Err(failure("embedding vector is empty or non-finite"));
    }
    let metadata = row
        .embedding_metadata_json
        .as_deref()
        .map(|value| {
            serde_json::from_str::<DurableMemoryEmbeddingBinding>(value)
                .map_err(|_| failure("provider embedding metadata JSON is malformed"))
        })
        .transpose()?;

    match (
        embedding.as_ref(),
        metadata.as_ref(),
        row.embedding_binding_sha256.as_ref(),
        row.embedding_provenance.as_str(),
    ) {
        (None, None, None, "unavailable") => {}
        (Some(_), None, None, "deterministic_fixture" | "harness_derived") => {}
        (Some(values), Some(metadata), Some(binding), "provider_reported") => {
            validate_provider_embedding_integrity(row, &content, values, metadata, binding)?;
        }
        _ => {
            return Err(failure(
                "embedding vector, provenance, metadata, and binding disagree",
            ))
        }
    }

    validate_durable_memory_record_hash(row, &content, embedding.as_ref(), metadata.as_ref())
        .map_err(|detail| failure(&detail))
}

fn validate_provider_embedding_integrity(
    row: &DurableMemoryIntegrityRow,
    content: &Value,
    values: &[f64],
    metadata: &DurableMemoryEmbeddingBinding,
    binding: &str,
) -> Result<(), String> {
    let failure = |detail: &str| {
        format!(
            "durable memory integrity failure for {}/{}: {detail}",
            row.memory_id, row.version
        )
    };
    if values.len() > OPENROUTER_EMBEDDING_DIMENSIONS
        || metadata.provider.dimensions != values.len()
    {
        return Err(failure("provider embedding dimension mismatch"));
    }
    if metadata.provider.provider_id != OPENROUTER_EMBEDDING_PROVIDER_ID
        || metadata.provider.requested_model_id.is_empty()
        || metadata.provider.requested_model_id.len() > 256
        || metadata.provider.canonical_model_slug.is_empty()
        || metadata.provider.canonical_model_slug.len() > 256
        || metadata.provider.resolved_model_id.is_empty()
        || metadata.provider.resolved_model_id.len() > 256
        || metadata.provider.measurement_provenance != "provider_reported"
    {
        return Err(failure("provider embedding identity is invalid"));
    }
    if metadata.provider.pricing.currency != "USD"
        || metadata.provider.pricing.source != "provider_catalog_reported"
        || metadata.provider.pricing.prompt_cost_per_token_usd != 0.0
        || metadata.provider.pricing.completion_cost_per_token_usd != 0.0
        || chrono::NaiveDate::parse_from_str(&metadata.provider.pricing.effective_date, "%Y-%m-%d")
            .is_err()
        || metadata
            .provider
            .input_tokens
            .is_some_and(|value| value < 0)
        || metadata
            .provider
            .cost_usd
            .is_some_and(|value| !value.is_finite() || value != 0.0)
    {
        return Err(failure("provider embedding pricing or usage is invalid"));
    }
    if metadata.tenant_id != row.tenant_id
        || metadata.workspace_id != row.workspace_id
        || metadata.agent_id != row.agent_id
        || metadata.run_id != row.run_id
        || metadata.task_id != row.task_id
        || metadata.memory_id != row.memory_id
        || metadata.memory_version != row.version
        || metadata.source_id != row.source_id
        || metadata.source_sha256 != row.source_sha256
    {
        return Err(failure(
            "provider embedding scope/source/version binding mismatch",
        ));
    }
    let normalized_content = content
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if metadata.provider.normalized_content_sha256 != sha256_bytes(normalized_content.as_bytes()) {
        return Err(failure("provider embedding content hash mismatch"));
    }
    let vector_hash =
        sha256_json(&serde_json::to_value(values).map_err(|error| error.to_string())?)?;
    if metadata.provider.vector_sha256 != vector_hash {
        return Err(failure("provider embedding vector hash mismatch"));
    }
    let expected_binding =
        sha256_json(&serde_json::to_value(metadata).map_err(|error| error.to_string())?)?;
    if binding != expected_binding {
        return Err(failure("provider embedding metadata binding hash mismatch"));
    }
    Ok(())
}

fn validate_durable_memory_record_hash(
    row: &DurableMemoryIntegrityRow,
    content: &Value,
    embedding: Option<&Vec<f64>>,
    metadata: Option<&DurableMemoryEmbeddingBinding>,
) -> Result<(), String> {
    let scope = json!({
        "tenant_id": row.tenant_id,
        "workspace_id": row.workspace_id,
        "agent_id": row.agent_id,
        "task_id": row.task_id,
    });
    let current = json!({
        "schema_version": super::durable_memory::DURABLE_MEMORY_SCHEMA_VERSION,
        "memory_id": row.memory_id,
        "version": row.version,
        "scope": scope,
        "run_id": row.run_id,
        "source_id": row.source_id,
        "source_sha256": row.source_sha256,
        "conflict_key": row.conflict_key,
        "state": row.state,
        "confidence": row.confidence,
        "fresh_until": row.fresh_until,
        "expires_at": row.expires_at,
        "supersedes_memory_id": row.supersedes_memory_id,
        "content": content,
        "embedding": embedding,
        "embedding_provenance": row.embedding_provenance,
        "embedding_metadata": metadata,
        "embedding_binding_sha256": row.embedding_binding_sha256,
        "created_at": row.created_at,
        "created_by": row.created_by,
    });
    if sha256_json(&current)? == row.record_sha256 {
        return Ok(());
    }
    if metadata.is_some() || row.embedding_binding_sha256.is_some() {
        return Err("durable memory record hash mismatch".to_string());
    }
    let legacy = json!({
        "schema_version": super::durable_memory::DURABLE_MEMORY_SCHEMA_VERSION,
        "memory_id": row.memory_id,
        "version": row.version,
        "scope": scope,
        "run_id": row.run_id,
        "source_id": row.source_id,
        "source_sha256": row.source_sha256,
        "conflict_key": row.conflict_key,
        "state": row.state,
        "confidence": row.confidence,
        "fresh_until": row.fresh_until,
        "expires_at": row.expires_at,
        "supersedes_memory_id": row.supersedes_memory_id,
        "content": content,
        "embedding": embedding,
        "embedding_provenance": row.embedding_provenance,
        "created_at": row.created_at,
        "created_by": row.created_by,
    });
    if sha256_json(&legacy)? == row.record_sha256 {
        Ok(())
    } else {
        Err("durable memory record hash mismatch".to_string())
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256_json(value: &Value) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::embedding::{
        OPENROUTER_EMBEDDING_CANONICAL_SLUG, OPENROUTER_EMBEDDING_MODEL_ID,
    };
    use rusqlite::params;
    use tempfile::tempdir;

    fn new_store() -> (tempfile::TempDir, LocalProductStore) {
        let directory = tempdir().unwrap();
        let store = LocalProductStore::new(directory.path().join("integrity.db")).unwrap();
        (directory, store)
    }

    fn insert_embedding_row(
        store: &LocalProductStore,
        suffix: &str,
        embedding_json: &str,
        metadata_json: &str,
        binding_sha256: &str,
    ) {
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "INSERT INTO durable_memory_versions
                         (memory_id,version,tenant_id,workspace_id,source_id,source_sha256,
                          conflict_key,state,confidence,content_json,embedding_json,
                          embedding_provenance,embedding_metadata_json,embedding_binding_sha256,
                          record_sha256,created_at,created_by)
                         VALUES (?1,1,'tenant','workspace','source',?2,'fact','current',1.0,
                                 '{}',?3,'provider_reported',?4,?5,?6,
                                 '2026-07-15T00:00:00Z','integrity-test')",
                        params![
                            format!("memory-{suffix}"),
                            "a".repeat(64),
                            embedding_json,
                            metadata_json,
                            binding_sha256,
                            "b".repeat(64),
                        ],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
    }

    fn provider_binding(vector_sha256: &str) -> DurableMemoryEmbeddingBinding {
        DurableMemoryEmbeddingBinding {
            provider: ProviderEmbeddingMetadata {
                provider_id: OPENROUTER_EMBEDDING_PROVIDER_ID.to_string(),
                requested_model_id: OPENROUTER_EMBEDDING_MODEL_ID.to_string(),
                canonical_model_slug: OPENROUTER_EMBEDDING_CANONICAL_SLUG.to_string(),
                resolved_model_id: "private/openrouter/nvidia/llama-nemotron-embed-vl-1b-v2"
                    .to_string(),
                dimensions: OPENROUTER_EMBEDDING_DIMENSIONS,
                input_tokens: Some(1),
                cost_usd: Some(0.0),
                pricing: crate::provider::embedding::EmbeddingPricingEvidence {
                    prompt_cost_per_token_usd: 0.0,
                    completion_cost_per_token_usd: 0.0,
                    currency: "USD".to_string(),
                    effective_date: "2026-07-15".to_string(),
                    source: "provider_catalog_reported".to_string(),
                },
                measurement_provenance: "provider_reported".to_string(),
                normalized_content_sha256: sha256_bytes(b"{}"),
                vector_sha256: vector_sha256.to_string(),
            },
            tenant_id: "tenant".to_string(),
            workspace_id: "workspace".to_string(),
            agent_id: None,
            run_id: None,
            task_id: None,
            memory_id: "memory-binding".to_string(),
            memory_version: 1,
            source_id: "source".to_string(),
            source_sha256: "a".repeat(64),
        }
    }

    #[test]
    fn integrity_check_rejects_malformed_provider_metadata() {
        let (_directory, store) = new_store();
        assert_eq!(store.check_integrity().unwrap().status, "ok");
        insert_embedding_row(&store, "metadata", "[0.1]", "{}", &"c".repeat(64));
        assert!(store
            .check_integrity()
            .unwrap_err()
            .contains("metadata JSON is malformed"));
    }

    #[test]
    fn integrity_check_rejects_malformed_provider_vector() {
        let (_directory, store) = new_store();
        insert_embedding_row(&store, "vector", "not-json", "{}", &"c".repeat(64));
        assert!(store
            .check_integrity()
            .unwrap_err()
            .contains("vector JSON is malformed"));
    }

    #[test]
    fn integrity_check_rejects_vector_and_metadata_binding_hash_mismatch() {
        let vector = vec![0.25; OPENROUTER_EMBEDDING_DIMENSIONS];
        let vector_json = serde_json::to_string(&vector).unwrap();

        let (_directory, store) = new_store();
        let wrong_vector = provider_binding(&"d".repeat(64));
        insert_embedding_row(
            &store,
            "binding",
            &vector_json,
            &serde_json::to_string(&wrong_vector).unwrap(),
            &"c".repeat(64),
        );
        assert!(store
            .check_integrity()
            .unwrap_err()
            .contains("vector hash mismatch"));

        let (_directory, store) = new_store();
        let correct_vector = provider_binding(&sha256_bytes(vector_json.as_bytes()));
        insert_embedding_row(
            &store,
            "binding",
            &vector_json,
            &serde_json::to_string(&correct_vector).unwrap(),
            &"c".repeat(64),
        );
        assert!(store
            .check_integrity()
            .unwrap_err()
            .contains("metadata binding hash mismatch"));
    }
}
