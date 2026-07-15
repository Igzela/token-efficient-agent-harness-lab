use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{count_table, DatabaseConnection, LocalProductStore};
use crate::provider::embedding::{
    is_supported_durable_embedding_contract, EmbeddingContractEvidence, ProviderEmbeddingMetadata,
};
#[cfg(test)]
use crate::provider::embedding::{
    OPENROUTER_EMBEDDING_CANONICAL_SLUG, OPENROUTER_EMBEDDING_DIMENSIONS,
    OPENROUTER_EMBEDDING_MODEL_ID, OPENROUTER_EMBEDDING_PRICING_SOURCE,
    OPENROUTER_EMBEDDING_PROVIDER_ID, OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,
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
    "provider_embedding_operations",
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

#[derive(Debug)]
struct ProviderEmbeddingOperationIntegrityRow {
    operation_id: String,
    target_memory_id: String,
    target_version: i64,
    tenant_id: String,
    workspace_id: String,
    agent_id: Option<String>,
    run_id: Option<String>,
    task_id: Option<String>,
    source_id: String,
    source_sha256: String,
    operation_binding_sha256: String,
    content_sha256: String,
    contract_json: String,
    contract_sha256: String,
    receipt_sha256: String,
    provider_id: String,
    model_id: String,
    state: String,
    attempt_count: i64,
    vector_json: Option<String>,
    metadata_json: Option<String>,
    created_at: String,
    updated_at: String,
}

impl LocalProductStore {
    pub fn check_integrity(&self) -> Result<IntegrityReport, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let status: String = conn
                    .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                validate_sqlite_durable_memory_rows(conn)?;
                validate_sqlite_provider_embedding_operations(conn)?;

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
                validate_pg_provider_embedding_operations(client)?;

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

fn validate_sqlite_provider_embedding_operations(
    conn: &rusqlite::Connection,
) -> Result<(), String> {
    let mut statement = conn
        .prepare(
            "SELECT operation_id,target_memory_id,target_version,tenant_id,workspace_id,agent_id,run_id,
                    task_id,source_id,source_sha256,operation_binding_sha256,content_sha256,
                    contract_json,contract_sha256,receipt_sha256,provider_id,model_id,state,
                    attempt_count,vector_json,metadata_json,created_at,updated_at
             FROM provider_embedding_operations ORDER BY operation_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(ProviderEmbeddingOperationIntegrityRow {
                operation_id: row.get(0)?,
                target_memory_id: row.get(1)?,
                target_version: row.get(2)?,
                tenant_id: row.get(3)?,
                workspace_id: row.get(4)?,
                agent_id: row.get(5)?,
                run_id: row.get(6)?,
                task_id: row.get(7)?,
                source_id: row.get(8)?,
                source_sha256: row.get(9)?,
                operation_binding_sha256: row.get(10)?,
                content_sha256: row.get(11)?,
                contract_json: row.get(12)?,
                contract_sha256: row.get(13)?,
                receipt_sha256: row.get(14)?,
                provider_id: row.get(15)?,
                model_id: row.get(16)?,
                state: row.get(17)?,
                attempt_count: row.get(18)?,
                vector_json: row.get(19)?,
                metadata_json: row.get(20)?,
                created_at: row.get(21)?,
                updated_at: row.get(22)?,
            })
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        validate_provider_embedding_operation_integrity(&row.map_err(|error| error.to_string())?)?;
    }
    Ok(())
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

#[cfg(feature = "pg")]
fn validate_pg_provider_embedding_operations(client: &mut postgres::Client) -> Result<(), String> {
    let rows = client
        .query(
            "SELECT operation_id,target_memory_id,target_version,tenant_id,workspace_id,agent_id,run_id,
                    task_id,source_id,source_sha256,operation_binding_sha256,content_sha256,
                    contract_json,contract_sha256,receipt_sha256,provider_id,model_id,state,
                    attempt_count,vector_json,metadata_json,created_at,updated_at
             FROM provider_embedding_operations ORDER BY operation_id",
            &[],
        )
        .map_err(|error| error.to_string())?;
    for row in rows {
        validate_provider_embedding_operation_integrity(&ProviderEmbeddingOperationIntegrityRow {
            operation_id: row.get(0),
            target_memory_id: row.get(1),
            target_version: row.get(2),
            tenant_id: row.get(3),
            workspace_id: row.get(4),
            agent_id: row.get(5),
            run_id: row.get(6),
            task_id: row.get(7),
            source_id: row.get(8),
            source_sha256: row.get(9),
            operation_binding_sha256: row.get(10),
            content_sha256: row.get(11),
            contract_json: row.get(12),
            contract_sha256: row.get(13),
            receipt_sha256: row.get(14),
            provider_id: row.get(15),
            model_id: row.get(16),
            state: row.get(17),
            attempt_count: row.get(18),
            vector_json: row.get(19),
            metadata_json: row.get(20),
            created_at: row.get(21),
            updated_at: row.get(22),
        })?;
    }
    Ok(())
}

fn validate_provider_embedding_operation_integrity(
    row: &ProviderEmbeddingOperationIntegrityRow,
) -> Result<(), String> {
    let failure = |detail: &str| {
        format!(
            "provider embedding operation integrity failure for {}: {detail}",
            row.operation_id
        )
    };
    if row.target_memory_id.is_empty()
        || row.target_version <= 0
        || row.tenant_id.is_empty()
        || row.workspace_id.is_empty()
        || row.source_id.is_empty()
        || !is_sha256(&row.source_sha256)
        || !is_sha256(&row.operation_binding_sha256)
        || !is_sha256(&row.content_sha256)
        || !is_sha256(&row.contract_sha256)
        || !is_sha256(&row.receipt_sha256)
        || row.operation_id != format!("embedding-operation-{}", row.operation_binding_sha256)
    {
        return Err(failure("operation identity or hash binding is invalid"));
    }
    let contract: EmbeddingContractEvidence = serde_json::from_str(&row.contract_json)
        .map_err(|_| failure("contract evidence JSON is malformed"))?;
    if sha256_bytes(row.contract_json.as_bytes()) != row.contract_sha256
        || contract.provider_id != row.provider_id
        || contract.requested_model_id != row.model_id
        || !is_supported_durable_embedding_contract(&contract)
    {
        return Err(failure("contract evidence binding is invalid"));
    }
    let expected_receipt_sha256 = provider_embedding_operation_receipt_sha256(row)?;
    if row.receipt_sha256 != expected_receipt_sha256 {
        return Err(failure("operation receipt hash binding is invalid"));
    }
    if !matches!(
        row.state.as_str(),
        "request_sent"
            | "completed"
            | "failed"
            | "outcome_unknown"
            | "outcome_unknown_acknowledged"
            | "retry_authorized"
    ) {
        return Err(failure("operation state is invalid"));
    }
    if !(1..=4).contains(&row.attempt_count) {
        return Err(failure("operation attempt count is invalid"));
    }
    let created_at = chrono::DateTime::parse_from_rfc3339(&row.created_at)
        .map_err(|_| failure("operation timestamp is invalid"))?;
    let updated_at = chrono::DateTime::parse_from_rfc3339(&row.updated_at)
        .map_err(|_| failure("operation timestamp is invalid"))?;
    if updated_at < created_at {
        return Err(failure("operation timestamp ordering is invalid"));
    }

    match (
        row.state.as_str(),
        row.vector_json.as_deref(),
        row.metadata_json.as_deref(),
    ) {
        ("completed", Some(vector_json), Some(metadata_json)) => {
            let values: Vec<f64> = serde_json::from_str(vector_json)
                .map_err(|_| failure("completed vector JSON is malformed"))?;
            let metadata: ProviderEmbeddingMetadata = serde_json::from_str(metadata_json)
                .map_err(|_| failure("completed metadata JSON is malformed"))?;
            validate_provider_embedding_metadata(&metadata, &values, &row.content_sha256)
                .map_err(|detail| failure(&detail))?;
            if metadata.provider_id != row.provider_id
                || metadata.requested_model_id != row.model_id
            {
                return Err(failure(
                    "completed metadata provider/model does not match operation",
                ));
            }
        }
        ("completed", _, _) => {
            return Err(failure("completed operation receipt is incomplete"));
        }
        (_, None, None) => {}
        (_, _, _) => {
            return Err(failure(
                "non-completed operation must not retain vector or metadata",
            ));
        }
    }
    Ok(())
}

fn provider_embedding_operation_receipt_sha256(
    row: &ProviderEmbeddingOperationIntegrityRow,
) -> Result<String, String> {
    sha256_json(&json!({
        "operation_id": row.operation_id,
        "target_memory_id": row.target_memory_id,
        "target_version": row.target_version,
        "tenant_id":row.tenant_id,
        "workspace_id":row.workspace_id,
        "agent_id":row.agent_id,
        "run_id":row.run_id,
        "task_id":row.task_id,
        "source_id":row.source_id,
        "source_sha256":row.source_sha256,
        "operation_binding_sha256": row.operation_binding_sha256,
        "content_sha256": row.content_sha256,
        "contract_sha256":row.contract_sha256,
        "provider_id": row.provider_id,
        "model_id": row.model_id,
    }))
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
    let normalized_content = content
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    validate_provider_embedding_metadata(
        &metadata.provider,
        values,
        &sha256_bytes(normalized_content.as_bytes()),
    )
    .map_err(|detail| failure(&detail))?;
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
    let expected_binding =
        sha256_json(&serde_json::to_value(metadata).map_err(|error| error.to_string())?)?;
    if binding != expected_binding {
        return Err(failure("provider embedding metadata binding hash mismatch"));
    }
    Ok(())
}

fn validate_provider_embedding_metadata(
    metadata: &ProviderEmbeddingMetadata,
    values: &[f64],
    content_sha256: &str,
) -> Result<(), String> {
    if values.len() != metadata.dimensions || values.iter().any(|value| !value.is_finite()) {
        return Err("provider embedding dimension mismatch".to_string());
    }
    if !crate::provider::embedding::is_supported_durable_embedding_identity(metadata)
        || metadata.measurement_provenance != "provider_reported"
    {
        return Err("provider embedding identity is invalid".to_string());
    }
    if metadata.input_tokens.is_some_and(|value| value < 0)
        || metadata
            .cost_usd
            .is_some_and(|value| !value.is_finite() || value < 0.0)
    {
        return Err("provider embedding pricing or usage is invalid".to_string());
    }
    if metadata.normalized_content_sha256 != content_sha256 {
        return Err("provider embedding content hash mismatch".to_string());
    }
    let vector_hash =
        sha256_json(&serde_json::to_value(values).map_err(|error| error.to_string())?)?;
    if metadata.vector_sha256 != vector_hash {
        return Err("provider embedding vector hash mismatch".to_string());
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

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn sha256_json(value: &Value) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
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
                resolved_model_id: OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID.to_string(),
                dimensions: OPENROUTER_EMBEDDING_DIMENSIONS,
                input_tokens: Some(1),
                cost_usd: Some(0.0),
                pricing: crate::provider::embedding::EmbeddingPricingEvidence {
                    prompt_cost_per_token_usd: 0.0,
                    completion_cost_per_token_usd: 0.0,
                    currency: "USD".to_string(),
                    effective_date: "2026-07-15".to_string(),
                    source: OPENROUTER_EMBEDDING_PRICING_SOURCE.to_string(),
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

    #[allow(clippy::too_many_arguments)]
    fn insert_provider_embedding_operation(
        store: &LocalProductStore,
        operation_id: &str,
        binding_sha256: &str,
        content_sha256: &str,
        provider_id: &str,
        model_id: &str,
        state: &str,
        vector_json: Option<&str>,
        metadata_json: Option<&str>,
        ignore_checks: bool,
    ) {
        let contract_json = serde_json::to_string(&EmbeddingContractEvidence::current(
            crate::provider::embedding::EmbeddingPricingEvidence {
                prompt_cost_per_token_usd: 0.0,
                completion_cost_per_token_usd: 0.0,
                currency: "USD".to_string(),
                effective_date: "2026-07-15".to_string(),
                source: OPENROUTER_EMBEDDING_PRICING_SOURCE.to_string(),
            },
        ))
        .unwrap();
        let contract_sha256 = sha256_bytes(contract_json.as_bytes());
        let receipt_sha256 = sha256_json(&json!({
            "operation_id": operation_id,
            "target_memory_id": "memory-operation",
            "target_version": 1,
            "tenant_id":"tenant",
            "workspace_id":"workspace",
            "agent_id":null,
            "run_id":null,
            "task_id":null,
            "source_id":"source",
            "source_sha256":"d".repeat(64),
            "operation_binding_sha256": binding_sha256,
            "content_sha256": content_sha256,
            "contract_sha256":contract_sha256,
            "provider_id": provider_id,
            "model_id": model_id,
        }))
        .unwrap();
        store
            .with_conn(|connection| {
                if ignore_checks {
                    connection
                        .execute_batch("PRAGMA ignore_check_constraints = ON;")
                        .map_err(|error| error.to_string())?;
                }
                connection
                    .execute(
                        "INSERT INTO provider_embedding_operations
                         (operation_id,target_memory_id,target_version,tenant_id,workspace_id,source_id,
                          source_sha256,operation_binding_sha256,content_sha256,contract_json,
                          contract_sha256,receipt_sha256,provider_id,model_id,state,attempt_count,
                          vector_json,metadata_json,created_at,updated_at)
                         VALUES (?1,'memory-operation',1,'tenant','workspace','source',?2,?3,?4,?5,?6,?7,?8,?9,?10,1,?11,?12,
                                 '2026-07-15T00:00:00Z','2026-07-15T00:00:01Z')",
                        params![
                            operation_id,
                            "d".repeat(64),
                            binding_sha256,
                            content_sha256,
                            contract_json,
                            contract_sha256,
                            receipt_sha256,
                            provider_id,
                            model_id,
                            state,
                            vector_json,
                            metadata_json,
                        ],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
    }

    fn valid_operation_receipt() -> (String, String, String, String) {
        let binding_sha256 = "c".repeat(64);
        let content_sha256 = "a".repeat(64);
        let vector_json =
            serde_json::to_string(&vec![0.25; OPENROUTER_EMBEDDING_DIMENSIONS]).unwrap();
        let mut metadata = provider_binding(&sha256_bytes(vector_json.as_bytes())).provider;
        metadata.normalized_content_sha256 = content_sha256.clone();
        (
            format!("embedding-operation-{binding_sha256}"),
            binding_sha256,
            content_sha256,
            serde_json::to_string(&metadata).unwrap(),
        )
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

    #[test]
    fn integrity_check_rejects_non_pinned_provider_model_identity() {
        let vector = vec![0.25; OPENROUTER_EMBEDDING_DIMENSIONS];
        let vector_json = serde_json::to_string(&vector).unwrap();
        for identity_field in ["provider", "requested", "canonical", "resolved"] {
            let mut metadata = provider_binding(&sha256_bytes(vector_json.as_bytes()));
            match identity_field {
                "provider" => metadata.provider.provider_id = "another-provider".to_string(),
                "requested" => {
                    metadata.provider.requested_model_id =
                        "nvidia/another-embedding-model:free".to_string()
                }
                "canonical" => {
                    metadata.provider.canonical_model_slug =
                        "nvidia/another-embedding-model".to_string()
                }
                "resolved" => {
                    metadata.provider.resolved_model_id =
                        "private/openrouter/nvidia/another-embedding-model".to_string()
                }
                _ => unreachable!(),
            }
            let metadata_json = serde_json::to_string(&metadata).unwrap();
            let binding_sha256 = sha256_json(&serde_json::to_value(&metadata).unwrap()).unwrap();

            let (_directory, store) = new_store();
            insert_embedding_row(
                &store,
                identity_field,
                &vector_json,
                &metadata_json,
                &binding_sha256,
            );
            assert!(store
                .check_integrity()
                .unwrap_err()
                .contains("provider embedding identity is invalid"));
        }
    }

    #[test]
    fn integrity_check_rejects_provider_vector_below_pinned_dimension() {
        let vector = vec![0.25; OPENROUTER_EMBEDDING_DIMENSIONS - 1];
        let vector_json = serde_json::to_string(&vector).unwrap();
        let mut metadata = provider_binding(&sha256_bytes(vector_json.as_bytes()));
        metadata.provider.dimensions = vector.len();
        let metadata_json = serde_json::to_string(&metadata).unwrap();
        let binding_sha256 = sha256_json(&serde_json::to_value(&metadata).unwrap()).unwrap();

        let (_directory, store) = new_store();
        insert_embedding_row(
            &store,
            "dimension",
            &vector_json,
            &metadata_json,
            &binding_sha256,
        );
        assert!(store
            .check_integrity()
            .unwrap_err()
            .contains("provider embedding dimension mismatch"));
    }

    #[test]
    fn integrity_check_accepts_valid_completed_provider_embedding_operation() {
        let (operation_id, binding, content, metadata_json) = valid_operation_receipt();
        let vector_json =
            serde_json::to_string(&vec![0.25; OPENROUTER_EMBEDDING_DIMENSIONS]).unwrap();
        let (_directory, store) = new_store();
        insert_provider_embedding_operation(
            &store,
            &operation_id,
            &binding,
            &content,
            OPENROUTER_EMBEDDING_PROVIDER_ID,
            OPENROUTER_EMBEDDING_MODEL_ID,
            "completed",
            Some(&vector_json),
            Some(&metadata_json),
            false,
        );
        assert_eq!(store.check_integrity().unwrap().status, "ok");
    }

    #[test]
    fn integrity_check_rejects_invalid_provider_embedding_operation_state() {
        let (operation_id, binding, content, _) = valid_operation_receipt();
        let (_directory, store) = new_store();
        insert_provider_embedding_operation(
            &store,
            &operation_id,
            &binding,
            &content,
            OPENROUTER_EMBEDDING_PROVIDER_ID,
            OPENROUTER_EMBEDDING_MODEL_ID,
            "corrupt",
            None,
            None,
            true,
        );
        assert!(store
            .check_integrity()
            .unwrap_err()
            .contains("operation state is invalid"));
    }

    #[test]
    fn integrity_check_rejects_provider_embedding_operation_identity_and_hash_corruption() {
        let (operation_id, binding, content, _) = valid_operation_receipt();
        for (
            suffix,
            corrupt_operation_id,
            corrupt_binding,
            corrupt_content,
            provider_id,
            model_id,
        ) in [
            (
                "binding",
                format!("embedding-operation-{}", "d".repeat(64)),
                binding.clone(),
                content.clone(),
                OPENROUTER_EMBEDDING_PROVIDER_ID,
                OPENROUTER_EMBEDDING_MODEL_ID,
            ),
            (
                "malformed-binding",
                format!("embedding-operation-{}", "C".repeat(64)),
                "C".repeat(64),
                content.clone(),
                OPENROUTER_EMBEDDING_PROVIDER_ID,
                OPENROUTER_EMBEDDING_MODEL_ID,
            ),
            (
                "malformed-content",
                operation_id.clone(),
                binding.clone(),
                "A".repeat(64),
                OPENROUTER_EMBEDDING_PROVIDER_ID,
                OPENROUTER_EMBEDDING_MODEL_ID,
            ),
            (
                "provider",
                operation_id.clone(),
                binding.clone(),
                content.clone(),
                "another-provider",
                OPENROUTER_EMBEDDING_MODEL_ID,
            ),
            (
                "model",
                operation_id.clone(),
                binding.clone(),
                content.clone(),
                OPENROUTER_EMBEDDING_PROVIDER_ID,
                "nvidia/another-embedding-model:free",
            ),
        ] {
            let (_directory, store) = new_store();
            insert_provider_embedding_operation(
                &store,
                &corrupt_operation_id,
                &corrupt_binding,
                &corrupt_content,
                provider_id,
                model_id,
                "failed",
                None,
                None,
                false,
            );
            let error = store.check_integrity().unwrap_err();
            assert!(
                error.contains("identity")
                    || error.contains("hash binding")
                    || error.contains("contract evidence binding"),
                "{suffix} corruption returned unexpected error: {error}"
            );
        }

        let (_directory, store) = new_store();
        insert_provider_embedding_operation(
            &store,
            &operation_id,
            &binding,
            &content,
            OPENROUTER_EMBEDDING_PROVIDER_ID,
            OPENROUTER_EMBEDDING_MODEL_ID,
            "failed",
            None,
            None,
            false,
        );
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "UPDATE provider_embedding_operations SET receipt_sha256=?1",
                        params!["d".repeat(64)],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert!(store
            .check_integrity()
            .unwrap_err()
            .contains("receipt hash binding is invalid"));

        for mutation in [
            "UPDATE provider_embedding_operations SET target_memory_id='memory-other'",
            "UPDATE provider_embedding_operations SET target_version=2",
        ] {
            let (_directory, store) = new_store();
            insert_provider_embedding_operation(
                &store,
                &operation_id,
                &binding,
                &content,
                OPENROUTER_EMBEDDING_PROVIDER_ID,
                OPENROUTER_EMBEDDING_MODEL_ID,
                "failed",
                None,
                None,
                false,
            );
            store
                .with_conn(|connection| {
                    connection
                        .execute(mutation, [])
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                })
                .unwrap();
            assert!(store
                .check_integrity()
                .unwrap_err()
                .contains("receipt hash binding is invalid"));
        }
    }

    #[test]
    fn integrity_check_rejects_malformed_provider_embedding_operation_receipt() {
        let (operation_id, binding, content, metadata_json) = valid_operation_receipt();
        for (suffix, vector_json, metadata) in [
            ("vector", "not-json", metadata_json.as_str()),
            ("metadata", "[]", "{}"),
        ] {
            let (_directory, store) = new_store();
            insert_provider_embedding_operation(
                &store,
                &operation_id,
                &binding,
                &content,
                OPENROUTER_EMBEDDING_PROVIDER_ID,
                OPENROUTER_EMBEDDING_MODEL_ID,
                "completed",
                Some(vector_json),
                Some(metadata),
                false,
            );
            let error = store.check_integrity().unwrap_err();
            assert!(
                error.contains("malformed"),
                "{suffix} corruption returned unexpected error: {error}"
            );
        }
    }

    #[test]
    fn integrity_check_rejects_provider_embedding_operation_receipt_state_mismatch() {
        let (operation_id, binding, content, metadata_json) = valid_operation_receipt();
        let vector_json =
            serde_json::to_string(&vec![0.25; OPENROUTER_EMBEDDING_DIMENSIONS]).unwrap();
        for (state, vector, metadata, expected) in [
            ("completed", None, None, "receipt is incomplete"),
            (
                "completed",
                Some(vector_json.as_str()),
                None,
                "receipt is incomplete",
            ),
            (
                "completed",
                None,
                Some(metadata_json.as_str()),
                "receipt is incomplete",
            ),
            (
                "request_sent",
                Some(vector_json.as_str()),
                Some(metadata_json.as_str()),
                "must not retain vector or metadata",
            ),
        ] {
            let (_directory, store) = new_store();
            insert_provider_embedding_operation(
                &store,
                &operation_id,
                &binding,
                &content,
                OPENROUTER_EMBEDDING_PROVIDER_ID,
                OPENROUTER_EMBEDDING_MODEL_ID,
                state,
                vector,
                metadata,
                false,
            );
            assert!(store.check_integrity().unwrap_err().contains(expected));
        }
    }

    #[test]
    fn integrity_check_rejects_completed_operation_content_and_vector_hash_mismatch() {
        let (operation_id, binding, content, metadata_json) = valid_operation_receipt();
        let vector_json =
            serde_json::to_string(&vec![0.25; OPENROUTER_EMBEDDING_DIMENSIONS]).unwrap();
        for (suffix, row_content, receipt_vector, metadata, expected) in [
            (
                "content",
                "b".repeat(64),
                vector_json.clone(),
                metadata_json.clone(),
                "content hash mismatch",
            ),
            (
                "vector",
                content.clone(),
                serde_json::to_string(&vec![0.5; OPENROUTER_EMBEDDING_DIMENSIONS]).unwrap(),
                metadata_json.clone(),
                "vector hash mismatch",
            ),
        ] {
            let (_directory, store) = new_store();
            insert_provider_embedding_operation(
                &store,
                &operation_id,
                &binding,
                &row_content,
                OPENROUTER_EMBEDDING_PROVIDER_ID,
                OPENROUTER_EMBEDDING_MODEL_ID,
                "completed",
                Some(&receipt_vector),
                Some(&metadata),
                false,
            );
            let error = store.check_integrity().unwrap_err();
            assert!(
                error.contains(expected),
                "{suffix} corruption returned unexpected error: {error}"
            );
        }
    }

    #[test]
    fn integrity_check_rejects_completed_operation_metadata_corruption() {
        let (operation_id, binding, content, metadata_json) = valid_operation_receipt();
        let vector_json =
            serde_json::to_string(&vec![0.25; OPENROUTER_EMBEDDING_DIMENSIONS]).unwrap();
        let base: ProviderEmbeddingMetadata = serde_json::from_str(&metadata_json).unwrap();
        for (suffix, metadata, receipt_vector, expected) in [
            (
                "provider",
                ProviderEmbeddingMetadata {
                    provider_id: "another-provider".to_string(),
                    ..base.clone()
                },
                vector_json.clone(),
                "identity is invalid",
            ),
            (
                "model",
                ProviderEmbeddingMetadata {
                    requested_model_id: "nvidia/another-embedding-model:free".to_string(),
                    ..base.clone()
                },
                vector_json.clone(),
                "identity is invalid",
            ),
            (
                "dimension",
                ProviderEmbeddingMetadata {
                    dimensions: OPENROUTER_EMBEDDING_DIMENSIONS - 1,
                    ..base.clone()
                },
                vector_json.clone(),
                "dimension mismatch",
            ),
            (
                "non-finite",
                base.clone(),
                format!(
                    "[1e400,{}]",
                    std::iter::repeat_n("0.25", OPENROUTER_EMBEDDING_DIMENSIONS - 1)
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                "vector JSON is malformed",
            ),
        ] {
            let (_directory, store) = new_store();
            insert_provider_embedding_operation(
                &store,
                &operation_id,
                &binding,
                &content,
                OPENROUTER_EMBEDDING_PROVIDER_ID,
                OPENROUTER_EMBEDDING_MODEL_ID,
                "completed",
                Some(&receipt_vector),
                Some(&serde_json::to_string(&metadata).unwrap()),
                false,
            );
            let error = store.check_integrity().unwrap_err();
            assert!(
                error.contains(expected),
                "{suffix} corruption returned unexpected error: {error}"
            );
        }
    }
}
