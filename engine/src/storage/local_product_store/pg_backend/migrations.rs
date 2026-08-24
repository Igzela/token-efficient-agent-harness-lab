use super::super::{schema, LocalProductStore};
use serde_json::Value;

#[cfg(test)]
pub(super) const CURRENT_PG_VERSION: i64 = schema::CURRENT_POSTGRES_SCHEMA_VERSION;

fn ensure_schema_migrations_table(client: &mut postgres::Client) -> Result<(), String> {
    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version BIGINT PRIMARY KEY,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );",
        )
        .map_err(|e| format!("failed to create schema_migrations: {e}"))
}

fn current_pg_version(client: &mut postgres::Client) -> Result<i64, String> {
    let row = client
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map_err(|e| format!("failed to read current pg version: {e}"))?;
    Ok(row.get::<_, i64>(0))
}

fn apply_pg_migration(
    client: &mut postgres::Client,
    version: i64,
    sql: &str,
) -> Result<(), String> {
    client
        .batch_execute(sql)
        .map_err(|e| format!("migration {version} failed: {e}"))?;
    client
        .execute(
            "INSERT INTO schema_migrations (version) VALUES ($1)",
            &[&version],
        )
        .map_err(|e| format!("failed to record migration {version}: {e}"))?;
    Ok(())
}

fn pg_column_exists(
    client: &mut impl postgres::GenericClient,
    table: &str,
    column: &str,
) -> Result<bool, String> {
    let row = client
        .query_one(
            "SELECT EXISTS(
                SELECT 1 FROM information_schema.columns
                WHERE table_schema = current_schema()
                  AND table_name = $1 AND column_name = $2
            )",
            &[&table, &column],
        )
        .map_err(|e| format!("information_schema query failed: {e}"))?;
    Ok(row.get::<_, bool>(0))
}

fn apply_pg_v25_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V25_SCHEMA_VERSION;
    let mut tx = client
        .transaction()
        .map_err(|error| format!("failed to start migration 25 transaction: {error}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
             hashtext(current_database()), hashtext(current_schema())
         )",
        &[],
    )
    .map_err(|error| format!("failed to lock migration 25: {error}"))?;

    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|row| row.get::<_, i64>(0))
        .map_err(|error| format!("failed to re-read version for migration 25: {error}"))?;
    let marker_exists = tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = $1)",
            &[&version],
        )
        .map(|row| row.get::<_, bool>(0))
        .map_err(|error| format!("failed to read migration 25 marker: {error}"))?;
    if current_version < 24 {
        return Err(format!(
            "migration 25 requires a contiguous version 24 predecessor; found {current_version}"
        ));
    }

    let metadata = pg_column_exists(
        &mut tx,
        "durable_memory_versions",
        "embedding_metadata_json",
    )?;
    let binding = pg_column_exists(
        &mut tx,
        "durable_memory_versions",
        "embedding_binding_sha256",
    )?;
    let sql = match (metadata, binding) {
        (true, true) => "",
        (false, false) => {
            "ALTER TABLE durable_memory_versions ADD COLUMN embedding_metadata_json TEXT;
             ALTER TABLE durable_memory_versions ADD COLUMN embedding_binding_sha256 TEXT;"
        }
        (false, true) => {
            "ALTER TABLE durable_memory_versions ADD COLUMN embedding_metadata_json TEXT;"
        }
        (true, false) => {
            "ALTER TABLE durable_memory_versions ADD COLUMN embedding_binding_sha256 TEXT;"
        }
    };
    if !sql.is_empty() {
        tx.batch_execute(sql)
            .map_err(|error| format!("migration 25 failed: {error}"))?;
    }
    let operation_ddl = "CREATE TABLE IF NOT EXISTS provider_embedding_operations (
            operation_id TEXT PRIMARY KEY,
            operation_kind TEXT NOT NULL CHECK (operation_kind IN ('memory_version','retrieval_query')),
            target_memory_id TEXT NOT NULL,
            target_version BIGINT NOT NULL,
            tenant_id TEXT NOT NULL,
            workspace_id TEXT NOT NULL,
            agent_id TEXT,
            run_id TEXT,
            task_id TEXT,
            source_id TEXT NOT NULL,
            source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
            node_id TEXT,
            query_sha256 TEXT CHECK (query_sha256 IS NULL OR length(query_sha256) = 64),
            request_identity_sha256 TEXT NOT NULL CHECK (length(request_identity_sha256) = 64),
            operation_binding_sha256 TEXT NOT NULL CHECK (length(operation_binding_sha256) = 64),
            content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
            contract_json TEXT NOT NULL,
            contract_sha256 TEXT NOT NULL CHECK (length(contract_sha256) = 64),
            receipt_sha256 TEXT NOT NULL CHECK (length(receipt_sha256) = 64),
            provider_id TEXT NOT NULL,
            requested_model_id TEXT NOT NULL,
            resolved_model_id TEXT NOT NULL,
            dimensions BIGINT NOT NULL CHECK (dimensions > 0),
            reservation_event_id TEXT NOT NULL,
            send_event_id TEXT,
            outcome_event_id TEXT,
            result_kind TEXT CHECK (result_kind IS NULL OR result_kind IN ('memory_version','retrieval_event')),
            result_id TEXT,
            result_sha256 TEXT CHECK (result_sha256 IS NULL OR length(result_sha256) = 64),
            state TEXT NOT NULL CHECK (state IN ('preflight_reserved','reserved','sending','network_succeeded','succeeded','result_erased','failed_before_send','failed_known_outcome','outcome_unknown','outcome_unknown_acknowledged','retry_authorized')),
            attempt_count BIGINT NOT NULL DEFAULT 1 CHECK (attempt_count BETWEEN 1 AND 4),
            vector_json TEXT,
            metadata_json TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE (target_memory_id, target_version),
            FOREIGN KEY (reservation_event_id) REFERENCES provider_audit_events(event_id),
            FOREIGN KEY (send_event_id) REFERENCES provider_audit_events(event_id),
            FOREIGN KEY (outcome_event_id) REFERENCES provider_audit_events(event_id),
            CHECK ((result_kind IS NULL AND result_id IS NULL AND result_sha256 IS NULL)
                OR (result_kind IS NOT NULL AND result_id IS NOT NULL AND result_sha256 IS NOT NULL)),
            CHECK ((operation_kind='memory_version' AND node_id IS NULL AND query_sha256 IS NULL)
                OR (operation_kind='retrieval_query' AND run_id IS NOT NULL AND node_id IS NOT NULL
                    AND query_sha256 IS NOT NULL AND query_sha256=source_sha256))
        );";
    tx.batch_execute(operation_ddl)
        .map_err(|error| format!("migration 25 operation receipt failed: {error}"))?;
    let required_columns = [
        "operation_id",
        "operation_kind",
        "target_memory_id",
        "target_version",
        "tenant_id",
        "workspace_id",
        "agent_id",
        "run_id",
        "task_id",
        "source_id",
        "source_sha256",
        "node_id",
        "query_sha256",
        "request_identity_sha256",
        "operation_binding_sha256",
        "content_sha256",
        "contract_json",
        "contract_sha256",
        "receipt_sha256",
        "provider_id",
        "requested_model_id",
        "resolved_model_id",
        "dimensions",
        "reservation_event_id",
        "send_event_id",
        "outcome_event_id",
        "result_kind",
        "result_id",
        "result_sha256",
        "state",
        "attempt_count",
        "vector_json",
        "metadata_json",
        "created_at",
        "updated_at",
    ];
    let mut missing_columns = Vec::new();
    for column in required_columns {
        if !pg_column_exists(&mut tx, "provider_embedding_operations", column)? {
            missing_columns.push(column);
        }
    }
    let invalid_schema = !missing_columns.is_empty() || !pg_v25_operation_schema_valid(&mut tx)?;
    if current_version > version && !marker_exists {
        if invalid_schema && pg_table_present(&mut tx, "provider_embedding_operations")? {
            let occupied: bool = tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM provider_embedding_operations LIMIT 1)",
                    &[],
                )
                .map(|row| row.get(0))
                .map_err(|error| {
                    format!("failed to inspect partial migration 25 receipts: {error}")
                })?;
            if occupied {
                return Err(format!(
                    "migration 25 cannot repair an occupied partial operation table; missing or invalid {}",
                    if missing_columns.is_empty() {
                        "constraints/indexes/foreign-keys".to_string()
                    } else {
                        missing_columns.join(",")
                    }
                ));
            }
        }
        return Err(format!(
            "migration 25 requires a contiguous version 24 predecessor; found {current_version}"
        ));
    }
    if invalid_schema {
        let occupied: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM provider_embedding_operations LIMIT 1)",
                &[],
            )
            .map(|row| row.get(0))
            .map_err(|error| format!("failed to inspect partial migration 25 receipts: {error}"))?;
        if occupied {
            return Err(format!(
                "migration 25 cannot repair an occupied partial operation table; missing or invalid {}",
                if missing_columns.is_empty() { "constraints/indexes/foreign-keys".to_string() } else { missing_columns.join(",") }
            ));
        }
        tx.batch_execute("DROP TABLE provider_embedding_operations;")
            .map_err(|error| format!("migration 25 partial table cleanup failed: {error}"))?;
        tx.batch_execute(operation_ddl)
            .map_err(|error| format!("migration 25 partial table repair failed: {error}"))?;
    }
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|error| format!("failed to record migration 25: {error}"))?;

    if !pg_column_exists(
        &mut tx,
        "durable_memory_versions",
        "embedding_metadata_json",
    )? || !pg_column_exists(
        &mut tx,
        "durable_memory_versions",
        "embedding_binding_sha256",
    )? {
        return Err("migration 25 column verification failed".to_string());
    }
    for column in required_columns {
        if !pg_column_exists(&mut tx, "provider_embedding_operations", column)? {
            return Err(format!(
                "migration 25 operation table verification failed: missing {column}"
            ));
        }
    }
    tx.batch_execute(
        "CREATE INDEX IF NOT EXISTS idx_provider_embedding_operations_state
         ON provider_embedding_operations(state, updated_at);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_embedding_operations_retrieval_identity
         ON provider_embedding_operations(tenant_id,workspace_id,run_id,node_id,query_sha256,provider_id,
             requested_model_id,resolved_model_id,dimensions,request_identity_sha256)
         WHERE operation_kind='retrieval_query';",
    )
    .map_err(|error| format!("migration 25 operation index failed: {error}"))?;
    if !pg_v25_operation_schema_valid(&mut tx)? {
        return Err("migration 25 operation schema verification failed".to_string());
    }
    let marker: bool = tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=$1)",
            &[&version],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    if !marker {
        return Err("migration 25 schema version verification failed".to_string());
    }
    tx.commit()
        .map_err(|error| format!("failed to commit migration 25: {error}"))
}

fn pg_table_present(
    client: &mut impl postgres::GenericClient,
    table: &str,
) -> Result<bool, String> {
    client
        .query_one(
            "SELECT EXISTS(
                SELECT 1 FROM information_schema.tables
                WHERE table_schema = current_schema() AND table_name = $1
            )",
            &[&table],
        )
        .map(|row| row.get(0))
        .map_err(|error| error.to_string())
}

fn apply_pg_v26_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V26_SCHEMA_VERSION;
    let mut tx = client
        .transaction()
        .map_err(|error| format!("failed to start migration 26 transaction: {error}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
             hashtext(current_database()), hashtext(current_schema())
         )",
        &[],
    )
    .map_err(|error| format!("failed to lock migration 26: {error}"))?;
    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|row| row.get::<_, i64>(0))
        .map_err(|error| format!("failed to re-read version for migration 26: {error}"))?;
    if current_version >= version {
        tx.commit()
            .map_err(|error| format!("failed to finish migration 26 no-op: {error}"))?;
        return Ok(());
    }
    tx.batch_execute(schema::V26_DDL)
        .map_err(|error| format!("migration 26 failed: {error}"))?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|error| format!("failed to record migration {version}: {error}"))?;
    tx.commit()
        .map_err(|error| format!("failed to commit migration 26: {error}"))
}

fn apply_pg_v27_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V27_SCHEMA_VERSION;
    let mut tx = client
        .transaction()
        .map_err(|error| format!("failed to start migration 27 transaction: {error}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
             hashtext(current_database()), hashtext(current_schema())
         )",
        &[],
    )
    .map_err(|error| format!("failed to lock migration 27: {error}"))?;
    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|row| row.get::<_, i64>(0))
        .map_err(|error| format!("failed to re-read version for migration 27: {error}"))?;
    if current_version >= version {
        tx.commit()
            .map_err(|error| format!("failed to finish migration 27 no-op: {error}"))?;
        return Ok(());
    }
    tx.batch_execute(schema::V27_DDL)
        .map_err(|error| format!("migration 27 failed: {error}"))?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|error| format!("failed to record migration {version}: {error}"))?;
    tx.commit()
        .map_err(|error| format!("failed to commit migration 27: {error}"))
}

fn apply_pg_v29_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V29_SCHEMA_VERSION;
    let mut tx = client
        .transaction()
        .map_err(|error| format!("failed to start migration 29 transaction: {error}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
             hashtext(current_database()), hashtext(current_schema())
         )",
        &[],
    )
    .map_err(|error| format!("failed to lock migration 29: {error}"))?;
    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|row| row.get::<_, i64>(0))
        .map_err(|error| format!("failed to re-read version for migration 29: {error}"))?;
    if current_version >= version {
        tx.commit()
            .map_err(|error| format!("failed to finish migration 29 no-op: {error}"))?;
        return Ok(());
    }
    tx.batch_execute(schema::V29_DDL)
        .map_err(|error| format!("migration 29 failed: {error}"))?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|error| format!("failed to record migration {version}: {error}"))?;
    tx.commit()
        .map_err(|error| format!("failed to commit migration 29: {error}"))
}

fn apply_pg_v30_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V30_SCHEMA_VERSION;
    let mut tx = client
        .transaction()
        .map_err(|error| format!("failed to start migration 30 transaction: {error}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
             hashtext(current_database()), hashtext(current_schema())
         )",
        &[],
    )
    .map_err(|error| format!("failed to lock migration 30: {error}"))?;
    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|row| row.get::<_, i64>(0))
        .map_err(|error| format!("failed to re-read version for migration 30: {error}"))?;
    if current_version >= version {
        tx.commit()
            .map_err(|error| format!("failed to finish migration 30 no-op: {error}"))?;
        return Ok(());
    }
    tx.batch_execute(schema::V30_DDL)
        .map_err(|error| format!("migration 30 failed: {error}"))?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|error| format!("failed to record migration {version}: {error}"))?;
    tx.commit()
        .map_err(|error| format!("failed to commit migration 30: {error}"))
}

fn apply_pg_v31_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V31_SCHEMA_VERSION;
    let mut tx = client
        .transaction()
        .map_err(|error| format!("failed to start migration 31 transaction: {error}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
             hashtext(current_database()), hashtext(current_schema())
         )",
        &[],
    )
    .map_err(|error| format!("failed to lock migration 31: {error}"))?;
    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|row| row.get::<_, i64>(0))
        .map_err(|error| format!("failed to re-read version for migration 31: {error}"))?;
    if current_version >= version {
        tx.commit()
            .map_err(|error| format!("failed to finish migration 31 no-op: {error}"))?;
        return Ok(());
    }
    tx.batch_execute(schema::V31_DDL)
        .map_err(|error| format!("migration 31 failed: {error}"))?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|error| format!("failed to record migration {version}: {error}"))?;
    tx.commit()
        .map_err(|error| format!("failed to commit migration 31: {error}"))
}

fn apply_pg_v32_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V32_SCHEMA_VERSION;
    let mut tx = client
        .transaction()
        .map_err(|error| format!("failed to start migration 32 transaction: {error}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
             hashtext(current_database()), hashtext(current_schema())
         )",
        &[],
    )
    .map_err(|error| format!("failed to lock migration 32: {error}"))?;
    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|row| row.get::<_, i64>(0))
        .map_err(|error| format!("failed to re-read version for migration 32: {error}"))?;
    if current_version >= version {
        tx.commit()
            .map_err(|error| format!("failed to finish migration 32 no-op: {error}"))?;
        return Ok(());
    }
    tx.batch_execute(schema::V32_DDL)
        .map_err(|error| format!("migration 32 failed: {error}"))?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|error| format!("failed to record migration {version}: {error}"))?;
    tx.commit()
        .map_err(|error| format!("failed to commit migration 32: {error}"))
}

fn apply_pg_v34_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V34_SCHEMA_VERSION;
    let mut tx = client.transaction().map_err(|e| format!("m34 tx: {e}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtext(current_database()), hashtext(current_schema()))",
        &[],
    )
    .map_err(|e| e.to_string())?;
    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?;
    if current_version >= version {
        tx.commit().map_err(|e| e.to_string())?;
        return Ok(());
    }
    tx.batch_execute(schema::V34_DDL)
        .map_err(|e| format!("m34: {e}"))?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

fn apply_pg_v35_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V35_SCHEMA_VERSION;
    let mut tx = client.transaction().map_err(|e| format!("m35 tx: {e}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtext(current_database()), hashtext(current_schema()))",
        &[],
    )
    .map_err(|e| e.to_string())?;
    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?;
    if current_version >= version {
        tx.commit().map_err(|e| e.to_string())?;
        return Ok(());
    }
    tx.batch_execute(schema::V35_DDL)
        .map_err(|e| format!("m35: {e}"))?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

fn apply_pg_v36_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V36_SCHEMA_VERSION;
    let mut tx = client.transaction().map_err(|e| format!("m36 tx: {e}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtext(current_database()), hashtext(current_schema()))",
        &[],
    )
    .map_err(|e| e.to_string())?;
    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?;
    if current_version >= version {
        tx.batch_execute(schema::EC1_IDENTITY_LINEAGE_DDL)
            .map_err(|e| format!("m36 ec1 identity lineage repair: {e}"))?;
        tx.batch_execute(schema::EC1_CAUSAL_MANIFEST_DDL)
            .map_err(|e| format!("m36 ec1 causal manifest repair: {e}"))?;
        tx.batch_execute(schema::EC1_CANDIDATE_BINDING_DDL)
            .map_err(|e| format!("m36 ec1 candidate binding repair: {e}"))?;
        tx.batch_execute(schema::EC2_HOLDOUT_SEAL_DDL)
            .map_err(|e| format!("m36 ec2 holdout seal repair: {e}"))?;
        tx.batch_execute(schema::EC2_PREDICTION_OUTCOME_DDL)
            .map_err(|e| format!("m36 ec2 prediction outcome repair: {e}"))?;
        tx.batch_execute(schema::EC3_LIFECYCLE_COST_DDL)
            .map_err(|e| format!("m36 ec3 lifecycle cost repair: {e}"))?;
        tx.batch_execute(schema::EC3_LIFECYCLE_BUDGET_DDL)
            .map_err(|e| format!("m36 ec3 lifecycle budget repair: {e}"))?;
        tx.batch_execute(schema::EC3_LIFECYCLE_RECONCILIATION_DDL)
            .map_err(|e| format!("m36 ec3 lifecycle reconciliation repair: {e}"))?;
        repair_pg_v36_delegated_plan_owner(&mut tx)?;
        validate_pg_v36_structure(&mut tx)?;
        tx.commit().map_err(|e| e.to_string())?;
        return Ok(());
    }
    tx.batch_execute(schema::V36_DDL)
        .map_err(|e| format!("m36: {e}"))?;
    tx.batch_execute(schema::EC1_IDENTITY_LINEAGE_DDL)
        .map_err(|e| format!("m36 ec1 identity lineage: {e}"))?;
    tx.batch_execute(schema::EC1_CAUSAL_MANIFEST_DDL)
        .map_err(|e| format!("m36 ec1 causal manifest: {e}"))?;
    tx.batch_execute(schema::EC1_CANDIDATE_BINDING_DDL)
        .map_err(|e| format!("m36 ec1 candidate binding: {e}"))?;
    tx.batch_execute(schema::EC2_HOLDOUT_SEAL_DDL)
        .map_err(|e| format!("m36 ec2 holdout seal: {e}"))?;
    tx.batch_execute(schema::EC2_PREDICTION_OUTCOME_DDL)
        .map_err(|e| format!("m36 ec2 prediction outcome: {e}"))?;
    tx.batch_execute(schema::EC3_LIFECYCLE_COST_DDL)
        .map_err(|e| format!("m36 ec3 lifecycle cost: {e}"))?;
    tx.batch_execute(schema::EC3_LIFECYCLE_BUDGET_DDL)
        .map_err(|e| format!("m36 ec3 lifecycle budget: {e}"))?;
    tx.batch_execute(schema::EC3_LIFECYCLE_RECONCILIATION_DDL)
        .map_err(|e| format!("m36 ec3 lifecycle reconciliation: {e}"))?;
    repair_pg_v36_delegated_plan_owner(&mut tx)?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

fn apply_pg_v37_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V37_SCHEMA_VERSION;
    let mut tx = client.transaction().map_err(|e| format!("m37 tx: {e}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtext(current_database()), hashtext(current_schema()))",
        &[],
    )
    .map_err(|e| e.to_string())?;
    tx.batch_execute(schema::EC2_PREDICTION_OUTCOME_DDL)
        .map_err(|e| format!("m37 prediction outcome: {e}"))?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

fn apply_pg_v38_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V38_SCHEMA_VERSION;
    let mut tx = client.transaction().map_err(|e| format!("v38 tx: {e}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtext(current_database()), hashtext(current_schema()))",
        &[],
    )
    .map_err(|e| e.to_string())?;
    tx.batch_execute(schema::EC3_LIFECYCLE_COST_OBSERVATION_DDL)
        .map_err(|e| format!("v38 lifecycle cost observations: {e}"))?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

fn validate_pg_v37_schema(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    let version = client
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map_err(|e| e.to_string())?
        .get::<_, i64>(0);
    if version != super::super::migrations::V37_SCHEMA_VERSION {
        return Err(format!("PostgreSQL v37 schema version mismatch: {version}"));
    }
    validate_pg_v37_structure(client)
}

fn validate_pg_v37_structure(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    for table in super::super::migrations::V37_TABLES {
        if !pg_table_present(client, table)? {
            return Err(format!("PostgreSQL v37 schema missing table {table}"));
        }
    }
    for index in super::super::migrations::V37_INDEXES {
        let present = client
            .query_one(
                "SELECT EXISTS (
                    SELECT 1 FROM pg_indexes
                    WHERE schemaname=current_schema() AND indexname=$1
                 )",
                &[&index],
            )
            .map_err(|e| e.to_string())?
            .get::<_, bool>(0);
        if !present {
            return Err(format!("PostgreSQL v37 schema missing index {index}"));
        }
    }
    validate_pg_v36_structure(client)
}

fn validate_pg_v38_schema(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    let version = client
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map_err(|e| e.to_string())?
        .get::<_, i64>(0);
    if version != super::super::migrations::V38_SCHEMA_VERSION {
        return Err(format!("PostgreSQL v38 schema version mismatch: {version}"));
    }
    validate_pg_v37_structure(client)?;
    for table in super::super::migrations::V38_TABLES {
        if !pg_table_present(client, table)? {
            return Err(format!("PostgreSQL v38 schema missing table {table}"));
        }
    }
    for index in super::super::migrations::V38_INDEXES {
        let present = client
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE schemaname=current_schema() AND indexname=$1)",
                &[&index],
            )
            .map_err(|e| e.to_string())?
            .get::<_, bool>(0);
        if !present {
            return Err(format!("PostgreSQL v38 schema missing index {index}"));
        }
    }
    Ok(())
}

fn repair_pg_v36_delegated_plan_owner(
    client: &mut impl postgres::GenericClient,
) -> Result<(), String> {
    client
        .batch_execute(
            "ALTER TABLE managed_acceptance_delegations
                 ADD COLUMN IF NOT EXISTS product_task_id TEXT;
             ALTER TABLE api_key_metadata
                 ADD COLUMN IF NOT EXISTS tenant_id TEXT;
             ALTER TABLE workflow_plans
                 ADD COLUMN IF NOT EXISTS delegated_plan_owner_id TEXT;
             UPDATE workflow_plans
             SET delegated_plan_owner_id =
                 CAST(plan_json AS jsonb) #>> '{advisory,delegated_plan_owner_id}'
             WHERE request_source='product_golden_path_delegated'
               AND delegated_plan_owner_id IS NULL;",
        )
        .map_err(|error| format!("v36 delegated plan owner repair failed: {error}"))?;
    let duplicate = client
        .query_opt(
            "SELECT delegated_plan_owner_id
             FROM workflow_plans
             WHERE delegated_plan_owner_id IS NOT NULL
             GROUP BY delegated_plan_owner_id
             HAVING COUNT(*) > 1
             LIMIT 1",
            &[],
        )
        .map_err(|error| error.to_string())?;
    if duplicate.is_some() {
        return Err("v36 delegated plan owner repair found multiple plans for one owner".into());
    }
    client
        .batch_execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_workflow_plans_delegated_owner
             ON workflow_plans(delegated_plan_owner_id)
             WHERE delegated_plan_owner_id IS NOT NULL;",
        )
        .map_err(|error| format!("v36 delegated plan owner index repair failed: {error}"))
}

fn validate_pg_v34_tables(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    for table in super::super::migrations::V34_TABLES {
        if !pg_table_present(client, table)? {
            return Err(format!("PostgreSQL v34 schema missing table {table}"));
        }
    }
    validate_pg_v33_schema(client)
}

fn validate_pg_v35_schema(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    let version = client
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map_err(|e| e.to_string())?
        .get::<_, i64>(0);
    if version != super::super::migrations::V35_SCHEMA_VERSION {
        return Err(format!("PostgreSQL v35 schema version mismatch: {version}"));
    }
    validate_pg_v35_structure(client)
}

fn validate_pg_v35_structure(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    for table in super::super::migrations::V35_TABLES {
        if !pg_table_present(client, table)? {
            return Err(format!("PostgreSQL v35 schema missing table {table}"));
        }
    }
    validate_pg_v34_tables(client)
}

#[cfg(test)]
fn validate_pg_v36_schema(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    let version = client
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map_err(|e| e.to_string())?
        .get::<_, i64>(0);
    if version != super::super::migrations::V36_SCHEMA_VERSION {
        return Err(format!("PostgreSQL v36 schema version mismatch: {version}"));
    }
    validate_pg_v36_structure(client)
}

fn validate_pg_v36_structure(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    for table in super::super::migrations::V36_TABLES {
        if !pg_table_present(client, table)? {
            return Err(format!("PostgreSQL v36 schema missing table {table}"));
        }
    }
    for column in super::super::migrations::V36_COLUMNS {
        let present = client
            .query_one(
                "SELECT EXISTS (
                    SELECT 1 FROM information_schema.columns
                    WHERE table_schema=current_schema()
                      AND table_name='managed_acceptance_delegations'
                      AND column_name=$1
                 )",
                &[&column],
            )
            .map_err(|error| error.to_string())?
            .get::<_, bool>(0);
        if !present {
            return Err(format!("PostgreSQL v36 schema missing column {column}"));
        }
    }
    for index in super::super::migrations::V36_INDEXES {
        let present = client
            .query_one(
                "SELECT EXISTS (
                    SELECT 1 FROM pg_indexes
                    WHERE schemaname=current_schema()
                      AND tablename='managed_acceptance_delegations'
                      AND indexname=$1
                 )",
                &[&index],
            )
            .map_err(|error| error.to_string())?
            .get::<_, bool>(0);
        if !present {
            return Err(format!("PostgreSQL v36 schema missing index {index}"));
        }
    }
    if !pg_column_exists(
        client,
        "workflow_plans",
        super::super::migrations::V36_DELEGATED_PLAN_OWNER_COLUMN,
    )? {
        return Err(format!(
            "PostgreSQL v36 schema missing workflow_plans column {}",
            super::super::migrations::V36_DELEGATED_PLAN_OWNER_COLUMN
        ));
    }
    if !pg_column_exists(
        client,
        "api_key_metadata",
        super::super::migrations::V36_API_KEY_TENANT_COLUMN,
    )? {
        return Err(format!(
            "PostgreSQL v36 schema missing api_key_metadata column {}",
            super::super::migrations::V36_API_KEY_TENANT_COLUMN
        ));
    }
    let owner_index_present = client
        .query_one(
            "SELECT EXISTS (
                SELECT 1 FROM pg_indexes
                WHERE schemaname=current_schema() AND indexname=$1
             )",
            &[&super::super::migrations::V36_DELEGATED_PLAN_OWNER_INDEX],
        )
        .map_err(|error| error.to_string())?
        .get::<_, bool>(0);
    if !owner_index_present {
        return Err(format!(
            "PostgreSQL v36 schema missing index {}",
            super::super::migrations::V36_DELEGATED_PLAN_OWNER_INDEX
        ));
    }
    validate_pg_v35_structure(client)
}

fn apply_pg_v33_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V33_SCHEMA_VERSION;
    let mut tx = client
        .transaction()
        .map_err(|error| format!("failed to start migration 33 transaction: {error}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
             hashtext(current_database()), hashtext(current_schema())
         )",
        &[],
    )
    .map_err(|error| format!("failed to lock migration 33: {error}"))?;
    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|row| row.get::<_, i64>(0))
        .map_err(|error| format!("failed to re-read version for migration 33: {error}"))?;
    if current_version >= version {
        repair_pg_v32_transition_schema(&mut tx)?;
        repair_pg_v33_spend_schema(&mut tx)?;
        tx.commit()
            .map_err(|error| format!("failed to finish migration 33 repair: {error}"))?;
        return Ok(());
    }
    if !pg_table_present(&mut tx, "managed_acceptance_spend_authorizations")? {
        tx.batch_execute(schema::V33_DDL)
            .map_err(|error| format!("migration 33 failed: {error}"))?;
    }
    repair_pg_v32_transition_schema(&mut tx)?;
    for (col, decl) in [
        ("spend_authorization_id", "TEXT"),
        ("lease_token", "TEXT"),
        ("receipt_sha256", "TEXT"),
    ] {
        if !pg_column_exists(&mut tx, "managed_acceptance_attempts", col)? {
            tx.batch_execute(&format!(
                "ALTER TABLE managed_acceptance_attempts ADD COLUMN {col} {decl};"
            ))
            .map_err(|error| format!("migration 33 column {col}: {error}"))?;
        }
    }
    if !pg_column_exists(
        &mut tx,
        "managed_acceptance_spend_authorizations",
        "logical_authorization_sha256",
    )? {
        tx.batch_execute(
            "ALTER TABLE managed_acceptance_spend_authorizations
             ADD COLUMN logical_authorization_sha256 TEXT;",
        )
        .map_err(|error| format!("migration 33 spend logical identity column: {error}"))?;
    }
    repair_pg_v33_spend_schema(&mut tx)?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|error| format!("failed to record migration {version}: {error}"))?;
    tx.commit()
        .map_err(|error| format!("failed to commit migration 33: {error}"))
}

fn repair_pg_v32_transition_schema(tx: &mut postgres::Transaction<'_>) -> Result<(), String> {
    const TABLE: &str = "managed_acceptance_decision_transition_receipts";
    if !pg_table_present(tx, TABLE)? {
        return Ok(());
    }
    if !pg_column_exists(tx, TABLE, "sequence")? {
        tx.batch_execute(
            "ALTER TABLE managed_acceptance_decision_transition_receipts
             ADD COLUMN sequence BIGINT",
        )
        .map_err(|error| error.to_string())?;
    }
    if !pg_column_exists(tx, TABLE, "previous_transition_sequence")? {
        tx.batch_execute(
            "ALTER TABLE managed_acceptance_decision_transition_receipts
             ADD COLUMN previous_transition_sequence BIGINT",
        )
        .map_err(|error| error.to_string())?;
    }

    let invalid_genesis_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM (
                 SELECT decision_id
                 FROM managed_acceptance_decision_transition_receipts
                 GROUP BY decision_id
                 HAVING COUNT(*) FILTER (WHERE previous_transition_sha256 IS NULL) <> 1
             ) invalid",
            &[],
        )
        .map(|row| row.get(0))
        .map_err(|error| format!("v32 transition genesis scan failed: {error}"))?;
    if invalid_genesis_count != 0 {
        return Err(format!(
            "v32 transition repair found {invalid_genesis_count} decision chain(s) without exactly one genesis"
        ));
    }

    let fork_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM (
                 SELECT decision_id, previous_transition_sha256
                 FROM managed_acceptance_decision_transition_receipts
                 WHERE previous_transition_sha256 IS NOT NULL
                 GROUP BY decision_id, previous_transition_sha256
                 HAVING COUNT(*) > 1
             ) invalid",
            &[],
        )
        .map(|row| row.get(0))
        .map_err(|error| format!("v32 transition fork scan failed: {error}"))?;
    if fork_count != 0 {
        return Err(format!(
            "v32 transition repair found {fork_count} forked predecessor hash(es)"
        ));
    }

    let total_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM managed_acceptance_decision_transition_receipts",
            &[],
        )
        .map(|row| row.get(0))
        .map_err(|error| format!("v32 transition row count failed: {error}"))?;
    let chained_count: i64 = tx
        .query_one(
            "WITH RECURSIVE chain AS (
                 SELECT transition_receipt_id, decision_id, transition_sha256
                 FROM managed_acceptance_decision_transition_receipts
                 WHERE previous_transition_sha256 IS NULL
                 UNION ALL
                 SELECT child.transition_receipt_id, child.decision_id, child.transition_sha256
                 FROM managed_acceptance_decision_transition_receipts child
                 JOIN chain parent
                   ON child.decision_id = parent.decision_id
                  AND child.previous_transition_sha256 = parent.transition_sha256
             )
             SELECT COUNT(*) FROM chain",
            &[],
        )
        .map(|row| row.get(0))
        .map_err(|error| format!("v32 transition hash-chain traversal failed: {error}"))?;
    if chained_count != total_count {
        return Err(format!(
            "v32 transition repair found incomplete, orphaned, or cyclic hash chains: chained {chained_count} of {total_count} receipts"
        ));
    }

    tx.batch_execute(
        "DROP INDEX IF EXISTS idx_managed_acceptance_transition_sequence;
         ALTER TABLE managed_acceptance_decision_transition_receipts
           DROP CONSTRAINT IF EXISTS managed_acceptance_decision_transition_receipts_decision_id_sequence_key;
         WITH RECURSIVE chain AS (
             SELECT transition_receipt_id, decision_id, transition_sha256,
                    1::BIGINT AS sequence,
                    NULL::BIGINT AS previous_transition_sequence
             FROM managed_acceptance_decision_transition_receipts
             WHERE previous_transition_sha256 IS NULL
             UNION ALL
             SELECT child.transition_receipt_id, child.decision_id, child.transition_sha256,
                    parent.sequence + 1,
                    parent.sequence
             FROM managed_acceptance_decision_transition_receipts child
             JOIN chain parent
               ON child.decision_id = parent.decision_id
              AND child.previous_transition_sha256 = parent.transition_sha256
         )
         UPDATE managed_acceptance_decision_transition_receipts receipt
         SET sequence = chain.sequence,
             previous_transition_sequence = chain.previous_transition_sequence
         FROM chain
         WHERE receipt.transition_receipt_id = chain.transition_receipt_id
           AND (
               receipt.sequence IS DISTINCT FROM chain.sequence OR
               receipt.previous_transition_sequence IS DISTINCT FROM chain.previous_transition_sequence
           );
         ALTER TABLE managed_acceptance_decision_transition_receipts
           ALTER COLUMN sequence SET NOT NULL;
         CREATE UNIQUE INDEX idx_managed_acceptance_transition_sequence
           ON managed_acceptance_decision_transition_receipts(decision_id, sequence);",
    )
    .map_err(|error| format!("v32 transition hash-chain repair failed: {error}"))
}

fn repair_pg_v33_spend_schema(tx: &mut postgres::Transaction<'_>) -> Result<(), String> {
    if !pg_column_exists(
        tx,
        "managed_acceptance_spend_authorizations",
        "logical_authorization_sha256",
    )? {
        tx.batch_execute(
            "ALTER TABLE managed_acceptance_spend_authorizations
             ADD COLUMN logical_authorization_sha256 TEXT",
        )
        .map_err(|error| format!("v33 spend logical identity column repair: {error}"))?;
    }
    let rows = tx
        .query(
            "SELECT spend_authorization_id, body_json, spend_body_sha256,
                    logical_authorization_sha256
             FROM managed_acceptance_spend_authorizations
             FOR UPDATE",
            &[],
        )
        .map_err(|error| format!("v33 spend body scan failed: {error}"))?;
    for row in rows {
        let spend_id: String = row.get(0);
        let raw_body: String = row.get(1);
        let stored_body_sha: String = row.get(2);
        let stored_logical: Option<String> = row.get(3);
        let mut body: Value = serde_json::from_str(&raw_body)
            .map_err(|error| format!("v33 spend {spend_id} body_json is invalid: {error}"))?;
        let original_sha = super::super::managed_acceptance::sha256_hex(
            super::super::managed_acceptance::canonical_json(&body)?.as_bytes(),
        );
        if original_sha != stored_body_sha {
            return Err(format!(
                "v33 spend {spend_id} body hash does not match its persisted body"
            ));
        }
        let logical = super::super::managed_acceptance::stable_spend_authorization_identity(&body)?;
        if let Some(stored) = stored_logical.as_deref() {
            if stored != logical {
                return Err(format!(
                    "v33 spend {spend_id} logical authorization hash is inconsistent"
                ));
            }
        }
        body.as_object_mut()
            .ok_or_else(|| format!("v33 spend {spend_id} body_json must be an object"))?
            .insert(
                "logical_authorization_sha256".to_string(),
                Value::String(logical.clone()),
            );
        let body_sha = super::super::managed_acceptance::sha256_hex(
            super::super::managed_acceptance::canonical_json(&body)?.as_bytes(),
        );
        tx.execute(
            "UPDATE managed_acceptance_spend_authorizations
             SET logical_authorization_sha256=$1, body_json=$2, spend_body_sha256=$3
             WHERE spend_authorization_id=$4",
            &[&logical, &body.to_string(), &body_sha, &spend_id],
        )
        .map_err(|error| format!("v33 spend {spend_id} backfill failed: {error}"))?;
    }

    let valid_check = tx
        .query(
            "SELECT pg_get_constraintdef(oid)
             FROM pg_constraint
             WHERE conrelid='managed_acceptance_spend_authorizations'::regclass
               AND contype='c'",
            &[],
        )
        .map_err(|error| format!("v33 spend constraint check failed: {error}"))?
        .into_iter()
        .map(|row| row.get::<_, String>(0))
        .any(|definition| {
            let normalized = definition
                .to_ascii_lowercase()
                .replace("::text", "")
                .replace("::character varying", "")
                .replace([' ', '\n', '\t', '(', ')', '\'', '"'], "");
            normalized.contains("status<>active")
                && normalized.contains("logical_authorization_sha256isnotnull")
        });
    if !valid_check {
        tx.batch_execute(
            "ALTER TABLE managed_acceptance_spend_authorizations
             ADD CONSTRAINT managed_acceptance_spend_active_logical_check
             CHECK (status <> 'active' OR logical_authorization_sha256 IS NOT NULL)",
        )
        .map_err(|error| format!("v33 spend active identity constraint repair: {error}"))?;
    }
    let index_definition: Option<String> = tx
        .query_opt(
            "SELECT indexdef FROM pg_indexes
             WHERE schemaname=current_schema()
               AND indexname='idx_managed_acceptance_spend_active_logical'",
            &[],
        )
        .map(|row| row.map(|row| row.get(0)))
        .map_err(|error| format!("v33 spend index lookup failed: {error}"))?;
    let index_ok = index_definition.as_deref().is_some_and(|definition| {
        let normalized = definition
            .to_ascii_lowercase()
            .replace("::text", "")
            .replace("::character varying", "")
            .replace([' ', '\n', '\t', '(', ')'], "");
        normalized.contains("createuniqueindex")
            && normalized.contains("tenant_id,logical_authorization_sha256")
            && normalized.contains("wherestatus='active'")
    });
    if !index_ok {
        if index_definition.is_some() {
            tx.batch_execute("DROP INDEX idx_managed_acceptance_spend_active_logical")
                .map_err(|error| format!("v33 spend index replacement failed: {error}"))?;
        }
        tx.batch_execute(
            "CREATE UNIQUE INDEX idx_managed_acceptance_spend_active_logical
             ON managed_acceptance_spend_authorizations(tenant_id, logical_authorization_sha256)
             WHERE status = 'active'",
        )
        .map_err(|error| format!("v33 spend active identity index repair: {error}"))?;
    }
    tx.batch_execute(
        "CREATE INDEX IF NOT EXISTS idx_managed_acceptance_spend_tenant
         ON managed_acceptance_spend_authorizations(tenant_id, status, expires_at)",
    )
    .map_err(|error| format!("v33 spend tenant index repair failed: {error}"))?;
    Ok(())
}

fn validate_pg_v33_schema(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    let version = client
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map_err(|error| error.to_string())?
        .get::<_, i64>(0);
    if version < super::super::migrations::V33_SCHEMA_VERSION {
        return Err(format!("PostgreSQL v33 schema version mismatch: {version}"));
    }
    for table in super::super::migrations::V33_TABLES {
        if !pg_table_present(client, table)? {
            return Err(format!("PostgreSQL v33 schema missing table {table}"));
        }
    }
    if !pg_column_exists(
        client,
        "managed_acceptance_spend_authorizations",
        "logical_authorization_sha256",
    )? {
        return Err("PostgreSQL v33 spend logical authorization identity is missing".to_string());
    }
    let active_identity_constraint_ok = client
        .query(
            "SELECT pg_get_constraintdef(constraint_meta.oid)
             FROM pg_constraint constraint_meta
             JOIN pg_class table_class ON table_class.oid=constraint_meta.conrelid
             JOIN pg_namespace namespace ON namespace.oid=table_class.relnamespace
             WHERE namespace.oid=current_schema()::regnamespace
               AND table_class.relname='managed_acceptance_spend_authorizations'
               AND constraint_meta.contype='c'",
            &[],
        )
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|row| {
            row.get::<_, String>(0)
                .to_ascii_lowercase()
                .replace('"', "")
        })
        .any(|definition| {
            definition.contains("logical_authorization_sha256 is not null")
                && definition.contains("status <>")
        });
    if !active_identity_constraint_ok {
        return Err(
            "PostgreSQL v33 active spend rows may omit logical authorization identity".to_string(),
        );
    }
    let index = client
        .query_opt(
            "SELECT index_meta.indisunique,
                    pg_get_expr(index_meta.indpred, index_meta.indrelid),
                    array_agg(attribute.attname ORDER BY key.ordinality)
             FROM pg_class index_class
             JOIN pg_namespace namespace ON namespace.oid=index_class.relnamespace
             JOIN pg_index index_meta ON index_meta.indexrelid=index_class.oid
             JOIN pg_class table_class ON table_class.oid=index_meta.indrelid
             JOIN LATERAL unnest(index_meta.indkey) WITH ORDINALITY AS key(attnum, ordinality)
               ON TRUE
             JOIN pg_attribute attribute
               ON attribute.attrelid=table_class.oid AND attribute.attnum=key.attnum
             WHERE namespace.oid=current_schema()::regnamespace
               AND index_class.relname='idx_managed_acceptance_spend_active_logical'
               AND table_class.relname='managed_acceptance_spend_authorizations'
             GROUP BY index_meta.indisunique, index_meta.indpred, index_meta.indrelid",
            &[],
        )
        .map_err(|error| error.to_string())?
        .map(|row| {
            (
                row.get::<_, bool>(0),
                row.get::<_, Option<String>>(1),
                row.get::<_, Vec<String>>(2),
            )
        });
    let predicate_ok = index.as_ref().is_some_and(|(_, predicate, _)| {
        predicate.as_ref().is_some_and(|predicate| {
            let normalized = predicate.to_ascii_lowercase();
            normalized.contains("status = 'active'")
                && !normalized.contains("logical_authorization_sha256 is not null")
        })
    });
    let expected_columns = vec![
        "tenant_id".to_string(),
        "logical_authorization_sha256".to_string(),
    ];
    let index_ok = matches!(
        index.as_ref(),
        Some((true, Some(_), columns)) if *columns == expected_columns
    );
    if !index_ok || !predicate_ok {
        return Err(
            "PostgreSQL v33 active logical spend index is missing or malformed".to_string(),
        );
    }
    validate_pg_v32_schema(client)
}

fn validate_pg_v32_schema(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    let version = client
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map_err(|error| error.to_string())?
        .get::<_, i64>(0);
    if version == super::super::migrations::V32_SCHEMA_VERSION {
        for table in super::super::migrations::V32_TABLES {
            if !pg_table_present(client, table)? {
                return Err(format!("PostgreSQL v32 schema missing table {table}"));
            }
        }
    } else if version < super::super::migrations::V32_SCHEMA_VERSION {
        return Err(format!("PostgreSQL v32 schema version mismatch: {version}"));
    } else {
        for table in super::super::migrations::V32_TABLES {
            if !pg_table_present(client, table)? {
                return Err(format!("PostgreSQL schema missing table {table}"));
            }
        }
    }
    for (index, expected_columns, predicate) in [
        (
            "idx_managed_acceptance_transition_one_child",
            vec!["decision_id", "previous_transition_sha256"],
            "previous_transition_sha256 is not null",
        ),
        (
            "idx_managed_acceptance_transition_one_genesis",
            vec!["decision_id"],
            "previous_transition_sha256 is null",
        ),
    ] {
        if !pg_partial_unique_index_matches(
            client,
            "managed_acceptance_decision_transition_receipts",
            index,
            &expected_columns,
            predicate,
        )? {
            return Err(format!(
                "PostgreSQL v32 transition index {index} is missing or malformed"
            ));
        }
    }
    validate_pg_v31_schema(client)
}

fn pg_partial_unique_index_matches(
    client: &mut impl postgres::GenericClient,
    table: &str,
    index: &str,
    expected_columns: &[&str],
    expected_predicate: &str,
) -> Result<bool, String> {
    let metadata = client
        .query_opt(
            "SELECT index_meta.indisunique,
                    pg_get_expr(index_meta.indpred, index_meta.indrelid),
                    array_agg(attribute.attname ORDER BY key.ordinality)
             FROM pg_class index_class
             JOIN pg_namespace namespace ON namespace.oid=index_class.relnamespace
             JOIN pg_index index_meta ON index_meta.indexrelid=index_class.oid
             JOIN pg_class table_class ON table_class.oid=index_meta.indrelid
             JOIN LATERAL unnest(index_meta.indkey) WITH ORDINALITY AS key(attnum, ordinality)
               ON TRUE
             JOIN pg_attribute attribute
               ON attribute.attrelid=table_class.oid AND attribute.attnum=key.attnum
             WHERE namespace.oid=current_schema()::regnamespace
               AND index_class.relname=$1
               AND table_class.relname=$2
             GROUP BY index_meta.indisunique, index_meta.indpred, index_meta.indrelid",
            &[&index, &table],
        )
        .map_err(|error| error.to_string())?
        .map(|row| {
            (
                row.get::<_, bool>(0),
                row.get::<_, Option<String>>(1),
                row.get::<_, Vec<String>>(2),
            )
        });
    let expected_columns = expected_columns
        .iter()
        .map(|column| (*column).to_string())
        .collect::<Vec<_>>();
    Ok(matches!(
        metadata,
        Some((true, Some(predicate), columns))
            if columns == expected_columns
                && predicate
                    .to_ascii_lowercase()
                    .contains(expected_predicate)
    ))
}

fn validate_pg_v31_schema(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    let version = client
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map_err(|error| error.to_string())?
        .get::<_, i64>(0);
    // Intermediate v31 validation only when head is still 31.
    if version == super::super::migrations::V31_SCHEMA_VERSION {
        for table in super::super::migrations::V31_TABLES {
            if !pg_table_present(client, table)? {
                return Err(format!("PostgreSQL v31 schema missing table {table}"));
            }
        }
    } else if version < super::super::migrations::V31_SCHEMA_VERSION {
        return Err(format!("PostgreSQL v31 schema version mismatch: {version}"));
    } else {
        for table in super::super::migrations::V31_TABLES {
            if !pg_table_present(client, table)? {
                return Err(format!("PostgreSQL schema missing table {table}"));
            }
        }
    }
    validate_pg_v30_tables(client)
}

fn validate_pg_v30_tables(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    for table in super::super::migrations::V30_TABLES {
        if !pg_table_present(client, table)? {
            return Err(format!("PostgreSQL v30 schema missing table {table}"));
        }
    }
    validate_pg_v29_tables(client)
}

fn validate_pg_v29_tables(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    for table in super::super::migrations::V29_TABLES {
        if !pg_table_present(client, table)? {
            return Err(format!("PostgreSQL v29 schema missing table {table}"));
        }
    }
    validate_pg_v28_tables(client)
}

fn validate_pg_v28_tables(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    for table in super::super::migrations::V28_TABLES {
        if !pg_table_present(client, table)? {
            return Err(format!("PostgreSQL schema missing table {table}"));
        }
    }
    validate_pg_v27_tables(client)
}

fn apply_pg_v28_migration(client: &mut postgres::Client) -> Result<(), String> {
    let version = super::super::migrations::V28_SCHEMA_VERSION;
    let mut tx = client
        .transaction()
        .map_err(|error| format!("failed to start migration 28 transaction: {error}"))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
             hashtext(current_database()), hashtext(current_schema())
         )",
        &[],
    )
    .map_err(|error| format!("failed to lock migration 28: {error}"))?;
    let current_version = tx
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map(|row| row.get::<_, i64>(0))
        .map_err(|error| format!("failed to re-read version for migration 28: {error}"))?;
    if current_version >= version {
        tx.commit()
            .map_err(|error| format!("failed to finish migration 28 no-op: {error}"))?;
        return Ok(());
    }
    tx.batch_execute(schema::V28_DDL)
        .map_err(|error| format!("migration 28 failed: {error}"))?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&version],
    )
    .map_err(|error| format!("failed to record migration {version}: {error}"))?;
    tx.commit()
        .map_err(|error| format!("failed to commit migration 28: {error}"))
}

fn validate_pg_v27_tables(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    for table in super::super::migrations::V27_TABLES {
        if !pg_table_present(client, table)? {
            return Err(format!("PostgreSQL schema missing table {table}"));
        }
    }
    validate_pg_v26_tables(client)
}

fn validate_pg_v26_tables(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    // Shared column checks for the v26 recursive surface (also required under v27).
    let required = [
        ("recursive_execution_trees", "root_run_id", "text", true),
        ("recursive_execution_trees", "workflow_id", "text", true),
        ("recursive_execution_trees", "root_node_id", "text", true),
        (
            "recursive_execution_trees",
            "tree_schema_version",
            "text",
            true,
        ),
        ("recursive_execution_trees", "tree_json", "text", true),
        ("recursive_execution_trees", "version", "bigint", true),
        ("recursive_execution_trees", "created_at", "text", true),
        ("recursive_execution_trees", "updated_at", "text", true),
        ("recursive_execution_nodes", "node_id", "text", true),
        ("recursive_execution_nodes", "root_run_id", "text", true),
        ("recursive_execution_nodes", "parent_node_id", "text", false),
        ("recursive_execution_nodes", "proposal_id", "text", false),
        ("recursive_execution_nodes", "depth", "bigint", true),
        (
            "recursive_execution_nodes",
            "objective_fingerprint",
            "text",
            true,
        ),
        ("recursive_execution_nodes", "status", "text", true),
        ("recursive_execution_nodes", "version", "bigint", true),
        ("recursive_execution_nodes", "created_at", "text", true),
        ("recursive_execution_nodes", "updated_at", "text", true),
    ];
    for (table, column, expected_type, expected_not_null) in required {
        let actual: Option<(String, String)> = client
            .query_opt(
                "SELECT data_type, is_nullable FROM information_schema.columns
                 WHERE table_schema=current_schema() AND table_name=$1 AND column_name=$2",
                &[&table, &column],
            )
            .map_err(|error| error.to_string())?
            .map(|row| (row.get(0), row.get(1)));
        if actual.as_ref().map(|(data_type, _)| data_type.as_str()) != Some(expected_type)
            || actual
                .as_ref()
                .is_some_and(|(_, nullable)| (nullable == "NO") != expected_not_null)
        {
            return Err(format!(
                "PostgreSQL v26 schema type or nullability mismatch for {table}.{column}"
            ));
        }
    }
    for (table, expected_columns) in [
        ("recursive_execution_trees", ["root_run_id"].as_slice()),
        (
            "recursive_execution_nodes",
            ["root_run_id", "node_id"].as_slice(),
        ),
    ] {
        let primary_key: bool = client
            .query_one(
                "SELECT EXISTS(
                     SELECT 1 FROM pg_constraint c
                     JOIN pg_class t ON t.oid=c.conrelid
                     WHERE t.relnamespace=current_schema()::regnamespace
                       AND t.relname=$1 AND c.contype='p'
                       AND (SELECT array_agg(a.attname ORDER BY key.ordinality)
                            FROM unnest(c.conkey) WITH ORDINALITY AS key(attnum, ordinality)
                            JOIN pg_attribute a
                              ON a.attrelid=t.oid AND a.attnum=key.attnum)=$2
                 )",
                &[&table, &expected_columns],
            )
            .map_err(|error| error.to_string())?
            .get(0);
        if !primary_key {
            return Err(format!(
                "PostgreSQL v26 schema missing primary key for {table}"
            ));
        }
    }
    for (index, expected_table, expected_columns) in [
        (
            "idx_recursive_execution_trees_workflow",
            "recursive_execution_trees",
            ["workflow_id", "updated_at"].as_slice(),
        ),
        (
            "idx_recursive_execution_nodes_root",
            "recursive_execution_nodes",
            ["root_run_id", "depth", "node_id"].as_slice(),
        ),
        (
            "idx_recursive_execution_nodes_parent",
            "recursive_execution_nodes",
            ["root_run_id", "parent_node_id", "status", "node_id"].as_slice(),
        ),
    ] {
        let definition: Option<(String, String, bool, bool, Vec<String>)> = client
            .query_opt(
                "SELECT table_class.relname, access_method.amname,
                        index_meta.indisunique, index_meta.indpred IS NULL,
                        array_agg(attribute.attname ORDER BY key.ordinality)
                 FROM pg_class index_class
                 JOIN pg_namespace namespace ON namespace.oid=index_class.relnamespace
                 JOIN pg_index index_meta ON index_meta.indexrelid=index_class.oid
                 JOIN pg_class table_class ON table_class.oid=index_meta.indrelid
                 JOIN pg_am access_method ON access_method.oid=index_class.relam
                 JOIN LATERAL unnest(index_meta.indkey) WITH ORDINALITY AS key(attnum, ordinality)
                   ON TRUE
                 JOIN pg_attribute attribute
                   ON attribute.attrelid=table_class.oid AND attribute.attnum=key.attnum
                 WHERE namespace.oid=current_schema()::regnamespace AND index_class.relname=$1
                 GROUP BY table_class.relname, access_method.amname,
                          index_meta.indisunique, index_meta.indpred",
                &[&index],
            )
            .map_err(|error| error.to_string())?
            .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4)));
        let expected_columns = expected_columns
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        if definition
            != Some((
                expected_table.to_string(),
                "btree".to_string(),
                false,
                true,
                expected_columns,
            ))
        {
            return Err(format!(
                "PostgreSQL v26 schema missing or malformed index {index}"
            ));
        }
    }
    let foreign_key: bool = client
        .query_one(
            "SELECT EXISTS(
                 SELECT 1 FROM pg_constraint c
                 JOIN pg_class child ON child.oid=c.conrelid
                 JOIN pg_class parent ON parent.oid=c.confrelid
                 WHERE child.relnamespace=current_schema()::regnamespace
                   AND parent.relnamespace=current_schema()::regnamespace
                   AND child.relname='recursive_execution_nodes'
                   AND parent.relname='recursive_execution_trees'
                   AND c.contype='f'
                   AND cardinality(c.conkey)=1 AND cardinality(c.confkey)=1
                   AND (SELECT a.attname FROM pg_attribute a
                        WHERE a.attrelid=child.oid AND a.attnum=c.conkey[1])='root_run_id'
                   AND (SELECT a.attname FROM pg_attribute a
                        WHERE a.attrelid=parent.oid AND a.attnum=c.confkey[1])='root_run_id'
             )",
            &[],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    if !foreign_key {
        return Err("PostgreSQL v26 schema missing recursive root foreign key".to_string());
    }
    let depth_check: bool = client
        .query_one(
            "SELECT EXISTS(
                 SELECT 1 FROM pg_constraint c
                 JOIN pg_class t ON t.oid=c.conrelid
                 WHERE t.relnamespace=current_schema()::regnamespace
                   AND t.relname='recursive_execution_nodes'
                   AND c.contype='c'
                   AND regexp_replace(
                       lower(pg_get_expr(c.conbin, c.conrelid)),
                       '[[:space:]()]', '', 'g'
                   ) IN ('depth>=0anddepth<=2', 'depthbetween0and2')
             )",
            &[],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    if !depth_check {
        return Err("PostgreSQL v26 schema missing recursive depth constraint".to_string());
    }
    let unique_identity: bool = client
        .query_one(
            "SELECT EXISTS(
                 SELECT 1 FROM pg_constraint c
                 JOIN pg_class t ON t.oid=c.conrelid
                 WHERE t.relnamespace=current_schema()::regnamespace
                   AND t.relname='recursive_execution_nodes'
                   AND c.contype='u'
                   AND cardinality(c.conkey)=2
                   AND (SELECT a.attname FROM pg_attribute a
                        WHERE a.attrelid=t.oid AND a.attnum=c.conkey[1])='root_run_id'
                   AND (SELECT a.attname FROM pg_attribute a
                        WHERE a.attrelid=t.oid AND a.attnum=c.conkey[2])='objective_fingerprint'
             )",
            &[],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    if !unique_identity {
        return Err("PostgreSQL v26 schema missing recursive objective uniqueness".to_string());
    }
    let fingerprint_check: bool = client
        .query_one(
            "SELECT EXISTS(
                 SELECT 1 FROM pg_constraint c
                 JOIN pg_class t ON t.oid=c.conrelid
                 WHERE t.relnamespace=current_schema()::regnamespace
                   AND t.relname='recursive_execution_nodes'
                   AND c.contype='c'
                   AND regexp_replace(
                       lower(pg_get_expr(c.conbin, c.conrelid)),
                       '[[:space:]()]', '', 'g'
                   )='lengthobjective_fingerprint=64'
             )",
            &[],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    if !fingerprint_check {
        return Err(
            "PostgreSQL v26 schema missing objective fingerprint length constraint".to_string(),
        );
    }
    Ok(())
}

fn pg_v25_operation_schema_valid(
    client: &mut impl postgres::GenericClient,
) -> Result<bool, String> {
    let required_not_null = [
        "operation_id",
        "operation_kind",
        "target_memory_id",
        "target_version",
        "tenant_id",
        "workspace_id",
        "source_id",
        "source_sha256",
        "request_identity_sha256",
        "operation_binding_sha256",
        "content_sha256",
        "contract_json",
        "contract_sha256",
        "receipt_sha256",
        "provider_id",
        "requested_model_id",
        "resolved_model_id",
        "dimensions",
        "reservation_event_id",
        "state",
        "attempt_count",
        "created_at",
        "updated_at",
    ];
    for column in required_not_null {
        let valid: bool = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM information_schema.columns
             WHERE table_schema=current_schema() AND table_name='provider_embedding_operations'
               AND column_name=$1 AND is_nullable='NO')",
                &[&column],
            )
            .map_err(|error| error.to_string())?
            .get(0);
        if !valid {
            return Ok(false);
        }
    }
    let constraints = client
        .query(
            "SELECT contype::TEXT,pg_get_constraintdef(oid)
         FROM pg_constraint WHERE conrelid='provider_embedding_operations'::regclass",
            &[],
        )
        .map_err(|error| error.to_string())?;
    let normalize_constraint = |value: &str| {
        value
            .to_ascii_lowercase()
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
    };
    let actual_constraints = constraints
        .iter()
        .map(|row| {
            format!(
                "{}:{}",
                row.get::<_, String>(0),
                normalize_constraint(&row.get::<_, String>(1))
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    let expected_constraints = [
        "p:PRIMARY KEY (operation_id)",
        "u:UNIQUE (target_memory_id, target_version)",
        "f:FOREIGN KEY (reservation_event_id) REFERENCES provider_audit_events(event_id)",
        "f:FOREIGN KEY (send_event_id) REFERENCES provider_audit_events(event_id)",
        "f:FOREIGN KEY (outcome_event_id) REFERENCES provider_audit_events(event_id)",
        "c:CHECK (((attempt_count >= 1) AND (attempt_count <= 4)))",
        "c:CHECK ((dimensions > 0))",
        "c:CHECK ((length(content_sha256) = 64))",
        "c:CHECK ((length(contract_sha256) = 64))",
        "c:CHECK ((length(operation_binding_sha256) = 64))",
        "c:CHECK ((length(receipt_sha256) = 64))",
        "c:CHECK ((length(request_identity_sha256) = 64))",
        "c:CHECK ((length(source_sha256) = 64))",
        "c:CHECK ((operation_kind = ANY (ARRAY['memory_version'::text, 'retrieval_query'::text])))",
        "c:CHECK ((((operation_kind = 'memory_version'::text) AND (node_id IS NULL) AND (query_sha256 IS NULL)) OR ((operation_kind = 'retrieval_query'::text) AND (run_id IS NOT NULL) AND (node_id IS NOT NULL) AND (query_sha256 IS NOT NULL) AND (query_sha256 = source_sha256))))",
        "c:CHECK (((query_sha256 IS NULL) OR (length(query_sha256) = 64)))",
        "c:CHECK ((((result_kind IS NULL) AND (result_id IS NULL) AND (result_sha256 IS NULL)) OR ((result_kind IS NOT NULL) AND (result_id IS NOT NULL) AND (result_sha256 IS NOT NULL))))",
        "c:CHECK (((result_kind IS NULL) OR (result_kind = ANY (ARRAY['memory_version'::text, 'retrieval_event'::text]))))",
        "c:CHECK (((result_sha256 IS NULL) OR (length(result_sha256) = 64)))",
        "c:CHECK ((state = ANY (ARRAY['preflight_reserved'::text, 'reserved'::text, 'sending'::text, 'network_succeeded'::text, 'succeeded'::text, 'result_erased'::text, 'failed_before_send'::text, 'failed_known_outcome'::text, 'outcome_unknown'::text, 'outcome_unknown_acknowledged'::text, 'retry_authorized'::text])))",
    ]
    .into_iter()
    .map(|constraint| {
        let (kind, definition) = constraint.split_once(':').expect("constraint kind");
        format!("{kind}:{}", normalize_constraint(definition))
    })
    .collect::<std::collections::BTreeSet<_>>();
    if actual_constraints != expected_constraints {
        return Ok(false);
    }
    let indexes = client
        .query(
            "SELECT indexname,indexdef FROM pg_indexes
         WHERE schemaname=current_schema() AND tablename='provider_embedding_operations'",
            &[],
        )
        .map_err(|error| error.to_string())?;
    let indexes = indexes
        .iter()
        .map(|row| {
            (
                row.get::<_, String>(0).to_ascii_lowercase(),
                row.get::<_, String>(1).to_ascii_lowercase(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let state_index = indexes
        .get("idx_provider_embedding_operations_state")
        .is_some_and(|definition| {
            definition.contains("using btree (state, updated_at)")
                && !definition.contains(" where ")
        });
    let retrieval_index = indexes
        .get("idx_provider_embedding_operations_retrieval_identity")
        .is_some_and(|definition| {
            definition.starts_with("create unique index")
                && definition.contains(
                    "using btree (tenant_id, workspace_id, run_id, node_id, query_sha256, provider_id, requested_model_id, resolved_model_id, dimensions, request_identity_sha256)",
                )
                && definition
                    .ends_with("where (operation_kind = 'retrieval_query'::text)")
        });
    Ok(state_index && retrieval_index)
}

impl LocalProductStore {
    pub(crate) fn rollback_pg_v38_to_v37(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_pg_conn(|client| {
            let mut tx = client.transaction().map_err(|e| e.to_string())?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtext(current_database()), hashtext(current_schema()))",
                &[],
            )
            .map_err(|e| e.to_string())?;
            tx.batch_execute(
                "LOCK TABLE schema_migrations,
                    harness_evolution_ec3_lifecycle_cost_records,
                    harness_evolution_ec3_lifecycle_costs,
                    harness_evolution_ec3_lifecycle_budgets,
                    harness_evolution_ec3_lifecycle_reconciliations
                 IN ACCESS EXCLUSIVE MODE",
            )
            .map_err(|e| e.to_string())?;
            let version: i64 = tx
                .query_one("SELECT COALESCE(MAX(version),0) FROM schema_migrations", &[])
                .map_err(|e| e.to_string())?
                .get(0);
            if version != super::super::migrations::V38_SCHEMA_VERSION {
                return Err(format!(
                    "v38 rollback requires current schema version 38; found {version}"
                ));
            }
            for table in super::super::migrations::V38_TABLES {
                let occupied: i64 = tx
                    .query_one(&format!("SELECT COUNT(*) FROM {table}"), &[])
                    .map_err(|e| e.to_string())?
                    .get(0);
                if occupied > 0 {
                    return Err(format!(
                        "v38 rollback blocked: lifecycle-cost observations exist in {table}"
                    ));
                }
            }
            tx.batch_execute(
                "DROP INDEX IF EXISTS idx_harness_evolution_ec3_lifecycle_reconciliations_candidate;
                 DROP INDEX IF EXISTS idx_harness_evolution_ec3_lifecycle_budgets_contract;
                 DROP INDEX IF EXISTS idx_harness_evolution_ec3_lifecycle_costs_candidate;
                 DROP INDEX IF EXISTS idx_harness_evolution_ec3_cost_task_run;
                 DROP INDEX IF EXISTS idx_harness_evolution_ec3_cost_candidate;
                 DROP TABLE IF EXISTS harness_evolution_ec3_lifecycle_reconciliations;
                 DROP TABLE IF EXISTS harness_evolution_ec3_lifecycle_budgets;
                 DROP TABLE IF EXISTS harness_evolution_ec3_lifecycle_costs;
                 DROP TABLE IF EXISTS harness_evolution_ec3_lifecycle_cost_records;",
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "DELETE FROM schema_migrations WHERE version=$1",
                &[&super::super::migrations::V38_SCHEMA_VERSION],
            )
            .map_err(|e| e.to_string())?;
            let details = serde_json::json!({
                "from_version": super::super::migrations::V38_SCHEMA_VERSION,
                "to_version": super::super::migrations::V37_SCHEMA_VERSION,
                "tables": super::super::migrations::V38_TABLES,
                "indexes": super::super::migrations::V38_INDEXES,
            })
            .to_string();
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1,$2,'schema.rollback.v38_to_v37','local_product_store',$3)",
                &[&now, &actor, &details],
            )
            .map_err(|e| e.to_string())?;
            validate_pg_v37_schema(&mut tx)?;
            tx.commit().map_err(|e| e.to_string())
        })
    }

    pub(crate) fn rollback_pg_v36_to_v35(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_pg_conn(|client| {
            let mut tx = client.transaction().map_err(|e| e.to_string())?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtext(current_database()), hashtext(current_schema()))",
                &[],
            )
            .map_err(|e| e.to_string())?;
            let version: i64 = tx
                .query_one("SELECT COALESCE(MAX(version),0) FROM schema_migrations", &[])
                .map_err(|e| e.to_string())?
                .get(0);
            if version != super::super::migrations::V36_SCHEMA_VERSION {
                return Err(format!("v36 rollback requires current schema version 36; found {version}"));
            }
            tx.batch_execute(
                "LOCK TABLE managed_acceptance_delegations, api_key_metadata IN ACCESS EXCLUSIVE MODE",
            )
            .map_err(|e| e.to_string())?;
            let api_key_tenant_archives = tx
                .query(
                    "SELECT key_id, user_id, role, tenant_id, scopes_json, revoked_at, expires_at
                     FROM api_key_metadata ORDER BY key_id",
                    &[],
                )
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|row| {
                    let scopes_json: String = row.get(4);
                    let scopes: Value = serde_json::from_str(&scopes_json).map_err(|error| {
                        format!("v36 rollback blocked: API-key scopes JSON is invalid: {error}")
                    })?;
                    super::super::migrations::build_v36_api_key_tenant_binding_archive(
                        serde_json::json!({
                            "key_id": row.get::<_, String>(0),
                            "user_id": row.get::<_, String>(1),
                            "role": row.get::<_, String>(2),
                            "tenant_id": row.get::<_, Option<String>>(3),
                            "scopes": scopes,
                            "revoked_at": row.get::<_, Option<String>>(5),
                            "expires_at": row.get::<_, Option<String>>(6),
                        }),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let archives = tx
                .query(
                    "SELECT delegation_sha256, product_task_id, body_json, proposal_sha256, proposal_json,
                            status, total_cost_usd, manifest_approval_sha256,
                            manifest_approval_json, spend_body_sha256, spend_body_json,
                            spend_status, manifest_json, attempt_id, attempt_lease_id,
                            attempt_lease_token, attempt_status,
                            artifact_confirmation_sha256, artifact_confirmation_json,
                            provider_request_journal_json, terminal_receipt_json, terminal_at
                     FROM managed_acceptance_delegations
                     ORDER BY delegation_sha256",
                    &[],
                )
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|row| {
                    super::super::migrations::build_v36_delegation_downgrade_archive(
                        super::super::migrations::V36DelegationArchiveSource {
                            delegation_sha256: row.get(0),
                            product_task_id: row.get(1),
                            body_json: row.get(2),
                            proposal_sha256: row.get(3),
                            proposal_json: row.get(4),
                            status: row.get(5),
                            total_cost_usd: row.get(6),
                            manifest_approval_sha256: row.get(7),
                            manifest_approval_json: row.get(8),
                            spend_body_sha256: row.get(9),
                            spend_body_json: row.get(10),
                            spend_status: row.get(11),
                            manifest_json: row.get(12),
                            attempt_id: row.get(13),
                            attempt_lease_id: row.get(14),
                            attempt_lease_token: row.get(15),
                            attempt_status: row.get(16),
                            artifact_confirmation_sha256: row.get(17),
                            artifact_confirmation_json: row.get(18),
                            provider_request_journal_json: row.get(19),
                            terminal_receipt_json: row.get(20),
                            terminal_at: row.get(21),
                        },
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            for archive in &archives {
                let delegation_sha = archive
                    .pointer("/source_evidence/delegation_sha256")
                    .and_then(Value::as_str)
                    .ok_or("v36 downgrade archive delegation hash is missing")?;
                let resource = format!(
                    "managed_delegation_archive:{}",
                    &delegation_sha[..16]
                );
                let details =
                    super::super::managed_acceptance::canonical_json(archive)?;
                tx.execute(
                    "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                     VALUES ($1,$2,'schema.rollback.v36_delegation_archived',$3,$4)",
                    &[&now, &actor, &resource, &details],
                )
                .map_err(|error| error.to_string())?;
            }
            let archive_hashes = Value::Array(
                archives
                    .iter()
                    .filter_map(|archive| archive.get("archive_sha256").cloned())
                    .collect(),
            );
            let archive_set_sha256 =
                super::super::managed_acceptance::sha256_hex(
                    super::super::managed_acceptance::canonical_json(&archive_hashes)?
                        .as_bytes(),
                );
            for archive in &api_key_tenant_archives {
                let key_id = archive
                    .pointer("/source_evidence/key_id")
                    .and_then(Value::as_str)
                    .ok_or("v36 API-key binding archive key_id is missing")?;
                let details = super::super::managed_acceptance::canonical_json(archive)?;
                tx.execute(
                    "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                     VALUES ($1,$2,'schema.rollback.v36_api_key_tenant_archived',$3,$4)",
                    &[
                        &now,
                        &actor,
                        &format!("api_key_tenant_binding:{key_id}"),
                        &details,
                    ],
                )
                .map_err(|error| error.to_string())?;
            }
            let api_key_tenant_archive_hashes = Value::Array(
                api_key_tenant_archives
                    .iter()
                    .filter_map(|archive| archive.get("archive_sha256").cloned())
                    .collect(),
            );
            let api_key_tenant_archive_set_sha256 =
                super::super::managed_acceptance::sha256_hex(
                    super::super::managed_acceptance::canonical_json(
                        &api_key_tenant_archive_hashes,
                    )?
                    .as_bytes(),
                );
            tx.batch_execute(
                "DROP INDEX IF EXISTS idx_workflow_plans_delegated_owner;
                 ALTER TABLE workflow_plans DROP COLUMN delegated_plan_owner_id;
                 DROP INDEX IF EXISTS idx_managed_acceptance_delegations_lease;
                 DROP INDEX IF EXISTS idx_managed_acceptance_delegations_attempt;
                 DROP INDEX IF EXISTS idx_managed_acceptance_delegations_spend;
                 DROP INDEX IF EXISTS idx_managed_acceptance_delegations_status;
                 DROP TABLE IF EXISTS managed_acceptance_delegations;
                 ALTER TABLE api_key_metadata DROP COLUMN tenant_id;",
            )
            .map_err(|e| e.to_string())?;
            let details = serde_json::json!({
                "from_version": 36,
                "to_version": 35,
                "tables": super::super::migrations::V36_TABLES,
                "archived_delegations": archives.len(),
                "archive_set_sha256": archive_set_sha256,
                "archived_api_key_tenant_bindings": api_key_tenant_archives.len(),
                "api_key_tenant_archive_set_sha256": api_key_tenant_archive_set_sha256,
            })
            .to_string();
            tx.execute(
                "DELETE FROM schema_migrations WHERE version=$1",
                &[&super::super::migrations::V36_SCHEMA_VERSION],
            )
            .map_err(|e| e.to_string())?;
            validate_pg_v35_schema(&mut tx)?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1,$2,'schema.rollback.v36_to_v35','local_product_store',$3)",
                &[&now, &actor, &details],
            )
            .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())
        })
    }

    pub(crate) fn rollback_pg_v35_to_v34_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client| {
            let mut tx = client.transaction().map_err(|e| e.to_string())?;
            // Serialize this destructive schema transition with the v35
            // receipt publisher. A receipt transaction takes the same
            // transaction-scoped schema lock before it can move a task into
            // workspace_preparing, so rollback cannot observe an empty table
            // and then erase a concurrently-published recovery boundary.
            tx.query_one(
                "SELECT pg_advisory_xact_lock(
                     hashtext(current_database()), hashtext(current_schema())
                 )",
                &[],
            )
            .map_err(|e| e.to_string())?;
            tx.batch_execute(
                "LOCK TABLE schema_migrations IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE product_task_workspace_preparations IN ACCESS EXCLUSIVE MODE;",
            )
            .map_err(|e| e.to_string())?;
            let version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map_err(|e| e.to_string())?
                .get::<_, i64>(0);
            if version != super::super::migrations::V35_SCHEMA_VERSION {
                return Err(format!(
                    "v35 rollback requires current schema version 35; found {version}"
                ));
            }
            for table in super::super::migrations::V35_TABLES {
                let count: i64 = tx
                    .query_one(&format!("SELECT COUNT(*) FROM {table}"), &[])
                    .map_err(|e| e.to_string())?
                    .get(0);
                if count > 0 {
                    return Err(format!(
                        "v35 rollback blocked: ProductTask preparation receipts exist in {table}"
                    ));
                }
            }
            tx.batch_execute(
                "DROP INDEX IF EXISTS idx_product_task_workspace_preparations_state;
                 DROP TABLE IF EXISTS product_task_workspace_preparations;",
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "DELETE FROM schema_migrations WHERE version=$1",
                &[&super::super::migrations::V35_SCHEMA_VERSION],
            )
            .map_err(|e| e.to_string())?;
            let details = serde_json::json!({
                "from_version": super::super::migrations::V35_SCHEMA_VERSION,
                "to_version": super::super::migrations::V34_SCHEMA_VERSION,
                "tables": super::super::migrations::V35_TABLES,
            })
            .to_string();
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1,$2,'schema.rollback.v35_to_v34','local_product_store',$3)",
                &[&now, &actor, &details],
            )
            .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())
        })
    }

    pub(crate) fn rollback_pg_v34_to_v33_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client| {
            let mut tx = client.transaction().map_err(|e| e.to_string())?;
            let version = tx.query_one("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", &[]).map_err(|e| e.to_string())?.get::<_, i64>(0);
            if version != super::super::migrations::V34_SCHEMA_VERSION {
                return Err(format!("v34 rollback requires current schema version 34; found {version}"));
            }
            for table in super::super::migrations::V34_TABLES {
                let count: i64 = tx.query_one(&format!("SELECT COUNT(*) FROM {table}"), &[]).map_err(|e| e.to_string())?.get(0);
                if count > 0 {
                    return Err(format!("v34 rollback blocked: RWE authority exists in {table}"));
                }
            }
            tx.batch_execute("DROP TABLE IF EXISTS rwe_task_attempts; DROP TABLE IF EXISTS rwe_runs; DROP TABLE IF EXISTS rwe_run_authorizations;").map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM schema_migrations WHERE version=$1", &[&super::super::migrations::V34_SCHEMA_VERSION]).map_err(|e| e.to_string())?;
            let details = serde_json::json!({"from_version": 34, "to_version": 33}).to_string();
            tx.execute("INSERT INTO audit_log (created_at, actor, action, resource, details_json) VALUES ($1,$2,'schema.rollback.v34_to_v33','local_product_store',$3)", &[&now, &actor, &details]).map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())
        })
    }

    pub(crate) fn rollback_pg_v33_to_v32_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client| {
            let mut tx = client.transaction().map_err(|e| e.to_string())?;
            let version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map_err(|e| e.to_string())?
                .get::<_, i64>(0);
            if version != super::super::migrations::V33_SCHEMA_VERSION {
                return Err(format!(
                    "v33 rollback requires current schema version 33; found {version}"
                ));
            }
            for table in super::super::migrations::V33_TABLES {
                let count: i64 = tx
                    .query_one(&format!("SELECT COUNT(*) FROM {table}"), &[])
                    .map_err(|e| e.to_string())?
                    .get(0);
                if count > 0 {
                    return Err(format!(
                        "v33 rollback blocked: managed acceptance spend exists in {table}"
                    ));
                }
            }
            tx.batch_execute("DROP TABLE IF EXISTS managed_acceptance_spend_authorizations;")
                .map_err(|e| e.to_string())?;
            tx.execute(
                "DELETE FROM schema_migrations WHERE version=$1",
                &[&super::super::migrations::V33_SCHEMA_VERSION],
            )
            .map_err(|e| e.to_string())?;
            let details = serde_json::json!({
                "from_version": super::super::migrations::V33_SCHEMA_VERSION,
                "to_version": super::super::migrations::V32_SCHEMA_VERSION,
                "tables": super::super::migrations::V33_TABLES,
            })
            .to_string();
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1, $2, 'schema.rollback.v33_to_v32', 'local_product_store', $3)",
                &[&now, &actor, &details],
            )
            .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())
        })
    }

    pub(crate) fn rollback_pg_v32_to_v31_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client: &mut postgres::Client| {
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            let current_version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map(|row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_v32_rollback_source(current_version)?;
            for table in super::super::migrations::V32_TABLES {
                tx.batch_execute(&format!("LOCK TABLE {table} IN ACCESS EXCLUSIVE MODE"))
                    .map_err(|error| error.to_string())?;
                let occupied = tx
                    .query_one(
                        &format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)"),
                        &[],
                    )
                    .map(|row| row.get::<_, bool>(0))
                    .map_err(|error| error.to_string())?;
                if occupied {
                    return Err(format!(
                        "v32 rollback blocked: managed acceptance authority exists in {table}"
                    ));
                }
            }
            tx.batch_execute(
                "DROP TABLE managed_acceptance_decision_transition_receipts;
                 DROP TABLE managed_acceptance_attempts;
                 DROP TABLE managed_acceptance_authorizations;
                 DROP TABLE managed_acceptance_decisions;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1,$2,'schema.rollback.v32_to_v31','local_product_store',$3)",
                &[
                    &now,
                    &actor,
                    &serde_json::json!({
                        "from_version": super::super::migrations::V32_SCHEMA_VERSION,
                        "to_version": super::super::migrations::V31_SCHEMA_VERSION,
                        "tables": super::super::migrations::V32_TABLES,
                    })
                    .to_string(),
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "DELETE FROM schema_migrations WHERE version=$1",
                &[&super::super::migrations::V32_SCHEMA_VERSION],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    pub(in crate::storage::local_product_store) fn rollback_pg_v31_to_v30_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client: &mut postgres::Client| {
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            let current_version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map(|row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_v31_rollback_source(current_version)?;
            tx.batch_execute("LOCK TABLE product_task_terminal_evidence IN ACCESS EXCLUSIVE MODE")
                .map_err(|error| error.to_string())?;
            let occupied = tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM product_task_terminal_evidence LIMIT 1)",
                    &[],
                )
                .map(|row| row.get::<_, bool>(0))
                .map_err(|error| error.to_string())?;
            if occupied {
                return Err(
                    "v31 rollback blocked: authoritative terminal evidence exists".to_string(),
                );
            }
            tx.batch_execute("DROP TABLE product_task_terminal_evidence;")
                .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1,$2,'schema.rollback.v31_to_v30','local_product_store',$3)",
                &[
                    &now,
                    &actor,
                    &serde_json::json!({
                        "from_version": super::super::migrations::V31_SCHEMA_VERSION,
                        "to_version": super::super::migrations::V30_SCHEMA_VERSION,
                        "tables": super::super::migrations::V31_TABLES,
                    })
                    .to_string(),
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "DELETE FROM schema_migrations WHERE version=$1",
                &[&super::super::migrations::V31_SCHEMA_VERSION],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    pub(in crate::storage::local_product_store) fn rollback_pg_v30_to_v29_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client: &mut postgres::Client| {
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            let current_version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map(|row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_v30_rollback_source(current_version)?;
            tx.batch_execute("LOCK TABLE product_tasks IN ACCESS EXCLUSIVE MODE")
                .map_err(|error| error.to_string())?;
            for table in super::super::migrations::V30_TABLES {
                let occupied = tx
                    .query_one(
                        &format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)"),
                        &[],
                    )
                    .map(|row| row.get::<_, bool>(0))
                    .map_err(|error| error.to_string())?;
                if occupied {
                    return Err(format!(
                        "v30 rollback blocked: authoritative product task data exists in {table}"
                    ));
                }
            }
            tx.batch_execute("DROP TABLE product_tasks;")
                .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1,$2,'schema.rollback.v30_to_v29','local_product_store',$3)",
                &[
                    &now,
                    &actor,
                    &serde_json::json!({
                        "from_version": super::super::migrations::V30_SCHEMA_VERSION,
                        "to_version": super::super::migrations::V29_SCHEMA_VERSION,
                        "tables": super::super::migrations::V30_TABLES,
                    })
                    .to_string(),
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "DELETE FROM schema_migrations WHERE version=$1",
                &[&super::super::migrations::V30_SCHEMA_VERSION],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    pub(in crate::storage::local_product_store) fn rollback_pg_v29_to_v28_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client: &mut postgres::Client| {
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            let current_version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map(|row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_v29_rollback_source(current_version)?;
            tx.batch_execute(
                "LOCK TABLE harness_evolution_pr_ready_receipts, harness_evolution_pr_ready_bundles
                 IN ACCESS EXCLUSIVE MODE",
            )
            .map_err(|error| error.to_string())?;
            for table in super::super::migrations::V29_TABLES {
                let occupied = tx
                    .query_one(&format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)"), &[])
                    .map(|row| row.get::<_, bool>(0))
                    .map_err(|error| error.to_string())?;
                if occupied {
                    return Err(format!(
                        "v29 rollback blocked: authoritative harness evolution PR_READY data exists in {table}"
                    ));
                }
            }
            tx.batch_execute(
                "DROP TABLE harness_evolution_pr_ready_receipts;
                 DROP TABLE harness_evolution_pr_ready_bundles;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1,$2,'schema.rollback.v29_to_v28','local_product_store',$3)",
                &[
                    &now,
                    &actor,
                    &serde_json::json!({
                        "from_version": super::super::migrations::V29_SCHEMA_VERSION,
                        "to_version": super::super::migrations::V28_SCHEMA_VERSION,
                        "tables": super::super::migrations::V29_TABLES,
                    })
                    .to_string(),
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "DELETE FROM schema_migrations WHERE version=$1",
                &[&super::super::migrations::V29_SCHEMA_VERSION],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    pub(in crate::storage::local_product_store) fn rollback_pg_v28_to_v27_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client: &mut postgres::Client| {
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            let current_version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map(|row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_v28_rollback_source(current_version)?;
            tx.batch_execute(
                "LOCK TABLE harness_evolution_eval_receipts, harness_evolution_pareto_archive,
                 harness_evolution_evaluations, harness_evolution_sealed_holdouts
                 IN ACCESS EXCLUSIVE MODE",
            )
            .map_err(|error| error.to_string())?;
            for table in super::super::migrations::V28_TABLES {
                let occupied = tx
                    .query_one(&format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)"), &[])
                    .map(|row| row.get::<_, bool>(0))
                    .map_err(|error| error.to_string())?;
                if occupied {
                    return Err(format!(
                        "v28 rollback blocked: authoritative harness evolution evaluation data exists in {table}"
                    ));
                }
            }
            tx.batch_execute(
                "DROP TABLE harness_evolution_eval_receipts;
                 DROP TABLE harness_evolution_pareto_archive;
                 DROP TABLE harness_evolution_evaluations;
                 DROP TABLE harness_evolution_sealed_holdouts;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1,$2,'schema.rollback.v28_to_v27','local_product_store',$3)",
                &[
                    &now,
                    &actor,
                    &serde_json::json!({
                        "from_version": super::super::migrations::V28_SCHEMA_VERSION,
                        "to_version": super::super::migrations::V27_SCHEMA_VERSION,
                        "tables": super::super::migrations::V28_TABLES,
                    })
                    .to_string(),
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "DELETE FROM schema_migrations WHERE version=$1",
                &[&super::super::migrations::V28_SCHEMA_VERSION],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    pub(in crate::storage::local_product_store) fn rollback_pg_v27_to_v26_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client: &mut postgres::Client| {
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            let current_version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map(|row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_v27_rollback_source(current_version)?;
            tx.batch_execute(
                "LOCK TABLE harness_evolution_receipts, harness_evolution_candidates,
                 harness_evolution_proposals, harness_evolution_active_identity
                 IN ACCESS EXCLUSIVE MODE",
            )
            .map_err(|error| error.to_string())?;
            for table in super::super::migrations::V27_TABLES {
                let occupied = tx
                    .query_one(&format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)"), &[])
                    .map(|row| row.get::<_, bool>(0))
                    .map_err(|error| error.to_string())?;
                if occupied {
                    return Err(format!(
                        "v27 rollback blocked: authoritative harness evolution data exists in {table}"
                    ));
                }
            }
            tx.batch_execute(
                "DROP TABLE harness_evolution_receipts;
                 DROP TABLE harness_evolution_candidates;
                 DROP TABLE harness_evolution_proposals;
                 DROP TABLE harness_evolution_active_identity;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1,$2,'schema.rollback.v27_to_v26','local_product_store',$3)",
                &[
                    &now,
                    &actor,
                    &serde_json::json!({
                        "from_version": super::super::migrations::V27_SCHEMA_VERSION,
                        "to_version": super::super::migrations::V26_SCHEMA_VERSION,
                        "tables": super::super::migrations::V27_TABLES,
                    })
                    .to_string(),
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "DELETE FROM schema_migrations WHERE version=$1",
                &[&super::super::migrations::V27_SCHEMA_VERSION],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    pub(in crate::storage::local_product_store) fn rollback_pg_v26_to_v25_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client| {
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            let current_version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map(|row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_v26_rollback_source(current_version)?;
            tx.batch_execute(
                "LOCK TABLE recursive_execution_nodes, recursive_execution_trees
                 IN ACCESS EXCLUSIVE MODE",
            )
            .map_err(|error| error.to_string())?;
            for table in super::super::migrations::V26_TABLES {
                let occupied = tx
                    .query_one(&format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)"), &[])
                    .map(|row| row.get::<_, bool>(0))
                    .map_err(|error| error.to_string())?;
                if occupied {
                    return Err(format!(
                        "v26 rollback blocked: authoritative recursive execution data exists in {table}"
                    ));
                }
            }
            tx.batch_execute(
                "DROP TABLE recursive_execution_nodes;
                 DROP TABLE recursive_execution_trees;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1,$2,'schema.rollback.v26_to_v25','local_product_store',$3)",
                &[&now, &actor, &super::super::migrations::v26_rollback_audit_details()],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "DELETE FROM schema_migrations WHERE version=$1",
                &[&super::super::migrations::V26_SCHEMA_VERSION],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    pub(crate) fn run_pg_migrations_internal(&self) -> Result<(), String> {
        self.with_pg_conn(|client: &mut postgres::Client| {
            ensure_schema_migrations_table(client)?;
            let current = current_pg_version(client)?;

            for migration in schema::POSTGRES_MIGRATIONS {
                if migration.version == 25 {
                    apply_pg_v25_migration(client)?;
                    continue;
                }
                if migration.version == 26 {
                    apply_pg_v26_migration(client)?;
                    continue;
                }
                if migration.version == 27 {
                    apply_pg_v27_migration(client)?;
                    continue;
                }
                if migration.version == 28 {
                    apply_pg_v28_migration(client)?;
                    continue;
                }
                if migration.version == 29 {
                    apply_pg_v29_migration(client)?;
                    continue;
                }
                if migration.version == 30 {
                    apply_pg_v30_migration(client)?;
                    continue;
                }
                if migration.version == 31 {
                    apply_pg_v31_migration(client)?;
                    continue;
                }
                if migration.version == 32 {
                    apply_pg_v32_migration(client)?;
                    continue;
                }
                if migration.version == 33 {
                    apply_pg_v33_migration(client)?;
                    continue;
                }
                if migration.version == 34 {
                    apply_pg_v34_migration(client)?;
                    continue;
                }
                if migration.version == 35 {
                    apply_pg_v35_migration(client)?;
                    continue;
                }
                if migration.version == 36 {
                    apply_pg_v36_migration(client)?;
                    continue;
                }
                if migration.version == 37 {
                    apply_pg_v37_migration(client)?;
                    continue;
                }
                if migration.version == 38 {
                    apply_pg_v38_migration(client)?;
                    continue;
                }
                if migration.version <= current {
                    continue;
                }
                let sql = match migration.version {
                    1..=9 | 11..=19 => {
                        // PG DDL already includes all tables/columns for these versions.
                        // Record as applied with no-op.
                        ""
                    }
                    10 => {
                        // Policy signal columns on orchestration_decisions.
                        // DDL includes them, but guard against pre-existing PG databases.
                        let has_col = pg_column_exists(client, "orchestration_decisions", "quality_signal_json")?;
                        if has_col {
                            ""
                        } else {
                            "ALTER TABLE orchestration_decisions ADD COLUMN quality_signal_json TEXT;
                             ALTER TABLE orchestration_decisions ADD COLUMN routing_signal_json TEXT;
                             ALTER TABLE orchestration_decisions ADD COLUMN cost_signal_json TEXT;
                             ALTER TABLE orchestration_decisions ADD COLUMN approval_signal_json TEXT;
                             ALTER TABLE orchestration_decisions ADD COLUMN queue_signal_json TEXT;
                             ALTER TABLE orchestration_decisions ADD COLUMN executor_pool_signal_json TEXT;
                             ALTER TABLE orchestration_decisions ADD COLUMN candidate_executors_json TEXT;
                             ALTER TABLE orchestration_decisions ADD COLUMN degraded_reason TEXT;"
                        }
                    }
                    20 => {
                        "CREATE TABLE IF NOT EXISTS offline_replay_artifacts (
                             artifact_sequence BIGSERIAL PRIMARY KEY,
                             artifact_id TEXT NOT NULL UNIQUE,
                             report_schema_version TEXT NOT NULL,
                             status TEXT NOT NULL,
                             eligibility_content_sha256 TEXT NOT NULL,
                             content_sha256 TEXT NOT NULL,
                             created_at TEXT NOT NULL,
                             artifact_json TEXT NOT NULL
                         );
                         CREATE INDEX IF NOT EXISTS idx_offline_replay_artifacts_status
                             ON offline_replay_artifacts(status, artifact_sequence);
                         CREATE INDEX IF NOT EXISTS idx_offline_replay_artifacts_created
                             ON offline_replay_artifacts(created_at);"
                    }
                    21 => {
                        "ALTER TABLE dispatch_history
                         ADD COLUMN IF NOT EXISTS trace_schema_version TEXT;
                         ALTER TABLE dispatch_history
                         ADD COLUMN IF NOT EXISTS trace_content_sha256 TEXT;"
                    }
                    22 => {
                        "CREATE TABLE IF NOT EXISTS agent_action_receipts (
                             run_id TEXT NOT NULL,
                             node_id TEXT NOT NULL,
                             agent_id TEXT NOT NULL,
                             action_sha256 TEXT NOT NULL CHECK (length(action_sha256) = 64),
                             action_type TEXT NOT NULL,
                             result_json TEXT NOT NULL,
                             created_at TEXT NOT NULL,
                             PRIMARY KEY (run_id, node_id)
                         );
                         CREATE INDEX IF NOT EXISTS idx_agent_action_receipts_agent
                             ON agent_action_receipts(agent_id, run_id);
                         CREATE TABLE IF NOT EXISTS tool_allowlist_profiles (
                             profile_id TEXT PRIMARY KEY,
                             configured_at TEXT NOT NULL
                         );
                         INSERT INTO tool_allowlist_profiles (profile_id, configured_at)
                             SELECT profile_id, COALESCE(MIN(created_at), 'migration-v22')
                             FROM tool_allowlists GROUP BY profile_id
                             ON CONFLICT(profile_id) DO NOTHING;
                         CREATE TABLE IF NOT EXISTS tool_execution_authorizations (
                             run_id TEXT NOT NULL,
                             node_id TEXT NOT NULL,
                             action_sha256 TEXT NOT NULL CHECK (length(action_sha256) = 64),
                             tool_name TEXT NOT NULL,
                             profile_id TEXT NOT NULL,
                             status TEXT NOT NULL CHECK (status IN ('requested', 'approved', 'rejected', 'consumed')),
                             requested_approval_id TEXT NOT NULL UNIQUE,
                             resolved_by TEXT,
                             created_at TEXT NOT NULL,
                             updated_at TEXT NOT NULL,
                             PRIMARY KEY (run_id, node_id)
                         );
                         CREATE INDEX IF NOT EXISTS idx_tool_execution_authorizations_status
                             ON tool_execution_authorizations(status, run_id);"
                    }
                    23 => schema::V23_DDL,
                    24 => schema::V24_DDL,
                    _ => {
                        return Err(format!(
                            "unknown pg migration version: {}",
                            migration.version
                        ))
                    }
                };

                if sql.is_empty() {
                    // No-op migration: just record the version.
                    client
                        .execute(
                            "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT DO NOTHING",
                            &[&migration.version],
                        )
                        .map_err(|e| format!("failed to record no-op migration {}: {e}", migration.version))?;
                } else {
                    apply_pg_migration(client, migration.version, sql)?;
                }
            }

            if super::super::schema::CURRENT_POSTGRES_SCHEMA_VERSION == 38 {
                validate_pg_v38_schema(client)?;
            } else {
                validate_pg_v37_schema(client)?;
            }

            // Seed the scheduler_heartbeat singleton row.
            client
                .batch_execute(
                    "INSERT INTO scheduler_heartbeat (id, last_heartbeat_at, tick_count, error_count, uptime_seconds, metadata_json, updated_at)
                     VALUES (1, '', 0, 0, 0.0, '{}', '')
                     ON CONFLICT (id) DO NOTHING;",
                )
                .map_err(|e| format!("failed to seed scheduler_heartbeat: {e}"))?;

            Ok(())
        })
    }

    pub(in crate::storage::local_product_store) fn rollback_pg_v22_to_v21_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client| {
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            tx.batch_execute(
                "LOCK TABLE schema_migrations IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE agent_action_receipts IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE tool_allowlist_profiles IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE tool_execution_authorizations IN ACCESS EXCLUSIVE MODE;",
            )
            .map_err(|error| error.to_string())?;
            let current_version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map(|row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_v22_rollback_source(current_version)?;

            let mut occupied = Vec::new();
            for table in super::super::migrations::V22_TABLES {
                let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)");
                let contains_rows = tx
                    .query_one(&sql, &[])
                    .map(|row| row.get::<_, bool>(0))
                    .map_err(|error| error.to_string())?;
                if contains_rows {
                    occupied.push(table.to_string());
                }
            }
            super::super::migrations::require_empty_v22_tables(&occupied)?;

            tx.batch_execute(
                "DROP INDEX IF EXISTS idx_agent_action_receipts_agent;
                 DROP INDEX IF EXISTS idx_tool_execution_authorizations_status;
                 DROP TABLE agent_action_receipts;
                 DROP TABLE tool_execution_authorizations;
                 DROP TABLE tool_allowlist_profiles;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1, $2, 'schema.rollback.v22_to_v21', 'local_product_store', $3)",
                &[
                    &now,
                    &actor,
                    &super::super::migrations::v22_rollback_audit_details(),
                ],
            )
            .map_err(|error| match error.as_db_error() {
                Some(db_error) => {
                    format!(
                        "failed to record v22 rollback audit: {}",
                        db_error.message()
                    )
                }
                None => format!("failed to record v22 rollback audit: {error}"),
            })?;
            let removed = tx
                .execute(
                    "DELETE FROM schema_migrations WHERE version = $1",
                    &[&super::super::migrations::V22_SCHEMA_VERSION],
                )
                .map_err(|error| error.to_string())?;
            if removed != 1 {
                return Err(format!(
                    "v22 rollback expected one version marker, removed {removed}"
                ));
            }
            tx.commit().map_err(|error| error.to_string())
        })
    }

    pub(in crate::storage::local_product_store) fn rollback_pg_v23_to_v22_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client| {
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            tx.batch_execute(
                "LOCK TABLE schema_migrations IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE durable_memory_versions IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE memory_retrieval_events IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE production_jobs IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE normalized_usage_observations IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE replay_producer_bindings IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE operator_acknowledgements IN ACCESS EXCLUSIVE MODE;",
            )
            .map_err(|error| error.to_string())?;
            let current_version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map(|row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_v23_rollback_source(current_version)?;

            let mut occupied = Vec::new();
            for table in super::super::migrations::V23_TABLES {
                let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)");
                if tx
                    .query_one(&sql, &[])
                    .map(|row| row.get::<_, bool>(0))
                    .map_err(|error| error.to_string())?
                {
                    occupied.push(table.to_string());
                }
            }
            super::super::migrations::require_empty_v23_tables(&occupied)?;

            tx.batch_execute(
                "DROP TABLE operator_acknowledgements;
                 DROP TABLE replay_producer_bindings;
                 DROP TABLE normalized_usage_observations;
                 DROP TABLE production_jobs;
                 DROP TABLE memory_retrieval_events;
                 DROP TABLE durable_memory_versions;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1, $2, 'schema.rollback.v23_to_v22', 'local_product_store', $3)",
                &[
                    &now,
                    &actor,
                    &super::super::migrations::v23_rollback_audit_details(),
                ],
            )
            .map_err(|error| error.to_string())?;
            let removed = tx
                .execute(
                    "DELETE FROM schema_migrations WHERE version = $1",
                    &[&super::super::migrations::V23_SCHEMA_VERSION],
                )
                .map_err(|error| error.to_string())?;
            if removed != 1 {
                return Err(format!(
                    "v23 rollback expected one version marker, removed {removed}"
                ));
            }
            tx.commit().map_err(|error| error.to_string())
        })
    }

    pub(in crate::storage::local_product_store) fn rollback_pg_v25_to_v24_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client| {
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            tx.batch_execute(
                "LOCK TABLE schema_migrations IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE durable_memory_versions IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE provider_embedding_operations IN ACCESS EXCLUSIVE MODE;",
            )
            .map_err(|error| error.to_string())?;
            let current_version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map(|row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_v25_rollback_source(current_version)?;
            let occupied_bindings = tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM durable_memory_versions
                     WHERE embedding_metadata_json IS NOT NULL
                        OR embedding_binding_sha256 IS NOT NULL LIMIT 1)",
                    &[],
                )
                .map(|row| row.get::<_, bool>(0))
                .map_err(|error| error.to_string())?;
            let occupied_operations = tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM provider_embedding_operations LIMIT 1)",
                    &[],
                )
                .map(|row| row.get::<_, bool>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_empty_v25_bindings(
                occupied_bindings || occupied_operations,
            )?;
            tx.batch_execute(
                "DROP TABLE provider_embedding_operations;
                 ALTER TABLE durable_memory_versions DROP COLUMN embedding_binding_sha256;
                 ALTER TABLE durable_memory_versions DROP COLUMN embedding_metadata_json;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1, $2, 'schema.rollback.v25_to_v24', 'local_product_store', $3)",
                &[
                    &now,
                    &actor,
                    &super::super::migrations::v25_rollback_audit_details(),
                ],
            )
            .map_err(|error| error.to_string())?;
            let removed = tx
                .execute(
                    "DELETE FROM schema_migrations WHERE version = $1",
                    &[&super::super::migrations::V25_SCHEMA_VERSION],
                )
                .map_err(|error| error.to_string())?;
            if removed != 1 {
                return Err(format!(
                    "v25 rollback expected one version marker, removed {removed}"
                ));
            }
            tx.commit().map_err(|error| error.to_string())
        })
    }

    pub(in crate::storage::local_product_store) fn rollback_pg_v24_to_v23_internal(
        &self,
        actor: &str,
        now: &str,
    ) -> Result<(), String> {
        self.with_pg_conn(|client| {
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            tx.batch_execute(
                "LOCK TABLE schema_migrations IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE external_runtime_checkpoints IN ACCESS EXCLUSIVE MODE;
                 LOCK TABLE external_runtime_invocations IN ACCESS EXCLUSIVE MODE;",
            )
            .map_err(|error| error.to_string())?;
            let current_version = tx
                .query_one(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    &[],
                )
                .map(|row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_v24_rollback_source(current_version)?;
            let mut occupied = Vec::new();
            for table in super::super::migrations::V24_TABLES {
                let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)");
                if tx
                    .query_one(&sql, &[])
                    .map(|row| row.get::<_, bool>(0))
                    .map_err(|error| error.to_string())?
                {
                    occupied.push(table.to_string());
                }
            }
            super::super::migrations::require_empty_v24_tables(&occupied)?;
            tx.batch_execute(
                "DROP TABLE external_runtime_invocations;
                 DROP TABLE external_runtime_checkpoints;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES ($1, $2, 'schema.rollback.v24_to_v23', 'local_product_store', $3)",
                &[
                    &now,
                    &actor,
                    &super::super::migrations::v24_rollback_audit_details(),
                ],
            )
            .map_err(|error| error.to_string())?;
            let removed = tx
                .execute(
                    "DELETE FROM schema_migrations WHERE version = $1",
                    &[&super::super::migrations::V24_SCHEMA_VERSION],
                )
                .map_err(|error| error.to_string())?;
            if removed != 1 {
                return Err(format!(
                    "v24 rollback expected one version marker, removed {removed}"
                ));
            }
            tx.commit().map_err(|error| error.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "pg-tests")]
    use crate::storage::local_product_store::{
        managed_acceptance,
        migrations::{V36_API_KEY_TENANT_COLUMN, V36_DELEGATED_PLAN_OWNER_COLUMN},
        DatabaseConnection,
    };
    #[cfg(feature = "pg-tests")]
    use postgres::NoTls;
    #[cfg(feature = "pg-tests")]
    use r2d2::Pool;
    #[cfg(feature = "pg-tests")]
    use r2d2_postgres::PostgresConnectionManager;
    #[cfg(feature = "pg-tests")]
    use std::path::PathBuf;

    #[test]
    fn pg_migration_list_is_sorted_and_contiguous() {
        for (i, m) in schema::POSTGRES_MIGRATIONS.iter().enumerate() {
            assert_eq!(
                m.version,
                (i + 1) as i64,
                "migration version mismatch at index {i}"
            );
        }
    }

    #[test]
    fn current_pg_version_constant_matches_list() {
        assert_eq!(
            CURRENT_PG_VERSION,
            schema::POSTGRES_MIGRATIONS.last().unwrap().version
        );
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn pg_v38_migration_empty_rollback_reapply_and_occupied_refusal() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        assert_eq!(fixture.store.schema_version().unwrap(), 38);
        fixture
            .store
            .rollback_pg_v38_to_v37("migration-test", "2026-08-24T00:00:00Z")
            .expect("empty v38 table can roll back");
        assert_eq!(fixture.store.schema_version().unwrap(), 37);
        fixture
            .store
            .run_pg_migrations_internal()
            .expect("v38 migration reapplies after empty rollback");
        assert_eq!(fixture.store.schema_version().unwrap(), 38);
        fixture
            .store
            .with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO harness_evolution_ec3_lifecycle_cost_records
                         (record_id, observation_key, contract_id, candidate_id, attempt_id,
                          phase, dimension, trust_source, terminal_class, source_schema_version,
                          source_digest, redacted_body_json, record_sha256, created_at)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
                        &[
                            &"pg-rollback-record",
                            &"pg-rollback-observation",
                            &"pg-contract",
                            &"pg-candidate",
                            &"pg-attempt",
                            &"evaluation",
                            &"model_tokens",
                            &"measured_direct",
                            &"completed",
                            &"execution_usage.v1",
                            &"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                            &"{}",
                            &"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                            &"2026-08-24T00:00:00Z",
                        ],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        let error = fixture
            .store
            .rollback_pg_v38_to_v37("migration-test", "2026-08-24T00:00:00Z")
            .unwrap_err();
        assert!(error.contains("observations"), "{error}");
        assert_eq!(fixture.store.schema_version().unwrap(), 38);
    }

    #[cfg(feature = "pg-tests")]
    struct PgSchemaCleanup {
        admin: postgres::Client,
        schema_name: String,
    }

    #[cfg(feature = "pg-tests")]
    impl Drop for PgSchemaCleanup {
        fn drop(&mut self) {
            let sql = format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema_name);
            let _ = self.admin.batch_execute(&sql);
        }
    }

    #[cfg(feature = "pg-tests")]
    struct IsolatedPgStore {
        store: LocalProductStore,
        _cleanup: PgSchemaCleanup,
    }

    #[cfg(feature = "pg-tests")]
    impl IsolatedPgStore {
        fn from_environment() -> Option<Self> {
            let url = match std::env::var("ACP_TEST_DATABASE_URL") {
                Ok(url) => url,
                Err(_) => {
                    eprintln!("ACP_TEST_DATABASE_URL not set; skipping PG v22 rollback test");
                    return None;
                }
            };
            let schema_name = format!(
                "acp_v22_{}",
                uuid::Uuid::new_v4().simple().to_string().to_lowercase()
            );
            let mut admin = postgres::Client::connect(&url, NoTls)
                .expect("connect to PostgreSQL test database");
            admin
                .batch_execute(&format!("CREATE SCHEMA {schema_name}"))
                .expect("create isolated PostgreSQL test schema");
            let cleanup = PgSchemaCleanup {
                admin,
                schema_name: schema_name.clone(),
            };

            let mut config: postgres::Config = url.parse().expect("parse PostgreSQL test URL");
            config.options(&format!("-c search_path={schema_name}"));
            let manager = PostgresConnectionManager::new(config, NoTls);
            // Concurrent plan-prepare and migration races need more than two
            // clients; undersizing the pool turns those tests into deadlocks
            // that surface as opaque "db error" timeouts.
            let pool = Pool::builder()
                .max_size(8)
                .build(manager)
                .expect("create isolated PostgreSQL pool");
            {
                let mut client = pool.get().expect("get isolated PostgreSQL connection");
                client
                    .batch_execute(schema::ddl_for(schema::Dialect::Postgres))
                    .expect("create isolated PostgreSQL schema");
            }
            let store = LocalProductStore {
                db_path: PathBuf::from(format!("postgres-schema:{schema_name}")),
                db: DatabaseConnection::Pg(pool),
                clock: Box::new(|| "2026-07-14T00:00:00Z".to_string()),
                encryption_active: false,
                embedding_client: crate::provider::embedding::ProviderEmbeddingClient::default(),
            };
            store
                .run_pg_migrations_internal()
                .expect("apply migrations in isolated PostgreSQL schema");
            Some(Self {
                store,
                _cleanup: cleanup,
            })
        }

        /// Build an explicit pre-v37 fixture for rollback tests. Production
        /// never downgrades v38 implicitly; the v36 rollback contract is
        /// exercised only after removing the additive v38/v37 state.
        fn remove_v37_state(&self) {
            self.store
                .with_pg_conn(|client| {
                    client
                        .batch_execute(
                            "DROP TABLE IF EXISTS harness_evolution_ec3_lifecycle_cost_records;
                             DROP TABLE IF EXISTS harness_evolution_ec2_prediction_outcomes;
                             DELETE FROM schema_migrations WHERE version IN (38, 37);",
                        )
                        .map_err(|error| error.to_string())
                })
                .expect("remove v37 state for rollback fixture");
            assert_eq!(self.store.schema_version().unwrap(), 36);
            self.store
                .with_pg_conn(validate_pg_v36_schema)
                .expect("the explicit v36 PostgreSQL fixture must validate");
        }
    }

    #[cfg(feature = "pg-tests")]
    fn pg_table_exists(store: &LocalProductStore, table: &str) -> bool {
        store
            .with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT EXISTS(
                             SELECT 1 FROM information_schema.tables
                             WHERE table_schema = current_schema() AND table_name = $1
                         )",
                        &[&table],
                    )
                    .map(|row| row.get(0))
                    .map_err(|error| error.to_string())
            })
            .unwrap()
    }

    #[cfg(feature = "pg-tests")]
    fn prepare_v23_rollback_fixture(store: &LocalProductStore) {
        prepare_v25_rollback_fixture(store);
        store
            .rollback_v25_to_v24("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 24);
        store
            .rollback_v24_to_v23("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 23);
    }

    #[cfg(feature = "pg-tests")]
    fn prepare_v25_rollback_fixture(store: &LocalProductStore) {
        // This helper exercises the pre-v37 rollback chain. Keep the fixture
        // explicit now that normal PostgreSQL startup migrates to v38.
        store
            .with_pg_conn(|client| {
                client
                    .batch_execute(
                        "DROP TABLE IF EXISTS harness_evolution_ec3_lifecycle_cost_records;
                         DROP TABLE IF EXISTS harness_evolution_ec2_prediction_outcomes;
                         DELETE FROM schema_migrations WHERE version IN (38, 37);",
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 36);
        store
            .rollback_v36_to_v35("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 35);
        store
            .rollback_v35_to_v34("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 34);
        store
            .rollback_v34_to_v33("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 33);
        store
            .rollback_v33_to_v32("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 32);
        store
            .rollback_v32_to_v31("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 31);
        store
            .rollback_v31_to_v30("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 30);
        store
            .rollback_v30_to_v29("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 29);
        store
            .rollback_v29_to_v28("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 28);
        store
            .rollback_v28_to_v27("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 27);
        store
            .rollback_v27_to_v26("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 26);
        store
            .rollback_v26_to_v25("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 25);
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v35_rollback_records_the_sqlite_parity_audit_shape() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        fixture.remove_v37_state();
        let store = &fixture.store;
        store
            .rollback_v36_to_v35("migration-test", true)
            .expect("empty v36 delegation table must roll back");
        assert_eq!(store.schema_version().unwrap(), 35);
        store
            .rollback_v35_to_v34("migration-test", true)
            .expect("empty v35 receipt table must roll back");
        assert_eq!(store.schema_version().unwrap(), 34);
        assert!(
            !pg_table_exists(store, "product_task_workspace_preparations"),
            "v35 receipt table must be removed only after the drain check"
        );
        let rollback_audit = store
            .with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT actor, resource, details_json FROM audit_log
                         WHERE action = 'schema.rollback.v35_to_v34'
                         ORDER BY audit_id DESC LIMIT 1",
                        &[],
                    )
                    .map(|row| {
                        (
                            row.get::<_, String>(0),
                            row.get::<_, String>(1),
                            row.get::<_, String>(2),
                        )
                    })
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(rollback_audit.0, "migration-test");
        assert_eq!(rollback_audit.1, "local_product_store");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rollback_audit.2).unwrap(),
            serde_json::json!({
                "from_version": 35,
                "to_version": 34,
                "tables": ["product_task_workspace_preparations"],
            })
        );
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v35_and_v36_validate_their_own_versions_with_shared_structure() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        fixture.remove_v37_state();
        let store = &fixture.store;

        store
            .with_pg_conn(validate_pg_v36_schema)
            .expect("a fully migrated v36 schema must validate");

        store
            .rollback_v36_to_v35("migration-test", true)
            .expect("empty v36 delegation table must roll back");
        store
            .with_pg_conn(validate_pg_v35_schema)
            .expect("the rolled-back v35 schema must validate");
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v36_rollback_archives_terminal_delegation_evidence_and_validates_v35() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        fixture.remove_v37_state();
        let store = &fixture.store;
        let body = serde_json::json!({
            "schema_version": "managed_delegation_contract.v1",
            "sensitive_fixture": "must-not-survive-pg-v36-rollback"
        });
        let mut proposal = serde_json::json!({
            "schema_version": "managed_proposal_manifest.v1",
            "target_repository": "Igzela/alters-lab",
            "target_main_sha": "a".repeat(40),
            "mutable_paths": ["docs/USER_GUIDE.md"]
        });
        proposal["manifest_sha256"] = serde_json::json!(
            managed_acceptance::compute_attempt_manifest_sha256(&proposal).unwrap()
        );
        let mut manifest = serde_json::json!({
            "schema_version": "managed_final_execution_manifest.v1",
            "target": {
                "repository": "Igzela/alters-lab",
                "main_sha": "a".repeat(40),
                "mutable_paths": ["docs/USER_GUIDE.md"]
            },
            "execution": {"product_task_id": "task-terminal"},
            "limits": {"max_cost_usd": 0.5}
        });
        manifest["manifest_sha256"] = serde_json::json!(
            managed_acceptance::compute_attempt_manifest_sha256(&manifest).unwrap()
        );
        let mut approval = serde_json::json!({});
        let approval_sha = managed_acceptance::sha256_hex(
            managed_acceptance::canonical_json(&approval)
                .unwrap()
                .as_bytes(),
        );
        approval["approval_receipt_sha256"] = serde_json::json!(approval_sha);
        let spend = serde_json::json!({});
        let spend_sha = managed_acceptance::sha256_hex(
            managed_acceptance::canonical_json(&spend)
                .unwrap()
                .as_bytes(),
        );
        let mut artifact = serde_json::json!({});
        let artifact_sha = managed_acceptance::sha256_hex(
            managed_acceptance::canonical_json(&artifact)
                .unwrap()
                .as_bytes(),
        );
        artifact["artifact_confirmation_sha256"] = serde_json::json!(artifact_sha);
        let terminal = serde_json::json!({
            "terminal_class": "succeeded",
            "delegation_state": "expired",
            "spend_authorization_state": "expired",
            "attempt_lease_state": "closed",
            "realized_cost_usd": 0.125,
            "rollback_evidence": {
                "workspace_status": "cleaned",
                "target_main_write": false,
                "sensitive_fixture": "must-not-survive-pg-v36-rollback"
            }
        });
        let body_sha = managed_acceptance::sha256_hex(
            managed_acceptance::canonical_json(&body)
                .unwrap()
                .as_bytes(),
        );
        let proposal_sha = proposal["manifest_sha256"].as_str().unwrap().to_string();
        let values = [
            body_sha,
            body.to_string(),
            proposal_sha,
            proposal.to_string(),
            approval_sha,
            approval.to_string(),
            spend_sha,
            spend.to_string(),
            manifest.to_string(),
            artifact_sha,
            artifact.to_string(),
            serde_json::json!([{"status":"succeeded","sensitive_fixture":"must-not-survive-pg-v36-rollback"}]).to_string(),
            terminal.to_string(),
        ];
        let parameters = values
            .iter()
            .map(|value| value as &(dyn postgres::types::ToSql + Sync))
            .collect::<Vec<_>>();
        store
            .with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO managed_acceptance_delegations (
                            delegation_id, tenant_id, product_task_id, principal_kind, principal_id,
                            manifest_approver_id, artifact_confirmer_id, attempt_activator_id,
                            delegation_sha256, body_json, proposal_sha256, proposal_json,
                            status, executions_allowed, executions_used, max_total_cost_usd,
                            total_cost_usd, spend_authorization_id, manifest_approval_sha256,
                            manifest_approval_json, spend_body_sha256, spend_status,
                            spend_body_json, manifest_json, attempt_id, attempt_lease_id,
                            attempt_lease_token, attempt_status, artifact_confirmation_sha256,
                            artifact_confirmation_json, provider_request_journal_json,
                            terminal_receipt_json, created_at, updated_at, expires_at,
                            terminal_at, revoked_at
                         ) VALUES (
                            'delegation-terminal', 'tenant-sensitive', 'task-terminal', 'operator_api_key',
                            'principal-sensitive', 'approver-sensitive', 'confirmer-sensitive',
                            'activator-sensitive', $1,$2,$3,$4,'expired',1,1,0.5,0.125,
                            'spend-sensitive',$5,$6,$7,'expired',$8,$9,
                            'attempt-sensitive','lease-sensitive','token-sensitive','closed',
                            $10,$11,$12,$13,'2026-07-31T00:00:00Z',
                            '2026-07-31T00:02:00Z','2026-08-01T00:00:00Z',
                            '2026-07-31T00:02:00Z',NULL
                         )",
                        &parameters,
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();

        store
            .rollback_v36_to_v35("migration-test", true)
            .expect("terminal PG delegation evidence must be archived");
        assert_eq!(store.schema_version().unwrap(), 35);
        store
            .with_pg_conn(validate_pg_v35_schema)
            .expect("archived rollback must leave a valid v35 schema");
        assert!(!pg_table_exists(store, "managed_acceptance_delegations"));
        let owner_column_exists = store
            .with_pg_conn(|client| {
                pg_column_exists(client, "workflow_plans", V36_DELEGATED_PLAN_OWNER_COLUMN)
            })
            .unwrap();
        assert!(!owner_column_exists);
        let key_tenant_column_exists = store
            .with_pg_conn(|client| {
                pg_column_exists(client, "api_key_metadata", V36_API_KEY_TENANT_COLUMN)
            })
            .unwrap();
        assert!(!key_tenant_column_exists);
        let archive: Value = store
            .with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT details_json FROM audit_log
                         WHERE action='schema.rollback.v36_delegation_archived'
                         ORDER BY audit_id DESC LIMIT 1",
                        &[],
                    )
                    .map(|row| serde_json::from_str::<Value>(&row.get::<_, String>(0)).unwrap())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        let encoded = archive.to_string();
        assert!(!encoded.contains("must-not-survive-pg-v36-rollback"));
        assert!(!encoded.contains("principal-sensitive"));
        assert!(!encoded.contains("token-sensitive"));
        assert_eq!(
            archive["source_evidence"]["product_task_id"],
            "task-terminal"
        );
        let mut unhashed = archive.clone();
        let archive_sha = unhashed
            .as_object_mut()
            .unwrap()
            .remove("archive_sha256")
            .unwrap();
        assert_eq!(
            archive_sha,
            managed_acceptance::sha256_hex(
                managed_acceptance::canonical_json(&unhashed)
                    .unwrap()
                    .as_bytes()
            )
        );
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_concurrent_delegated_plan_prepare_has_one_execution_identity() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let IsolatedPgStore { store, _cleanup } = fixture;
        let store = std::sync::Arc::new(store);
        store
            .with_pg_conn(|client| {
                // product_tasks stores approval flags as INTEGER (0/1), matching
                // the production admit path — not PostgreSQL BOOLEAN TRUE.
                let one: i32 = 1;
                client
                    .execute(
                        "INSERT INTO product_tasks (
                            task_id, schema_version, tenant_id, workspace_id, idempotency_key,
                            status, version, objective_fingerprint, target_id, target_repo_path,
                            source_revision, source_tree_hash, output_intent, risk_class,
                            approval_required, confirm_execution, confirm_output,
                            intake_contract_sha256, intake_json, workspace_binding_json,
                            plan_id, run_id, workspace_record_id, failure_code, failure_detail,
                            created_at, updated_at, created_by
                         ) VALUES (
                            'task-concurrent','product_task.v1','tenant-a','workspace-a',
                            'delegated-plan-concurrency','workspace_bound',1,$1,
                            'target-a','/redacted/target',$2,NULL,'draft_pr','low',
                            $4,$4,$4,$3,'{}','{}',NULL,NULL,NULL,NULL,NULL,
                            '2026-07-31T00:00:00Z','2026-07-31T00:00:00Z','test'
                         )",
                        &[&"a".repeat(64), &"b".repeat(40), &"c".repeat(64), &one],
                    )
                    .map(|_| ())
                    .map_err(|error| format!("product_tasks seed failed: {error}"))
            })
            .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let owner_id = "d".repeat(64);
        let plans = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for index in 0..4 {
                let store = std::sync::Arc::clone(&store);
                let barrier = std::sync::Arc::clone(&barrier);
                let owner_id = owner_id.clone();
                handles.push(scope.spawn(move || {
                    barrier.wait();
                    store.create_or_recover_delegated_workflow_plan(
                        "task-concurrent",
                        &owner_id,
                        "bounded delegated task",
                        &format!("pg-concurrent-{index}"),
                        |ids, created_at| {
                            Ok(serde_json::json!({
                                "schema_version": "read_only_plan.v1",
                                "status": "planned_executable",
                                "graph": {
                                    "nodes": [],
                                    "edges": [],
                                    "workflow_id": ids.workflow_id,
                                    "dispatch_id": ids.dispatch_id
                                },
                                "analysis": {},
                                "advisory": {
                                    "product_task_id": "task-concurrent",
                                    "delegated_plan_owner_id": owner_id
                                },
                                "boundaries": {
                                    "execution_authority": "delegated_product_golden_path"
                                },
                                "created_at": created_at
                            }))
                        },
                    )
                }));
            }
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .expect("pg concurrent plan worker panicked")
                        .unwrap_or_else(|error| {
                            panic!("pg concurrent delegated plan prepare failed: {error}")
                        })
                })
                .collect::<Vec<_>>()
        });
        let plan_id = plans[0]["plan_id"].as_str().unwrap();
        assert!(plans
            .iter()
            .all(|plan| plan["plan_id"].as_str() == Some(plan_id)));
        assert_eq!(
            store.list_workflow_plans_with_offset(100, 0).unwrap().len(),
            1
        );
        let replay = store
            .create_or_recover_delegated_workflow_plan(
                "task-concurrent",
                &owner_id,
                "bounded delegated task",
                "pg-restart-connection",
                |_, _| Err("replay must recover without rebuilding".into()),
            )
            .unwrap();
        assert_eq!(replay["plan_id"], plan_id);
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v35_rollback_waits_for_the_receipt_schema_publisher() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        fixture.remove_v37_state();
        let store = &fixture.store;
        store
            .rollback_v36_to_v35("migration-test", true)
            .expect("empty v36 delegation table must roll back");
        store
            .with_pg_conn(|client| {
                let mut publisher = client.transaction().map_err(|error| error.to_string())?;
                publisher
                    .query_one(
                        "SELECT pg_advisory_xact_lock(
                             hashtext(current_database()), hashtext(current_schema())
                         )",
                        &[],
                    )
                    .map_err(|error| error.to_string())?;

                std::thread::scope(|scope| -> Result<(), String> {
                    let (result_sender, result_receiver) = std::sync::mpsc::channel();
                    scope.spawn(move || {
                        let _ =
                            result_sender.send(store.rollback_v35_to_v34("migration-test", true));
                    });
                    assert!(
                        result_receiver
                            .recv_timeout(std::time::Duration::from_millis(200))
                            .is_err(),
                        "v35 rollback must wait for a receipt publisher's schema lock"
                    );
                    publisher.commit().map_err(|error| error.to_string())?;
                    result_receiver
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .map_err(|error| error.to_string())?
                        .map_err(|error| {
                            format!("v35 rollback failed after publisher exit: {error}")
                        })?;
                    Ok(())
                })
            })
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 34);
    }

    #[cfg(feature = "pg-tests")]
    fn prepare_v22_rollback_fixture(store: &LocalProductStore) {
        prepare_v23_rollback_fixture(store);
        store
            .rollback_v23_to_v22("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 22);
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn already_versioned_malformed_pg_v26_schema_is_rejected_fail_closed() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        fixture
            .store
            .with_pg_conn(|client| {
                client
                    .batch_execute(
                        "ALTER TABLE recursive_execution_nodes
                         DROP CONSTRAINT recursive_execution_nodes_pkey;",
                    )
                    .map_err(|error| error.to_string())
            })
            .expect("malform v26 node identity");
        let error = fixture
            .store
            .run_pg_migrations_internal()
            .expect_err("malformed v26 schema must fail closed");
        assert!(
            error.contains("missing primary key for recursive_execution_nodes"),
            "unexpected error: {error}"
        );
        assert_eq!(
            fixture.store.schema_version().expect("version"),
            CURRENT_PG_VERSION
        );
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn already_versioned_pg_v26_rejects_non_nullable_parent_identity() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        fixture
            .store
            .with_pg_conn(|client| {
                client
                    .batch_execute(
                        "ALTER TABLE recursive_execution_nodes
                         ALTER COLUMN parent_node_id SET NOT NULL;",
                    )
                    .map_err(|error| error.to_string())
            })
            .expect("malform v26 parent identity");
        let error = fixture
            .store
            .run_pg_migrations_internal()
            .expect_err("incorrectly non-null parent identity must fail closed");
        assert!(
            error.contains(
                "PostgreSQL v26 schema type or nullability mismatch for recursive_execution_nodes.parent_node_id"
            ),
            "unexpected error: {error}"
        );
        assert_eq!(
            fixture.store.schema_version().expect("version"),
            CURRENT_PG_VERSION
        );
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn already_versioned_pg_v26_rejects_weakened_constraints_and_indexes() {
        let cases = [
            (
                "ALTER TABLE recursive_execution_nodes
                 DROP CONSTRAINT recursive_execution_nodes_depth_check;
                 ALTER TABLE recursive_execution_nodes
                 ADD CONSTRAINT recursive_execution_nodes_depth_check CHECK (depth >= 0);",
                "missing recursive depth constraint",
            ),
            (
                "ALTER TABLE recursive_execution_nodes
                 DROP CONSTRAINT recursive_execution_nodes_objective_fingerprint_check;
                 ALTER TABLE recursive_execution_nodes
                 ADD CONSTRAINT recursive_execution_nodes_objective_fingerprint_check
                 CHECK (length(objective_fingerprint) > 0);",
                "missing objective fingerprint length constraint",
            ),
            (
                "ALTER TABLE recursive_execution_nodes
                 DROP CONSTRAINT recursive_execution_nodes_root_run_id_fkey;
                 ALTER TABLE recursive_execution_nodes
                 ADD CONSTRAINT recursive_execution_nodes_root_run_id_fkey
                 FOREIGN KEY (node_id) REFERENCES recursive_execution_trees(root_run_id);",
                "missing recursive root foreign key",
            ),
            (
                "DROP INDEX idx_recursive_execution_nodes_parent;
                 CREATE INDEX idx_recursive_execution_nodes_parent
                 ON recursive_execution_nodes(status);",
                "missing or malformed index idx_recursive_execution_nodes_parent",
            ),
        ];
        for (ddl, expected) in cases {
            let Some(fixture) = IsolatedPgStore::from_environment() else {
                return;
            };
            fixture
                .store
                .with_pg_conn(|client| client.batch_execute(ddl).map_err(|error| error.to_string()))
                .expect("malform v26 constraint");
            let error = fixture
                .store
                .run_pg_migrations_internal()
                .expect_err("weakened v26 schema must fail closed");
            assert!(error.contains(expected), "unexpected error: {error}");
            assert_eq!(
                fixture.store.schema_version().expect("version"),
                CURRENT_PG_VERSION
            );
        }
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v25_rollback_refuses_provider_bindings_without_moving_marker() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;
        prepare_v25_rollback_fixture(store);
        store
            .with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO durable_memory_versions
                         (memory_id, version, tenant_id, workspace_id, source_id,
                          source_sha256, conflict_key, state, confidence, content_json,
                          embedding_provenance, embedding_metadata_json,
                          embedding_binding_sha256, record_sha256, created_at, created_by)
                         VALUES
                         ('provider-bound', 1, 'tenant', 'workspace', 'source',
                          $1, 'fact', 'current', 1.0, '{}', 'provider_reported', '{}',
                          $2, $3, '2026-07-14T00:00:00Z', 'migration-test')",
                        &[
                            &"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                            &"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                            &"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        ],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();

        let error = store
            .rollback_v25_to_v24("migration-test", true)
            .unwrap_err();
        assert!(error.contains("provider embedding bindings exist"));
        assert_eq!(store.schema_version().unwrap(), 25);
        store
            .with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT embedding_binding_sha256
                         FROM durable_memory_versions
                         WHERE memory_id = 'provider-bound' AND version = 1",
                        &[],
                    )
                    .map(|row| {
                        assert_eq!(
                            row.get::<_, String>(0),
                            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                        );
                    })
                    .map_err(|error| error.to_string())
            })
            .unwrap();
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v25_migration_is_concurrent_restart_safe_with_partial_columns() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;
        prepare_v25_rollback_fixture(store);
        store
            .rollback_v25_to_v24("migration-test-setup", true)
            .unwrap();
        store
            .with_pg_conn(|client| {
                client
                    .batch_execute(
                        "ALTER TABLE durable_memory_versions
                         ADD COLUMN embedding_metadata_json TEXT;",
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 24);

        let barrier = std::sync::Barrier::new(2);
        let (left, right) = std::thread::scope(|scope| {
            let left = scope.spawn(|| {
                barrier.wait();
                store.run_pg_migrations_internal()
            });
            let right = scope.spawn(|| {
                barrier.wait();
                store.run_pg_migrations_internal()
            });
            (left.join().unwrap(), right.join().unwrap())
        });
        left.unwrap();
        right.unwrap();

        assert_eq!(store.schema_version().unwrap(), CURRENT_PG_VERSION);
        store
            .with_pg_conn(|client| {
                assert!(pg_column_exists(
                    client,
                    "durable_memory_versions",
                    "embedding_metadata_json"
                )?);
                assert!(pg_column_exists(
                    client,
                    "durable_memory_versions",
                    "embedding_binding_sha256"
                )?);
                let marker_count = client
                    .query_one(
                        "SELECT COUNT(*) FROM schema_migrations WHERE version = $1",
                        &[&super::super::super::migrations::V25_SCHEMA_VERSION],
                    )
                    .map(|row| row.get::<_, i64>(0))
                    .map_err(|error| error.to_string())?;
                assert_eq!(marker_count, 1);
                Ok(())
            })
            .unwrap();

        store
            .with_pg_conn(|client| {
                client
                    .batch_execute(
                        "DROP INDEX IF EXISTS idx_managed_acceptance_spend_active_logical;
                         ALTER TABLE managed_acceptance_spend_authorizations
                             DROP COLUMN IF EXISTS logical_authorization_sha256 CASCADE;",
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        store.run_pg_migrations_internal().unwrap();
        store.run_pg_migrations_internal().unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_PG_VERSION);

        store
            .with_pg_conn(|client| {
                client
                    .batch_execute(
                        "DELETE FROM schema_migrations WHERE version=25;
                         ALTER TABLE provider_embedding_operations
                             RENAME TO provider_embedding_operations_valid;
                         CREATE TABLE provider_embedding_operations AS
                             SELECT * FROM provider_embedding_operations_valid WHERE FALSE;
                         DROP TABLE provider_embedding_operations_valid;
                         INSERT INTO provider_embedding_operations (operation_id)
                             VALUES ('occupied-malformed');",
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        let occupied_error = store.run_pg_migrations_internal().unwrap_err();
        assert!(
            occupied_error.contains("occupied partial operation table"),
            "unexpected occupied constraint failure: {occupied_error}"
        );
        // The refusal is atomic: deleting only the v25 marker does not move or
        // silently rewrite the existing v26 marker.
        assert_eq!(store.schema_version().unwrap(), CURRENT_PG_VERSION);
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v25_migration_refuses_occupied_weakened_state_constraint() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;
        prepare_v25_rollback_fixture(store);
        store
            .with_pg_conn(|client| {
                client
                    .batch_execute(&format!(
                        r#"INSERT INTO provider_audit_events
                           (event_id,dispatch_id,provider_id,event_type,redaction_status,created_at)
                           VALUES ('paudit-preflight-{hash}','memory-embedding-{hash}','openrouter',
                                   'contract_check_reserved','redacted','2026-07-15T00:00:00Z');
                           INSERT INTO provider_embedding_operations
                           (operation_id,operation_kind,target_memory_id,target_version,tenant_id,workspace_id,
                            source_id,source_sha256,request_identity_sha256,operation_binding_sha256,
                            content_sha256,contract_json,contract_sha256,receipt_sha256,provider_id,
                            requested_model_id,resolved_model_id,dimensions,reservation_event_id,state,
                            attempt_count,created_at,updated_at)
                           VALUES ('embedding-operation-{hash}','memory_version','memory',1,'tenant','workspace',
                                   'source','{hash}','{hash}','{hash}','{hash}','{{}}','{hash}','{hash}',
                                   'openrouter','model:free','model:free',1,'paudit-preflight-{hash}',
                                   'preflight_reserved',1,'2026-07-15T00:00:00Z','2026-07-15T00:00:00Z');
                           DO $do$
                           DECLARE state_constraint TEXT;
                           BEGIN
                             SELECT conname INTO state_constraint
                             FROM pg_constraint
                             WHERE conrelid='provider_embedding_operations'::regclass
                               AND contype='c'
                               AND pg_get_constraintdef(oid) LIKE '%preflight_reserved%';
                             EXECUTE format('ALTER TABLE provider_embedding_operations DROP CONSTRAINT %I',
                                            state_constraint);
                           END $do$;
                           ALTER TABLE provider_embedding_operations
                             ADD CONSTRAINT weakened_state_check CHECK (state IN
                               ('preflight_reserved','reserved','sending','network_succeeded','succeeded',
                                'result_erased','failed_before_send','failed_known_outcome','outcome_unknown',
                                'outcome_unknown_acknowledged','retry_authorized','bogus'));
                           DELETE FROM schema_migrations WHERE version=25;"#,
                        hash = "a".repeat(64),
                    ))
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        let error = store.run_pg_migrations_internal().unwrap_err();
        assert!(error.contains("occupied partial operation table"));
        assert_eq!(store.schema_version().unwrap(), 24);
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v24_rollback_is_atomic_and_can_be_migrated_forward_again() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;
        prepare_v25_rollback_fixture(store);
        store
            .rollback_v25_to_v24("migration-test-setup", true)
            .unwrap();
        store.rollback_v24_to_v23("migration-test", true).unwrap();
        assert_eq!(store.schema_version().unwrap(), 23);
        for table in super::super::super::migrations::V24_TABLES {
            assert!(!pg_table_exists(store, table), "{table} should be removed");
        }
        let rollback_audit = store
            .with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT actor, details_json FROM audit_log
                         WHERE action = 'schema.rollback.v24_to_v23'",
                        &[],
                    )
                    .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(rollback_audit.0, "migration-test");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rollback_audit.1).unwrap(),
            serde_json::json!({
                "from_version":24,
                "to_version":23,
                "dropped_empty_tables":[
                    "external_runtime_checkpoints",
                    "external_runtime_invocations"
                ]
            })
        );
        store.run_pg_migrations_internal().unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_PG_VERSION);
        for table in super::super::super::migrations::V24_TABLES {
            assert!(pg_table_exists(store, table), "{table} should be restored");
        }
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v23_rollback_is_atomic_and_can_be_migrated_forward_again() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;

        prepare_v23_rollback_fixture(store);
        store.rollback_v23_to_v22("migration-test", true).unwrap();
        assert_eq!(store.schema_version().unwrap(), 22);
        for table in super::super::super::migrations::V23_TABLES {
            assert!(!pg_table_exists(store, table), "{table} should be removed");
        }
        let rollback_audit = store
            .with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT actor, details_json FROM audit_log
                         WHERE action = 'schema.rollback.v23_to_v22'",
                        &[],
                    )
                    .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(rollback_audit.0, "migration-test");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rollback_audit.1).unwrap(),
            serde_json::json!({
                "from_version": 23,
                "to_version": 22,
                "dropped_empty_tables": [
                    "durable_memory_versions",
                    "memory_retrieval_events",
                    "production_jobs",
                    "normalized_usage_observations",
                    "replay_producer_bindings",
                    "operator_acknowledgements"
                ]
            })
        );

        store.run_pg_migrations_internal().unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_PG_VERSION);
        for table in super::super::super::migrations::V23_TABLES {
            assert!(pg_table_exists(store, table), "{table} should be restored");
        }
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v23_rollback_refuses_authoritative_rows_without_moving_marker() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;
        prepare_v23_rollback_fixture(store);
        store
            .with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO production_jobs
                         (job_key, job_kind, scope_sha256, input_sha256, state,
                          created_at, updated_at)
                         VALUES ($1, $2, $3, $4, 'completed', $5, $5)",
                        &[
                            &"occupied-job",
                            &"budget_intelligence",
                            &"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                            &"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                            &"2026-07-14T00:00:00Z",
                        ],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();

        let error = store
            .rollback_v23_to_v22("migration-test", true)
            .unwrap_err();
        assert!(error.contains("authoritative v23 data exists"));
        assert!(error.contains("production_jobs"));
        assert_eq!(store.schema_version().unwrap(), 23);
        for table in super::super::super::migrations::V23_TABLES {
            assert!(
                pg_table_exists(store, table),
                "{table} must remain after refusal"
            );
        }
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v22_rollback_is_atomic_and_can_be_migrated_forward_again() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;
        prepare_v22_rollback_fixture(store);
        store
            .with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO tool_allowlists (profile_id, tool_name, created_at)
                         VALUES ($1, $2, $3)",
                        &[&"legacy-locked", &"echo", &"2026-07-14T00:00:00Z"],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();

        store.rollback_v22_to_v21("migration-test", true).unwrap();
        assert_eq!(store.schema_version().unwrap(), 21);
        for table in super::super::super::migrations::V22_TABLES {
            assert!(!pg_table_exists(store, table), "{table} should be removed");
        }
        let rollback_audit = store
            .with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT actor, resource, details_json FROM audit_log
                         WHERE action = 'schema.rollback.v22_to_v21'",
                        &[],
                    )
                    .map(|row| {
                        (
                            row.get::<_, String>(0),
                            row.get::<_, String>(1),
                            row.get::<_, String>(2),
                        )
                    })
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(rollback_audit.0, "migration-test");
        assert_eq!(rollback_audit.1, "local_product_store");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rollback_audit.2).unwrap(),
            serde_json::json!({
                "from_version": 22,
                "to_version": 21,
                "dropped_empty_tables": [
                    "agent_action_receipts",
                    "tool_allowlist_profiles",
                    "tool_execution_authorizations"
                ]
            })
        );

        store.run_pg_migrations_internal().unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_PG_VERSION);
        for table in super::super::super::migrations::V22_TABLES {
            assert!(pg_table_exists(store, table), "{table} should be restored");
        }
        assert_eq!(
            store
                .read_tool_allowlist_policy("legacy-locked")
                .unwrap()
                .unwrap()["value"]["tool_names"],
            serde_json::json!(["echo"])
        );
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v22_rollback_refuses_authoritative_rows_without_moving_marker() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;
        prepare_v22_rollback_fixture(store);
        store
            .configure_tool_allowlist("migration-test", "configured-empty", &[], None)
            .unwrap();
        store
            .with_pg_conn(|client| {
                client
                    .batch_execute(
                        "INSERT INTO agent_action_receipts
                         (run_id, node_id, agent_id, action_sha256, action_type, result_json, created_at)
                         VALUES
                         ('occupied-run', 'occupied-agent-node', 'occupied-agent',
                          'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                          'complete', '{}', '2026-07-14T00:00:00Z');
                         INSERT INTO tool_execution_authorizations
                         (run_id, node_id, action_sha256, tool_name, profile_id, status,
                          requested_approval_id, created_at, updated_at)
                         VALUES
                         ('occupied-run', 'occupied-tool-node',
                          'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                          'echo', 'configured-empty', 'requested', 'occupied-approval',
                          '2026-07-14T00:00:00Z', '2026-07-14T00:00:00Z');",
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();

        let error = store
            .rollback_v22_to_v21("migration-test", true)
            .unwrap_err();
        assert!(error.contains("authoritative v22 data exists"));
        for table in super::super::super::migrations::V22_TABLES {
            assert!(error.contains(table), "refusal must identify {table}");
        }
        assert_eq!(store.schema_version().unwrap(), 22);
        for table in super::super::super::migrations::V22_TABLES {
            assert!(
                pg_table_exists(store, table),
                "{table} must remain after refusal"
            );
        }
        assert!(store
            .read_tool_allowlist_policy("configured-empty")
            .unwrap()
            .is_some());
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v22_rollback_audit_failure_rolls_back_tables_and_version_marker() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;
        prepare_v22_rollback_fixture(store);
        store
            .with_pg_conn(|client| {
                client
                    .batch_execute(
                        "CREATE FUNCTION reject_v22_rollback_audit() RETURNS trigger
                         LANGUAGE plpgsql AS $$
                         BEGIN
                             IF NEW.action = 'schema.rollback.v22_to_v21' THEN
                                 RAISE EXCEPTION 'injected v22 rollback audit failure';
                             END IF;
                             RETURN NEW;
                         END;
                         $$;
                         CREATE TRIGGER reject_v22_rollback_audit
                         BEFORE INSERT ON audit_log
                         FOR EACH ROW EXECUTE FUNCTION reject_v22_rollback_audit();",
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();

        let error = store
            .rollback_v22_to_v21("migration-test", true)
            .unwrap_err();
        assert!(
            error.contains("injected v22 rollback audit failure"),
            "unexpected rollback error: {error}"
        );
        assert_eq!(store.schema_version().unwrap(), 22);
        for table in super::super::super::migrations::V22_TABLES {
            assert!(
                pg_table_exists(store, table),
                "{table} must be restored when rollback audit fails"
            );
        }
        let rollback_audits = store
            .with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT COUNT(*) FROM audit_log
                         WHERE action = 'schema.rollback.v22_to_v21'",
                        &[],
                    )
                    .map(|row| row.get::<_, i64>(0))
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(rollback_audits, 0);
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v22_rollback_waits_for_writer_then_refuses_committed_authority() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;
        prepare_v22_rollback_fixture(store);

        store
            .with_pg_conn(|client| {
                let mut blocker = client.transaction().map_err(|error| error.to_string())?;
                blocker
                    .execute(
                        "INSERT INTO tool_allowlist_profiles (profile_id, configured_at)
                         VALUES ($1, $2)",
                        &[&"concurrent-profile", &"2026-07-14T00:00:00Z"],
                    )
                    .map_err(|error| error.to_string())?;

                std::thread::scope(|scope| -> Result<(), String> {
                    let (result_sender, result_receiver) = std::sync::mpsc::channel();
                    scope.spawn(move || {
                        let _ =
                            result_sender.send(store.rollback_v22_to_v21("migration-test", true));
                    });

                    assert!(
                        result_receiver
                            .recv_timeout(std::time::Duration::from_millis(200))
                            .is_err(),
                        "rollback must not pass the writer's table lock"
                    );
                    blocker.commit().map_err(|error| error.to_string())?;
                    let error = result_receiver
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .map_err(|error| error.to_string())?
                        .expect_err("committed authority must block rollback");
                    assert!(error.contains("authoritative v22 data exists"));
                    assert!(error.contains("tool_allowlist_profiles"));
                    Ok(())
                })
            })
            .unwrap();

        assert_eq!(store.schema_version().unwrap(), 22);
        for table in super::super::super::migrations::V22_TABLES {
            assert!(
                pg_table_exists(store, table),
                "{table} must remain after concurrent-authority refusal"
            );
        }
    }
}
