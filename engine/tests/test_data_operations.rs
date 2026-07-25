use engine::storage::local_product_store::{
    ImportCounts, ImportResult, LocalProductStore, LOCAL_TEAM_EXPORT_SCHEMA_VERSION,
};
use serde_json::{json, Value};
use tempfile::tempdir;

fn make_export_snapshot() -> Value {
    json!({
        "schema_version": LOCAL_TEAM_EXPORT_SCHEMA_VERSION,
        "generated_at": "2026-05-29T00:00:00Z",
        "dispatches": [],
        "plans": [],
        "config": {
            "workspace_name": json!("Imported Team"),
            "provider_transport": json!("stub/off"),
        },
        "team": {
            "schema_version": "local_team.v1",
            "members": [
                {
                    "user_id": "imported-user",
                    "display_name": "Imported User",
                    "role": "admin",
                    "created_at": "2026-05-29T00:00:00Z",
                    "updated_at": "2026-05-29T00:00:00Z",
                }
            ],
            "api_keys": [],
        },
        "costs": {},
        "audit": [
            {
                "actor": "original-export",
                "action": "test.event",
                "resource": "test-resource",
                "details": {"note": "from export"},
            }
        ],
        "boundaries": {},
    })
}

// --- migration tests ---

#[test]
fn schema_version_returns_current_version() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let version = store.schema_version().unwrap();
    assert_eq!(version, 34);
}

#[test]
fn migration_runs_only_once() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    let _store1 = LocalProductStore::new(&path).unwrap();
    let store2 = LocalProductStore::new(&path).unwrap();
    assert_eq!(store2.schema_version().unwrap(), 34);
}

#[test]
fn migration_v1_adds_key_columns() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let key_result = store.record_api_key_metadata(
        "key-v1-test",
        "user1",
        "admin",
        &["team:read".to_string()],
        "test",
    );
    assert!(key_result.is_ok());
    let key = store.get_api_key_metadata("key-v1-test").unwrap();
    assert!(key.is_some());
    let key_val = key.unwrap();
    assert!(key_val.get("last_used_at").is_some());
    assert!(key_val.get("expires_at").is_some());
}

#[test]
fn fresh_database_starts_at_version_0_before_migrations() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 0);
    }
    let _store = LocalProductStore::new(&db_path).unwrap();
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 34);
    }
}

#[test]
fn migration_v22_preserves_existing_allowlist_authority() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("v21-allowlist.db");
    {
        let store = LocalProductStore::new(&db_path).unwrap();
        store
            .configure_tool_capability(
                "setup",
                "echo",
                "legacy bounded fixture",
                None,
                None,
                false,
                "low",
                None,
            )
            .unwrap();
        store
            .configure_tool_allowlist("setup", "legacy-locked", &["echo".to_string()], None)
            .unwrap();
    }
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("DELETE FROM tool_allowlist_profiles", [])
            .unwrap();
        conn.execute_batch("PRAGMA user_version = 21;").unwrap();
    }

    let upgraded = LocalProductStore::new(&db_path).unwrap();
    assert_eq!(upgraded.schema_version().unwrap(), 34);
    assert!(upgraded
        .check_tool_allowed("legacy-locked", "echo")
        .unwrap());
    assert!(!upgraded
        .check_tool_allowed("legacy-locked", "bash")
        .unwrap());
    assert_eq!(
        upgraded
            .read_tool_allowlist_policy("legacy-locked")
            .unwrap()
            .unwrap()["value"]["tool_names"],
        json!(["echo"])
    );
}

#[test]
fn policy_snapshot_indexes_exist_in_sqlite() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let _store = LocalProductStore::new(&db_path).unwrap();
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut stmt = conn
        .prepare("PRAGMA index_list('controlled_loop_policy_snapshots')")
        .unwrap();
    let indexes = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    for expected in [
        "idx_policy_snapshots_status",
        "idx_policy_snapshots_proposal",
        "idx_policy_snapshots_adjustment",
        "idx_policy_snapshots_policy_key",
        "idx_policy_snapshots_active_policy_key",
    ] {
        assert!(
            indexes.iter().any(|name| name == expected),
            "missing SQLite index {expected}"
        );
    }
}

// --- integrity tests ---

#[test]
fn check_integrity_on_clean_database() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let report = store.check_integrity().unwrap();
    assert_eq!(report.status, "ok");
    assert_eq!(report.schema_version, 34);
    assert_eq!(report.tables.len(), 65);
    for table in &report.tables {
        assert_eq!(table.status, "ok");
        assert!(table.row_count >= 0);
    }
}

#[test]
fn check_integrity_after_writes() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    store
        .record_dispatch(
            "req1",
            "test",
            &json!({"record": {"dispatch_id": "d1"}}),
            "actor",
        )
        .unwrap();
    store.upsert_team_member("u1", "User 1", "admin").unwrap();
    store.set_config_value("k1", json!("v1"), "actor").unwrap();

    let report = store.check_integrity().unwrap();
    assert_eq!(report.status, "ok");
    let dispatch_table = report
        .tables
        .iter()
        .find(|t| t.name == "dispatch_history")
        .unwrap();
    assert_eq!(dispatch_table.row_count, 1);
    let team_table = report
        .tables
        .iter()
        .find(|t| t.name == "team_members")
        .unwrap();
    assert_eq!(team_table.row_count, 1);
}

#[test]
fn check_integrity_table_names() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let report = store.check_integrity().unwrap();
    let names: Vec<&str> = report.tables.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"dispatch_history"));
    assert!(names.contains(&"local_config"));
    assert!(names.contains(&"team_members"));
    assert!(names.contains(&"api_key_metadata"));
    assert!(names.contains(&"audit_log"));
    assert!(names.contains(&"provider_audit_events"));
    assert!(names.contains(&"workflow_plans"));
    assert!(names.contains(&"workflow_runs"));
    assert!(names.contains(&"workflow_run_nodes"));
    assert!(names.contains(&"workflow_run_edges"));
    assert!(names.contains(&"workflow_run_events"));
    assert!(names.contains(&"workflow_run_approvals"));
    assert!(names.contains(&"supervised_patch_workspaces"));
    assert!(names.contains(&"supervised_patch_artifacts"));
    assert!(names.contains(&"regression_report_artifacts"));
    assert!(names.contains(&"controlled_loop_policy_proposals"));
}

// --- import tests ---

#[test]
fn import_snapshot_config() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let snapshot = json!({
        "schema_version": LOCAL_TEAM_EXPORT_SCHEMA_VERSION,
        "config": {"my_key": json!("my_value")},
        "team": {"members": [], "api_keys": []},
        "audit": [],
        "dispatches": [],
        "plans": [],
    });
    let result = store.import_snapshot(&snapshot).unwrap();
    assert_eq!(result.errors.len(), 0);
    assert_eq!(result.imported.config, 1);
    let config = store.config_snapshot().unwrap();
    assert_eq!(config["my_key"], "my_value");
}

#[test]
fn import_snapshot_team_members() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let snapshot = make_export_snapshot();
    let result = store.import_snapshot(&snapshot).unwrap();
    assert_eq!(result.errors.len(), 0);
    assert_eq!(result.imported.team, 1);
    let team = store.team_snapshot().unwrap();
    let members = team["members"].as_array().unwrap();
    assert!(members.iter().any(|m| m["user_id"] == "imported-user"));
}

#[test]
fn import_snapshot_audit_events() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let snapshot = make_export_snapshot();
    let result = store.import_snapshot(&snapshot).unwrap();
    assert_eq!(result.imported.audit, 1);
    let audit = store.audit_events(100).unwrap();
    assert!(audit.iter().any(|e| e["action"] == "test.event"));
}

#[test]
fn import_snapshot_dispatches() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let snapshot = json!({
        "schema_version": LOCAL_TEAM_EXPORT_SCHEMA_VERSION,
        "config": {},
        "team": {"members": [], "api_keys": []},
        "audit": [],
        "dispatches": [
            {
                "raw_request": "{\"test\": true}",
                "request_source": "import",
                "bundle": {"record": {"dispatch_id": "imported-1"}},
            }
        ],
        "plans": [],
    });
    let result = store.import_snapshot(&snapshot).unwrap();
    assert_eq!(result.imported.dispatches, 1);
    let dispatches = store.list_dispatches(100).unwrap();
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0]["dispatch_id"], "imported-1");
}

#[test]
fn import_snapshot_dispatches_skips_existing_dispatch_id() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let snapshot = json!({
        "schema_version": LOCAL_TEAM_EXPORT_SCHEMA_VERSION,
        "config": {},
        "team": {"members": [], "api_keys": []},
        "audit": [],
        "dispatches": [
            {
                "raw_request": "{\"test\": true}",
                "request_source": "import",
                "bundle": {"record": {"dispatch_id": "imported-1"}},
            }
        ],
        "plans": [],
    });

    let first = store.import_snapshot(&snapshot).unwrap();
    let second = store.import_snapshot(&snapshot).unwrap();

    assert_eq!(first.imported.dispatches, 1);
    assert_eq!(second.imported.dispatches, 0);
    let dispatches = store.list_dispatches(100).unwrap();
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0]["dispatch_id"], "imported-1");
}

#[test]
fn import_snapshot_wrong_schema_version_rejects() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let snapshot = json!({
        "schema_version": "wrong_version",
        "config": {},
        "team": {"members": [], "api_keys": []},
        "audit": [],
        "dispatches": [],
        "plans": [],
    });
    let result = store.import_snapshot(&snapshot);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("unsupported schema version"));
}

#[test]
fn import_snapshot_idempotent_config() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let snapshot = json!({
        "schema_version": LOCAL_TEAM_EXPORT_SCHEMA_VERSION,
        "config": {"key1": json!("value1")},
        "team": {"members": [], "api_keys": []},
        "audit": [],
        "dispatches": [],
        "plans": [],
    });
    store.import_snapshot(&snapshot).unwrap();
    let snapshot2 = json!({
        "schema_version": LOCAL_TEAM_EXPORT_SCHEMA_VERSION,
        "config": {"key1": json!("value2")},
        "team": {"members": [], "api_keys": []},
        "audit": [],
        "dispatches": [],
        "plans": [],
    });
    let result = store.import_snapshot(&snapshot2).unwrap();
    assert_eq!(result.imported.config, 1);
    let config = store.config_snapshot().unwrap();
    assert_eq!(config["key1"], "value2");
}

#[test]
fn import_snapshot_team_member_missing_user_id() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let snapshot = json!({
        "schema_version": LOCAL_TEAM_EXPORT_SCHEMA_VERSION,
        "config": {},
        "team": {"members": [{"display_name": "No ID", "role": "member"}], "api_keys": []},
        "audit": [],
        "dispatches": [],
        "plans": [],
    });
    let result = store.import_snapshot(&snapshot).unwrap();
    assert_eq!(result.imported.team, 0);
    assert!(result.errors.iter().any(|e| e.contains("missing user_id")));
}

// --- export/import roundtrip tests ---

#[test]
fn export_import_roundtrip_config() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    store
        .set_config_value("ws_name", json!("Roundtrip Team"), "test")
        .unwrap();
    store
        .upsert_team_member("rt-user", "RT User", "admin")
        .unwrap();

    let export = store.export_snapshot("noop", false).unwrap();
    let dir2 = tempdir().unwrap();
    let store2 = LocalProductStore::new(dir2.path().join("test.db")).unwrap();
    let result = store2.import_snapshot(&export).unwrap();
    assert_eq!(result.errors.len(), 0);

    let config = store2.config_snapshot().unwrap();
    assert_eq!(config["ws_name"], "Roundtrip Team");
    let team = store2.team_snapshot().unwrap();
    let members = team["members"].as_array().unwrap();
    assert!(members.iter().any(|m| m["user_id"] == "rt-user"));
}

#[test]
fn export_import_roundtrip_dispatches() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    store
        .record_dispatch(
            "req-rt",
            "test",
            &json!({"record": {"dispatch_id": "rt-dispatch-1"}}),
            "test",
        )
        .unwrap();

    let export = store.export_snapshot("noop", false).unwrap();
    let dir2 = tempdir().unwrap();
    let store2 = LocalProductStore::new(dir2.path().join("test.db")).unwrap();
    let result = store2.import_snapshot(&export).unwrap();
    assert_eq!(result.errors.len(), 0);
    assert_eq!(result.imported.dispatches, 1);

    let dispatches = store2.list_dispatches(100).unwrap();
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0]["dispatch_id"], "rt-dispatch-1");
}

#[test]
fn export_import_roundtrip_workflow_plans() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    store
        .create_workflow_plan(
            "Plan export roundtrip",
            "api",
            "actor",
            |ids, _created_at| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "analysis-0001"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "nodes": [],
                        "edges": [],
                    },
                    "boundaries": {"execution": "disabled"},
                }))
            },
        )
        .unwrap();

    let export = store.export_snapshot("noop", false).unwrap();
    assert_eq!(export["plans"].as_array().unwrap().len(), 1);

    let dir2 = tempdir().unwrap();
    let store2 = LocalProductStore::new(dir2.path().join("test.db")).unwrap();
    let result = store2.import_snapshot(&export).unwrap();
    assert_eq!(result.errors.len(), 0);
    assert_eq!(result.imported.plans, 1);

    let plans = store2.search_workflow_plans(10, 0, None).unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0]["plan_id"], "plan-0001");
    assert_eq!(plans[0]["raw_request"], "Plan export roundtrip");
}

#[test]
fn export_import_roundtrip_workflow_runs() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    store
        .create_workflow_plan(
            "Plan run export roundtrip",
            "api",
            "actor",
            |ids, _created_at| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "analysis-0001"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "nodes": [{
                            "schema_version": "workflow_node.v1",
                            "node_id": "node-a",
                            "workflow_id": ids.workflow_id,
                            "task_type": "docs",
                            "assigned_agent_id": null,
                            "status": "pending",
                            "input_refs": [],
                            "output_ref": null,
                            "budget": 0.1,
                            "cost_incurred": 0.0,
                            "error": null,
                            "created_at": "2026-06-05T00:00:00Z",
                            "started_at": null,
                            "completed_at": null
                        }],
                        "edges": [],
                    },
                    "boundaries": {"execution": "disabled"},
                }))
            },
        )
        .unwrap();
    store
        .create_workflow_run_from_plan("plan-0001", "actor")
        .unwrap();
    store
        .append_workflow_run_event(
            "run-0001",
            Some("node-a"),
            "node_status_observed",
            &json!({"status": "ready"}),
            "actor",
        )
        .unwrap();
    store
        .record_workflow_run_approval(
            "run-0001",
            "node-a",
            "approved",
            "reviewer",
            Some("metadata only"),
            None,
            None,
            None,
            None,
        )
        .unwrap();

    let export = store.export_snapshot("noop", false).unwrap();
    assert_eq!(export["workflow_runs"].as_array().unwrap().len(), 1);

    let dir2 = tempdir().unwrap();
    let store2 = LocalProductStore::new(dir2.path().join("test.db")).unwrap();
    let result = store2.import_snapshot(&export).unwrap();
    assert_eq!(result.errors.len(), 0);
    assert_eq!(result.imported.workflow_runs, 1);

    let run = store2.get_workflow_run("run-0001").unwrap().unwrap();
    assert_eq!(run["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(run["events"].as_array().unwrap().len(), 2);
    assert_eq!(run["approvals"].as_array().unwrap().len(), 1);
}

#[test]
fn export_import_roundtrip_supervised_patch_metadata() {
    let target_dir = tempdir().unwrap();
    let workspace_root = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let workspace_path = workspace_root.path().join("workspaces").join("ws-001");
    let workspace = store
        .record_supervised_patch_workspace(
            &json!({
                "plan_id": "plan-0001",
                "run_id": "run-0001",
                "target_id": "target-001",
                "target_repo_path": target_dir.path().to_string_lossy(),
                "workspace_path": workspace_path.to_string_lossy(),
                "source_revision": "abc123",
                "source_tree_hash": "tree123",
            }),
            "actor",
        )
        .unwrap();
    store
        .record_supervised_patch_artifact(
            &json!({
                "workspace_id": workspace["workspace_id"],
                "patch_hash": "sha256-patch",
                "changed_files": ["src/lib.rs"],
                "redaction_status": "redacted",
            }),
            "actor",
        )
        .unwrap();

    let export = store.export_snapshot("noop", false).unwrap();
    assert_eq!(
        export["supervised_patch_workspaces"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        export["supervised_patch_artifacts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let dir2 = tempdir().unwrap();
    let store2 = LocalProductStore::new(dir2.path().join("test.db")).unwrap();
    let result = store2.import_snapshot(&export).unwrap();
    assert_eq!(result.errors.len(), 0);
    assert_eq!(result.imported.supervised_patch_workspaces, 1);
    assert_eq!(result.imported.supervised_patch_artifacts, 1);

    let imported_workspace = store2
        .get_supervised_patch_workspace("patch-workspace-0001")
        .unwrap()
        .unwrap();
    assert_eq!(imported_workspace["target_id"], "target-001");
    assert_eq!(
        imported_workspace["boundary"]["target_repository_writes"],
        "disabled"
    );
    let imported_artifact = store2
        .get_supervised_patch_artifact("patch-artifact-0001")
        .unwrap()
        .unwrap();
    assert_eq!(imported_artifact["patch_apply_authority"], "disabled");
    assert_eq!(imported_artifact["changed_files"][0], "src/lib.rs");
}

#[test]
fn import_snapshot_rejects_supervised_patch_workspace_inside_target() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let target_path = dir.path().join("target");
    let workspace_path = target_path.join(".agent-control-plane").join("ws-001");

    let mut snapshot = make_export_snapshot();
    snapshot["supervised_patch_workspaces"] = json!([
        {
            "schema_version": "supervised_patch_workspace.v1",
            "workspace_id": "patch-workspace-0001",
            "run_id": "run-0001",
            "target_id": "target-001",
            "target_repo_path": target_path.to_string_lossy(),
            "target_repo_canonical_path": target_path.to_string_lossy(),
            "workspace_path": workspace_path.to_string_lossy(),
            "workspace_canonical_path": workspace_path.to_string_lossy(),
            "source_revision": "abc123",
            "status": "requested",
            "metadata_only": true,
            "execution_authority": "disabled",
            "boundary": {
                "metadata_only": true,
                "execution_authority": "disabled",
                "workspace_directory_creation": "not_performed",
                "target_repository_writes": "disabled",
                "registered_git_worktree": "forbidden",
                "git_worktree_add": "forbidden",
                "process_execution": "disabled",
                "provider_calls": "disabled",
                "push_merge_deploy_apply": "disabled"
            }
        }
    ]);

    let result = store.import_snapshot(&snapshot).unwrap();
    assert_eq!(result.imported.supervised_patch_workspaces, 0);
    assert!(result
        .errors
        .iter()
        .any(|error| error.contains("outside registered target repository")));
    assert_eq!(store.supervised_patch_workspaces(10).unwrap().len(), 0);
}

#[test]
fn import_snapshot_rejects_unsafe_supervised_patch_artifact_files() {
    let target_dir = tempdir().unwrap();
    let workspace_root = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let workspace_path = workspace_root.path().join("workspaces").join("ws-001");

    let mut snapshot = make_export_snapshot();
    snapshot["supervised_patch_workspaces"] = json!([
        {
            "schema_version": "supervised_patch_workspace.v1",
            "workspace_id": "patch-workspace-0001",
            "run_id": "run-0001",
            "target_id": "target-001",
            "target_repo_path": target_dir.path().to_string_lossy(),
            "target_repo_canonical_path": target_dir.path().to_string_lossy(),
            "workspace_path": workspace_path.to_string_lossy(),
            "workspace_canonical_path": workspace_path.to_string_lossy(),
            "source_revision": "abc123",
            "status": "requested",
            "metadata_only": true,
            "execution_authority": "disabled"
        }
    ]);
    snapshot["supervised_patch_artifacts"] = json!([
        {
            "schema_version": "supervised_patch_artifact.v1",
            "artifact_id": "patch-artifact-0001",
            "workspace_id": "patch-workspace-0001",
            "run_id": "run-0001",
            "target_id": "target-001",
            "source_revision": "abc123",
            "artifact_type": "patch_diff",
            "patch_hash": "sha256-patch",
            "changed_files": ["src\\lib.rs"],
            "redaction_status": "redacted",
            "metadata_only": true,
            "execution_authority": "disabled",
            "patch_apply_authority": "disabled",
            "artifact_file_created": false
        }
    ]);

    let result = store.import_snapshot(&snapshot).unwrap();
    assert_eq!(result.imported.supervised_patch_workspaces, 1);
    assert_eq!(result.imported.supervised_patch_artifacts, 0);
    assert!(result
        .errors
        .iter()
        .any(|error| error.contains("changed file must use forward slashes")));
    assert_eq!(store.supervised_patch_artifacts(10).unwrap().len(), 0);
}

// --- import result struct tests ---

#[test]
fn import_counts_default() {
    let counts = ImportCounts::default();
    assert_eq!(counts.dispatches, 0);
    assert_eq!(counts.config, 0);
    assert_eq!(counts.team, 0);
    assert_eq!(counts.audit, 0);
    assert_eq!(counts.plans, 0);
    assert_eq!(counts.workflow_runs, 0);
    assert_eq!(counts.supervised_patch_workspaces, 0);
    assert_eq!(counts.supervised_patch_artifacts, 0);
}

#[test]
fn import_result_struct_fields() {
    let result = ImportResult {
        imported: ImportCounts {
            dispatches: 3,
            config: 2,
            team: 1,
            audit: 5,
            plans: 4,
            workflow_runs: 3,
            supervised_patch_workspaces: 2,
            supervised_patch_artifacts: 1,
        },
        errors: vec!["err1".to_string()],
    };
    assert_eq!(result.imported.dispatches, 3);
    assert_eq!(result.imported.supervised_patch_workspaces, 2);
    assert_eq!(result.imported.supervised_patch_artifacts, 1);
    assert_eq!(result.errors.len(), 1);
}

// --- integrity report struct tests ---

#[test]
fn integrity_report_fields() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let report = store.check_integrity().unwrap();
    assert!(!report.status.is_empty());
    assert!(report.schema_version >= 1);
    assert!(!report.tables.is_empty());
}
