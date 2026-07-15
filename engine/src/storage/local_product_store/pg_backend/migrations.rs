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
    if current_version < 24 || (current_version > 25 && !marker_exists) {
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
        (false, false) => schema::V25_DDL,
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
    tx.commit()
        .map_err(|error| format!("failed to commit migration 25: {error}"))
}

impl LocalProductStore {
    pub(crate) fn run_pg_migrations_internal(&self) -> Result<(), String> {
        self.with_pg_conn(|client: &mut postgres::Client| {
            ensure_schema_migrations_table(client)?;
            let current = current_pg_version(client)?;

            for migration in schema::POSTGRES_MIGRATIONS {
                if migration.version == 25 {
                    apply_pg_v25_migration(client)?;
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
                 LOCK TABLE durable_memory_versions IN ACCESS EXCLUSIVE MODE;",
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
            let occupied = tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM durable_memory_versions
                     WHERE embedding_metadata_json IS NOT NULL
                        OR embedding_binding_sha256 IS NOT NULL LIMIT 1)",
                    &[],
                )
                .map(|row| row.get::<_, bool>(0))
                .map_err(|error| error.to_string())?;
            super::super::migrations::require_empty_v25_bindings(occupied)?;
            tx.batch_execute(
                "ALTER TABLE durable_memory_versions DROP COLUMN embedding_binding_sha256;
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
        assert_eq!(store.schema_version().unwrap(), 25);
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
    fn prepare_v22_rollback_fixture(store: &LocalProductStore) {
        prepare_v23_rollback_fixture(store);
        store
            .rollback_v23_to_v22("migration-test-setup", true)
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 22);
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v25_rollback_refuses_provider_bindings_without_moving_marker() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;
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

        assert_eq!(store.schema_version().unwrap(), 25);
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
        assert_eq!(store.schema_version().unwrap(), 25);
    }

    #[test]
    #[cfg(feature = "pg-tests")]
    fn pg_v24_rollback_is_atomic_and_can_be_migrated_forward_again() {
        let Some(fixture) = IsolatedPgStore::from_environment() else {
            return;
        };
        let store = &fixture.store;
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
        assert_eq!(store.schema_version().unwrap(), 25);
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
        assert_eq!(store.schema_version().unwrap(), 25);
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
        assert_eq!(store.schema_version().unwrap(), 25);
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
