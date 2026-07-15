use rusqlite::params;
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
    operation_kind: String,
    target_memory_id: String,
    target_version: i64,
    tenant_id: String,
    workspace_id: String,
    agent_id: Option<String>,
    run_id: Option<String>,
    task_id: Option<String>,
    source_id: String,
    source_sha256: String,
    node_id: Option<String>,
    query_sha256: Option<String>,
    request_identity_sha256: String,
    operation_binding_sha256: String,
    content_sha256: String,
    contract_json: String,
    contract_sha256: String,
    receipt_sha256: String,
    provider_id: String,
    requested_model_id: String,
    resolved_model_id: String,
    dimensions: i64,
    reservation_event_id: String,
    send_event_id: Option<String>,
    outcome_event_id: Option<String>,
    result_kind: Option<String>,
    result_id: Option<String>,
    result_sha256: Option<String>,
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
            "SELECT operation_id,operation_kind,target_memory_id,target_version,tenant_id,workspace_id,
                    agent_id,run_id,task_id,source_id,source_sha256,node_id,query_sha256,
                    request_identity_sha256,operation_binding_sha256,content_sha256,contract_json,
                    contract_sha256,receipt_sha256,provider_id,requested_model_id,resolved_model_id,
                    dimensions,reservation_event_id,send_event_id,outcome_event_id,result_kind,result_id,
                    result_sha256,state,attempt_count,vector_json,metadata_json,created_at,updated_at
             FROM provider_embedding_operations ORDER BY operation_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(ProviderEmbeddingOperationIntegrityRow {
                operation_id: row.get(0)?,
                operation_kind: row.get(1)?,
                target_memory_id: row.get(2)?,
                target_version: row.get(3)?,
                tenant_id: row.get(4)?,
                workspace_id: row.get(5)?,
                agent_id: row.get(6)?,
                run_id: row.get(7)?,
                task_id: row.get(8)?,
                source_id: row.get(9)?,
                source_sha256: row.get(10)?,
                node_id: row.get(11)?,
                query_sha256: row.get(12)?,
                request_identity_sha256: row.get(13)?,
                operation_binding_sha256: row.get(14)?,
                content_sha256: row.get(15)?,
                contract_json: row.get(16)?,
                contract_sha256: row.get(17)?,
                receipt_sha256: row.get(18)?,
                provider_id: row.get(19)?,
                requested_model_id: row.get(20)?,
                resolved_model_id: row.get(21)?,
                dimensions: row.get(22)?,
                reservation_event_id: row.get(23)?,
                send_event_id: row.get(24)?,
                outcome_event_id: row.get(25)?,
                result_kind: row.get(26)?,
                result_id: row.get(27)?,
                result_sha256: row.get(28)?,
                state: row.get(29)?,
                attempt_count: row.get(30)?,
                vector_json: row.get(31)?,
                metadata_json: row.get(32)?,
                created_at: row.get(33)?,
                updated_at: row.get(34)?,
            })
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let row = row.map_err(|error| error.to_string())?;
        validate_provider_embedding_operation_integrity(&row)?;
        validate_sqlite_provider_embedding_cross_owner(conn, &row)?;
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
            "SELECT operation_id,operation_kind,target_memory_id,target_version,tenant_id,workspace_id,
                    agent_id,run_id,task_id,source_id,source_sha256,node_id,query_sha256,
                    request_identity_sha256,operation_binding_sha256,content_sha256,contract_json,
                    contract_sha256,receipt_sha256,provider_id,requested_model_id,resolved_model_id,
                    dimensions,reservation_event_id,send_event_id,outcome_event_id,result_kind,result_id,
                    result_sha256,state,attempt_count,vector_json,metadata_json,created_at,updated_at
             FROM provider_embedding_operations ORDER BY operation_id",
            &[],
        )
        .map_err(|error| error.to_string())?;
    for row in rows {
        let operation = ProviderEmbeddingOperationIntegrityRow {
            operation_id: row.get(0),
            operation_kind: row.get(1),
            target_memory_id: row.get(2),
            target_version: row.get(3),
            tenant_id: row.get(4),
            workspace_id: row.get(5),
            agent_id: row.get(6),
            run_id: row.get(7),
            task_id: row.get(8),
            source_id: row.get(9),
            source_sha256: row.get(10),
            node_id: row.get(11),
            query_sha256: row.get(12),
            request_identity_sha256: row.get(13),
            operation_binding_sha256: row.get(14),
            content_sha256: row.get(15),
            contract_json: row.get(16),
            contract_sha256: row.get(17),
            receipt_sha256: row.get(18),
            provider_id: row.get(19),
            requested_model_id: row.get(20),
            resolved_model_id: row.get(21),
            dimensions: row.get(22),
            reservation_event_id: row.get(23),
            send_event_id: row.get(24),
            outcome_event_id: row.get(25),
            result_kind: row.get(26),
            result_id: row.get(27),
            result_sha256: row.get(28),
            state: row.get(29),
            attempt_count: row.get(30),
            vector_json: row.get(31),
            metadata_json: row.get(32),
            created_at: row.get(33),
            updated_at: row.get(34),
        };
        validate_provider_embedding_operation_integrity(&operation)?;
        validate_pg_provider_embedding_cross_owner(client, &operation)?;
    }
    Ok(())
}

fn validate_sqlite_provider_embedding_cross_owner(
    conn: &rusqlite::Connection,
    row: &ProviderEmbeddingOperationIntegrityRow,
) -> Result<(), String> {
    validate_sqlite_provider_event_binding(
        conn,
        &row.reservation_event_id,
        &row.provider_id,
        &["contract_check_reserved", "request_reserved"],
    )?;
    if let Some(event_id) = row.send_event_id.as_deref() {
        validate_sqlite_provider_event_binding(
            conn,
            event_id,
            &row.provider_id,
            &["request_sent"],
        )?;
    }
    if let Some(event_id) = row.outcome_event_id.as_deref() {
        validate_sqlite_provider_event_binding(
            conn,
            event_id,
            &row.provider_id,
            &["response_received", "error"],
        )?;
    }
    if row.state == super::provider_audit::ProviderEmbeddingReceiptState::ResultErased.as_str() {
        let tombstoned: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM durable_memory_versions
                 WHERE memory_id=?1 AND state='tombstoned')",
                params![row.target_memory_id],
                |value| value.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !tombstoned {
            return Err("erased provider embedding owner tombstone is missing".to_string());
        }
    }
    match (
        row.result_kind.as_deref(),
        row.result_id.as_deref(),
        row.result_sha256.as_deref(),
    ) {
        (Some("memory_version"), Some(result_id), Some(result_sha256)) => {
            let expected_id = format!("{}:{}", row.target_memory_id, row.target_version);
            let matches: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM durable_memory_versions
                 WHERE memory_id=?1 AND version=?2 AND tenant_id=?3 AND workspace_id=?4
                   AND agent_id IS ?5 AND run_id IS ?6 AND task_id IS ?7
                   AND source_id=?8 AND source_sha256=?9 AND embedding_binding_sha256=?10)",
                    params![
                        row.target_memory_id,
                        row.target_version,
                        row.tenant_id,
                        row.workspace_id,
                        row.agent_id,
                        row.run_id,
                        row.task_id,
                        row.source_id,
                        row.source_sha256,
                        result_sha256
                    ],
                    |value| value.get(0),
                )
                .map_err(|error| error.to_string())?;
            if result_id != expected_id || !matches {
                return Err(
                    "provider embedding memory result cross-owner binding is invalid".to_string(),
                );
            }
        }
        (Some("retrieval_event"), Some(result_id), Some(result_sha256)) => {
            let matches: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM memory_retrieval_events
                 WHERE retrieval_id=?1 AND tenant_id=?2 AND workspace_id=?3 AND run_id=?4
                   AND node_id=?5 AND result_sha256=?6)",
                    params![
                        result_id,
                        row.tenant_id,
                        row.workspace_id,
                        row.run_id,
                        row.node_id,
                        result_sha256
                    ],
                    |value| value.get(0),
                )
                .map_err(|error| error.to_string())?;
            if !matches {
                return Err(
                    "provider embedding retrieval result cross-owner binding is invalid"
                        .to_string(),
                );
            }
        }
        (None, None, None) => {}
        _ => return Err("provider embedding result cross-owner binding is invalid".to_string()),
    }
    Ok(())
}

fn validate_sqlite_provider_event_binding(
    conn: &rusqlite::Connection,
    event_id: &str,
    provider_id: &str,
    event_types: &[&str],
) -> Result<(), String> {
    let binding = conn
        .query_row(
            "SELECT provider_id,event_type FROM provider_audit_events WHERE event_id=?1",
            [event_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|_| "provider embedding audit cross-owner binding is missing".to_string())?;
    if binding.0 != provider_id || !event_types.contains(&binding.1.as_str()) {
        return Err("provider embedding audit cross-owner binding is invalid".to_string());
    }
    Ok(())
}

#[cfg(feature = "pg")]
fn validate_pg_provider_embedding_cross_owner(
    client: &mut postgres::Client,
    row: &ProviderEmbeddingOperationIntegrityRow,
) -> Result<(), String> {
    for (event_id, event_types) in [
        (
            Some(row.reservation_event_id.as_str()),
            &["contract_check_reserved", "request_reserved"][..],
        ),
        (row.send_event_id.as_deref(), &["request_sent"][..]),
        (
            row.outcome_event_id.as_deref(),
            &["response_received", "error"][..],
        ),
    ] {
        if let Some(event_id) = event_id {
            let event = client
                .query_opt(
                    "SELECT provider_id,event_type FROM provider_audit_events WHERE event_id=$1",
                    &[&event_id],
                )
                .map_err(|error| error.to_string())?
                .ok_or_else(|| {
                    "provider embedding audit cross-owner binding is missing".to_string()
                })?;
            let provider: String = event.get(0);
            let event_type: String = event.get(1);
            if provider != row.provider_id || !event_types.contains(&event_type.as_str()) {
                return Err("provider embedding audit cross-owner binding is invalid".to_string());
            }
        }
    }
    if row.state == super::provider_audit::ProviderEmbeddingReceiptState::ResultErased.as_str() {
        let tombstoned: bool = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM durable_memory_versions
                 WHERE memory_id=$1 AND state='tombstoned')",
                &[&row.target_memory_id],
            )
            .map_err(|error| error.to_string())?
            .get(0);
        if !tombstoned {
            return Err("erased provider embedding owner tombstone is missing".to_string());
        }
    }
    match (
        row.result_kind.as_deref(),
        row.result_id.as_deref(),
        row.result_sha256.as_deref(),
    ) {
        (Some("memory_version"), Some(result_id), Some(result_sha256)) => {
            let expected_id = format!("{}:{}", row.target_memory_id, row.target_version);
            let matches: bool = client
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM durable_memory_versions
                 WHERE memory_id=$1 AND version=$2 AND tenant_id=$3 AND workspace_id=$4
                   AND agent_id IS NOT DISTINCT FROM $5 AND run_id IS NOT DISTINCT FROM $6
                   AND task_id IS NOT DISTINCT FROM $7 AND source_id=$8 AND source_sha256=$9
                   AND embedding_binding_sha256=$10)",
                    &[
                        &row.target_memory_id,
                        &row.target_version,
                        &row.tenant_id,
                        &row.workspace_id,
                        &row.agent_id,
                        &row.run_id,
                        &row.task_id,
                        &row.source_id,
                        &row.source_sha256,
                        &result_sha256,
                    ],
                )
                .map_err(|error| error.to_string())?
                .get(0);
            if result_id != expected_id || !matches {
                return Err(
                    "provider embedding memory result cross-owner binding is invalid".to_string(),
                );
            }
        }
        (Some("retrieval_event"), Some(result_id), Some(result_sha256)) => {
            let matches: bool = client
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM memory_retrieval_events
                 WHERE retrieval_id=$1 AND tenant_id=$2 AND workspace_id=$3 AND run_id=$4
                   AND node_id=$5 AND result_sha256=$6)",
                    &[
                        &result_id,
                        &row.tenant_id,
                        &row.workspace_id,
                        &row.run_id,
                        &row.node_id,
                        &result_sha256,
                    ],
                )
                .map_err(|error| error.to_string())?
                .get(0);
            if !matches {
                return Err(
                    "provider embedding retrieval result cross-owner binding is invalid"
                        .to_string(),
                );
            }
        }
        (None, None, None) => {}
        _ => return Err("provider embedding result cross-owner binding is invalid".to_string()),
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
        || !matches!(
            row.operation_kind.as_str(),
            "memory_version" | "retrieval_query"
        )
        || row.target_version <= 0
        || row.tenant_id.is_empty()
        || row.workspace_id.is_empty()
        || row.source_id.is_empty()
        || !is_sha256(&row.source_sha256)
        || !is_sha256(&row.request_identity_sha256)
        || !is_sha256(&row.operation_binding_sha256)
        || !is_sha256(&row.content_sha256)
        || !is_sha256(&row.contract_sha256)
        || !is_sha256(&row.receipt_sha256)
        || row.reservation_event_id.is_empty()
        || row.operation_id != format!("embedding-operation-{}", row.operation_binding_sha256)
    {
        return Err(failure("operation identity or hash binding is invalid"));
    }
    let contract: EmbeddingContractEvidence = serde_json::from_str(&row.contract_json)
        .map_err(|_| failure("contract evidence JSON is malformed"))?;
    if sha256_bytes(row.contract_json.as_bytes()) != row.contract_sha256
        || contract.provider_id != row.provider_id
        || contract.requested_model_id != row.requested_model_id
        || contract.resolved_model_id != row.resolved_model_id
        || contract.dimensions as i64 != row.dimensions
        || !is_supported_durable_embedding_contract(&contract)
    {
        return Err(failure("contract evidence binding is invalid"));
    }
    let expected_receipt_sha256 = provider_embedding_operation_receipt_sha256(row)?;
    if row.receipt_sha256 != expected_receipt_sha256 {
        return Err(failure("operation receipt hash binding is invalid"));
    }
    let state = super::provider_audit::ProviderEmbeddingReceiptState::parse(&row.state)
        .map_err(|_| failure("operation state is invalid"))?;
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
        state,
        row.vector_json.as_deref(),
        row.metadata_json.as_deref(),
    ) {
        (
            super::provider_audit::ProviderEmbeddingReceiptState::NetworkSucceeded
            | super::provider_audit::ProviderEmbeddingReceiptState::Succeeded,
            Some(vector_json),
            Some(metadata_json),
        ) => {
            let values: Vec<f64> = serde_json::from_str(vector_json)
                .map_err(|_| failure("completed vector JSON is malformed"))?;
            let metadata: ProviderEmbeddingMetadata = serde_json::from_str(metadata_json)
                .map_err(|_| failure("completed metadata JSON is malformed"))?;
            validate_provider_embedding_metadata(&metadata, &values, &row.content_sha256)
                .map_err(|detail| failure(&detail))?;
            if metadata.provider_id != row.provider_id
                || metadata.requested_model_id != row.requested_model_id
                || metadata.resolved_model_id != row.resolved_model_id
                || metadata.dimensions as i64 != row.dimensions
            {
                return Err(failure(
                    "completed metadata provider/model does not match operation",
                ));
            }
        }
        (
            super::provider_audit::ProviderEmbeddingReceiptState::NetworkSucceeded
            | super::provider_audit::ProviderEmbeddingReceiptState::Succeeded,
            _,
            _,
        ) => {
            return Err(failure("successful operation receipt is incomplete"));
        }
        (_, None, None) => {}
        (_, _, _) => {
            return Err(failure(
                "non-successful operation must not retain vector or metadata",
            ));
        }
    }
    match state {
        super::provider_audit::ProviderEmbeddingReceiptState::PreflightReserved
        | super::provider_audit::ProviderEmbeddingReceiptState::Reserved => {
            if row.send_event_id.is_some() || row.outcome_event_id.is_some() {
                return Err(failure("reserved operation has send/outcome evidence"));
            }
        }
        super::provider_audit::ProviderEmbeddingReceiptState::Sending => {
            if row.send_event_id.is_none() || row.outcome_event_id.is_some() {
                return Err(failure("sending operation audit binding is incomplete"));
            }
        }
        super::provider_audit::ProviderEmbeddingReceiptState::NetworkSucceeded
        | super::provider_audit::ProviderEmbeddingReceiptState::FailedKnownOutcome
        | super::provider_audit::ProviderEmbeddingReceiptState::OutcomeUnknown => {
            if row.send_event_id.is_none() || row.outcome_event_id.is_none() {
                return Err(failure(
                    "completed network phase audit binding is incomplete",
                ));
            }
        }
        super::provider_audit::ProviderEmbeddingReceiptState::Succeeded => {
            if row.send_event_id.is_none()
                || row.outcome_event_id.is_none()
                || row.result_kind.is_none()
                || row.result_id.is_none()
                || row
                    .result_sha256
                    .as_deref()
                    .is_none_or(|value| !is_sha256(value))
            {
                return Err(failure("succeeded operation result binding is incomplete"));
            }
        }
        super::provider_audit::ProviderEmbeddingReceiptState::ResultErased => {
            if row.send_event_id.is_none()
                || row.outcome_event_id.is_none()
                || row.result_kind.is_some()
                || row.result_id.is_some()
                || row.result_sha256.is_some()
                || row.vector_json.is_some()
                || row.metadata_json.is_some()
            {
                return Err(failure("erased operation evidence binding is invalid"));
            }
        }
        super::provider_audit::ProviderEmbeddingReceiptState::FailedBeforeSend
            if row.send_event_id.is_some() || row.outcome_event_id.is_none() =>
        {
            return Err(failure("failed-before-send audit binding is invalid"));
        }
        _ => {}
    }
    Ok(())
}

fn provider_embedding_operation_receipt_sha256(
    row: &ProviderEmbeddingOperationIntegrityRow,
) -> Result<String, String> {
    sha256_json(&json!({
        "operation_id": row.operation_id,
        "operation_kind": row.operation_kind,
        "target_memory_id": row.target_memory_id,
        "target_version": row.target_version,
        "tenant_id":row.tenant_id,
        "workspace_id":row.workspace_id,
        "agent_id":row.agent_id,
        "run_id":row.run_id,
        "task_id":row.task_id,
        "source_id":row.source_id,
        "source_sha256":row.source_sha256,
        "node_id":row.node_id,
        "query_sha256":row.query_sha256,
        "request_identity_sha256":row.request_identity_sha256,
        "operation_binding_sha256": row.operation_binding_sha256,
        "content_sha256": row.content_sha256,
        "contract_sha256":row.contract_sha256,
        "provider_id": row.provider_id,
        "requested_model_id": row.requested_model_id,
        "resolved_model_id": row.resolved_model_id,
        "dimensions": row.dimensions,
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
                    request_cost_per_request_usd: 0.0,
                    image_cost_per_image_usd: 0.0,
                    request_max_price: crate::provider::embedding::EmbeddingPricingOverrides::zero(
                    ),
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
                request_cost_per_request_usd: 0.0,
                image_cost_per_image_usd: 0.0,
                request_max_price: crate::provider::embedding::EmbeddingPricingOverrides::zero(),
                currency: "USD".to_string(),
                effective_date: "2026-07-15".to_string(),
                source: OPENROUTER_EMBEDDING_PRICING_SOURCE.to_string(),
            },
        ))
        .unwrap();
        let contract_sha256 = sha256_bytes(contract_json.as_bytes());
        let receipt_sha256 = sha256_json(&json!({
            "operation_id": operation_id,
            "operation_kind": "memory_version",
            "target_memory_id": "memory-operation",
            "target_version": 1,
            "tenant_id":"tenant",
            "workspace_id":"workspace",
            "agent_id":null,
            "run_id":null,
            "task_id":null,
            "source_id":"source",
            "source_sha256":"d".repeat(64),
            "node_id":null,
            "query_sha256":null,
            "request_identity_sha256":binding_sha256,
            "operation_binding_sha256": binding_sha256,
            "content_sha256": content_sha256,
            "contract_sha256":contract_sha256,
            "provider_id": provider_id,
            "requested_model_id": model_id,
            "resolved_model_id": OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,
            "dimensions": OPENROUTER_EMBEDDING_DIMENSIONS as i64,
        }))
        .unwrap();
        let reservation_event_id = format!("fixture-reservation-{operation_id}");
        let send_event_id = format!("fixture-send-{operation_id}");
        let outcome_event_id = format!("fixture-outcome-{operation_id}");
        let has_send = state != "reserved";
        let has_outcome = matches!(
            state,
            "network_succeeded" | "succeeded" | "failed_known_outcome" | "outcome_unknown"
        );
        let has_result = state == "succeeded";
        store
            .with_conn(|connection| {
                if ignore_checks {
                    connection
                        .execute_batch("PRAGMA ignore_check_constraints = ON;")
                        .map_err(|error| error.to_string())?;
                }
                for (event_id, event_type) in [
                    (&reservation_event_id, "request_reserved"),
                    (&send_event_id, "request_sent"),
                    (&outcome_event_id, "response_received"),
                ] {
                    connection.execute(
                        "INSERT INTO provider_audit_events
                         (event_id,dispatch_id,provider_id,event_type,redaction_status,created_at)
                         VALUES (?1,?2,?3,?4,'redacted','2026-07-15T00:00:00Z')",
                        params![event_id, operation_id, provider_id, event_type],
                    ).map_err(|error| error.to_string())?;
                }
                connection
                    .execute(
                        "INSERT INTO provider_embedding_operations
                         (operation_id,operation_kind,target_memory_id,target_version,tenant_id,workspace_id,source_id,
                          source_sha256,request_identity_sha256,operation_binding_sha256,content_sha256,contract_json,
                          contract_sha256,receipt_sha256,provider_id,requested_model_id,resolved_model_id,dimensions,
                          reservation_event_id,send_event_id,outcome_event_id,result_kind,result_id,result_sha256,
                          state,attempt_count,vector_json,metadata_json,created_at,updated_at)
                         VALUES (?1,'memory_version','memory-operation',1,'tenant','workspace','source',?2,?3,?3,?4,?5,
                                 ?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,1,?19,?20,
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
                            OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,
                            OPENROUTER_EMBEDDING_DIMENSIONS as i64,
                            reservation_event_id,
                            has_send.then_some(send_event_id),
                            has_outcome.then_some(outcome_event_id),
                            has_result.then_some("memory_version"),
                            has_result.then_some("memory-operation:1"),
                            has_result.then_some("b".repeat(64)),
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
        let error = store.check_integrity().unwrap_err();
        assert!(
            error.contains("provider embedding dimension mismatch")
                || error.contains("provider embedding identity is invalid"),
            "unexpected dimension corruption error: {error}"
        );
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
            "network_succeeded",
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
                "failed_known_outcome",
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
            "failed_known_outcome",
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
                "failed_known_outcome",
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
                "network_succeeded",
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
            ("network_succeeded", None, None, "receipt is incomplete"),
            (
                "network_succeeded",
                Some(vector_json.as_str()),
                None,
                "receipt is incomplete",
            ),
            (
                "network_succeeded",
                None,
                Some(metadata_json.as_str()),
                "receipt is incomplete",
            ),
            (
                "sending",
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
                "network_succeeded",
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
                "network_succeeded",
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
