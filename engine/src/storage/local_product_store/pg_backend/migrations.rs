use super::super::LocalProductStore;

pub(super) const CURRENT_PG_VERSION: i64 = 11;

struct PgMigration {
    version: i64,
    description: &'static str,
}

const PG_MIGRATIONS: &[PgMigration] = &[
    PgMigration {
        version: 1,
        description: "add last_used_at and expires_at to api_key_metadata",
    },
    PgMigration {
        version: 2,
        description: "add inert workflow run state tables",
    },
    PgMigration {
        version: 3,
        description: "add supervised patch metadata tables",
    },
    PgMigration {
        version: 4,
        description: "add scheduler feedback table for feedback-driven routing",
    },
    PgMigration {
        version: 5,
        description: "add agent_profiles table and profile_id column on workflow_run_nodes",
    },
    PgMigration {
        version: 6,
        description: "add tool_capabilities, tool_allowlists, and tool_hooks tables",
    },
    PgMigration {
        version: 7,
        description: "add orchestration_decisions table for policy decision trace",
    },
    PgMigration {
        version: 8,
        description: "add executor_pool table for resource/executor pool tracking",
    },
    PgMigration {
        version: 9,
        description: "add queue/priority/backpressure columns to workflow_runs",
    },
    PgMigration {
        version: 10,
        description: "add policy signal columns to orchestration_decisions",
    },
    PgMigration {
        version: 11,
        description: "add scheduler_heartbeat table for persistent heartbeat",
    },
];

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
    client: &mut postgres::Client,
    table: &str,
    column: &str,
) -> Result<bool, String> {
    let row = client
        .query_one(
            "SELECT EXISTS(
                SELECT 1 FROM information_schema.columns
                WHERE table_name = $1 AND column_name = $2
            )",
            &[&table, &column],
        )
        .map_err(|e| format!("information_schema query failed: {e}"))?;
    Ok(row.get::<_, bool>(0))
}

impl LocalProductStore {
    pub(crate) fn run_pg_migrations_internal(&self) -> Result<(), String> {
        self.with_pg_conn(|client: &mut postgres::Client| {
            ensure_schema_migrations_table(client)?;
            let current = current_pg_version(client)?;

            for migration in PG_MIGRATIONS {
                if migration.version <= current {
                    continue;
                }
                let sql = match migration.version {
                    1..=9 | 11 => {
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
                    _ => return Err(format!("unknown pg migration version: {}", migration.version)),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_migration_list_is_sorted_and_contiguous() {
        for (i, m) in PG_MIGRATIONS.iter().enumerate() {
            assert_eq!(
                m.version,
                (i + 1) as i64,
                "migration version mismatch at index {i}"
            );
        }
    }

    #[test]
    fn current_pg_version_constant_matches_list() {
        assert_eq!(CURRENT_PG_VERSION, PG_MIGRATIONS.last().unwrap().version);
    }
}
