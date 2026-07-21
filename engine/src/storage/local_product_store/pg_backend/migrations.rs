use super::super::{schema, LocalProductStore};

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

fn validate_pg_v27_schema(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    let version = client
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map_err(|error| error.to_string())?
        .get::<_, i64>(0);
    if version != super::super::migrations::V27_SCHEMA_VERSION {
        return Err(format!("PostgreSQL v27 schema version mismatch: {version}"));
    }
    for table in super::super::migrations::V27_TABLES {
        if !pg_table_present(client, table)? {
            return Err(format!("PostgreSQL v27 schema missing table {table}"));
        }
    }
    // Recursive surface from v26 must remain present.
    validate_pg_v26_tables(client)
}

fn validate_pg_v26_schema(client: &mut impl postgres::GenericClient) -> Result<(), String> {
    let version = client
        .query_one(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            &[],
        )
        .map_err(|error| error.to_string())?
        .get::<_, i64>(0);
    if version != super::super::migrations::V26_SCHEMA_VERSION {
        return Err(format!("PostgreSQL v26 schema version mismatch: {version}"));
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

            validate_pg_v27_schema(client)?;

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
    use crate::storage::local_product_store::DatabaseConnection;
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
            let pool = Pool::builder()
                .max_size(2)
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
        assert_eq!(fixture.store.schema_version().expect("version"), 26);
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
        assert_eq!(fixture.store.schema_version().expect("version"), 26);
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
            assert_eq!(fixture.store.schema_version().expect("version"), 26);
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

        assert_eq!(store.schema_version().unwrap(), 26);
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

        store.run_pg_migrations_internal().unwrap();
        assert_eq!(store.schema_version().unwrap(), 26);

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
        assert_eq!(store.schema_version().unwrap(), 26);
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
        assert_eq!(store.schema_version().unwrap(), 26);
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
        assert_eq!(store.schema_version().unwrap(), 26);
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
        assert_eq!(store.schema_version().unwrap(), 26);
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
