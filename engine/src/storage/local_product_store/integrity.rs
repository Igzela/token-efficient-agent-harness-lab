use super::{count_table, DatabaseConnection, LocalProductStore};

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
    "supervised_patch_workspaces",
    "supervised_patch_artifacts",
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
];

impl LocalProductStore {
    pub fn check_integrity(&self) -> Result<IntegrityReport, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let status: String = conn
                    .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                    .map_err(|e| e.to_string())?;

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
