# Data Directory

The agent control plane stores all local state under `.agent-control-plane/` (relative to the working directory). This directory is auto-created on first startup.

## Layout

```
.agent-control-plane/
├── local-team.db          # Main SQLite database (WAL mode)
├── local-team.db-wal      # Write-ahead log (auto-managed by SQLite)
├── local-team.db-shm      # Shared memory file (auto-managed by SQLite)
└── backups/               # Local backup directory
    ├── backup_metadata.json
    ├── backup-0001.db
    └── ...
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ACP_DB_PATH` | `.agent-control-plane/local-team.db` | Path to the main SQLite database |
| `ACP_BACKUP_DIR` | `<db_parent>/backups` | Directory for local backup files |

## Database Schema

The main database has 6 tables:

- **dispatch_history** — Every dispatch request and its result
- **local_config** — Key-value configuration store
- **team_members** — Team member records (user_id, display_name, role)
- **api_key_metadata** — API key tracking (scopes, expiry, revocation)
- **audit_log** — Audit trail of all mutations
- **provider_audit_events** — Provider-level audit events

Schema version is tracked via `PRAGMA user_version`. Currently at version 1.

## File Management

### WAL Checkpoint

SQLite uses WAL (Write-Ahead Logging) mode for concurrent reads. The WAL file grows until checkpointed. The engine checkpoints automatically before backup creation. To manually checkpoint:

```bash
sqlite3 .agent-control-plane/local-team.db "PRAGMA wal_checkpoint(FULL);"
```

### Backup Rotation

Backups accumulate in the `backups/` directory. There is no automatic rotation. To clean up old backups:

1. List backups: `GET /api/v1/backups` (requires `backup:admin` scope)
2. Delete individual backups: use `BackupManager::delete_backup()` or remove files manually

### Export/Import

Full state can be exported and imported:

```bash
# Export
curl -H "Authorization: Bearer <key>" http://localhost:8080/api/v1/export > export.json

# Import (requires confirm_import=true)
curl -X POST -H "Content-Type: application/json" \
  -H "Authorization: Bearer <key>" \
  -d '{"snapshot": <export_json>, "confirm_import": true}' \
  http://localhost:8080/api/v1/import
```

Import is idempotent for existing dispatch IDs: repeated imports skip dispatch rows that are already present.

### Integrity Check

Verify database integrity:

```bash
curl -H "Authorization: Bearer <key>" http://localhost:8080/api/v1/storage/integrity
```

Returns per-table row counts and `PRAGMA integrity_check` status.

### Backup Verify and Restore Dry-Run

Verify a backup without touching the live database:

```bash
curl -H "Authorization: Bearer <key>" \
  http://localhost:8080/api/v1/backups/backup-0001/verify
```

Dry-run restore verification:

```bash
curl -X POST -H "Content-Type: application/json" \
  -H "Authorization: Bearer <key>" \
  -d '{"confirm_restore_dry_run": true}' \
  http://localhost:8080/api/v1/backups/backup-0001/restore/dry-run
```

For a scripted smoke:

```bash
uv run --no-project python scripts/acp_restore_smoke.py --token "$ACP_ADMIN_API_KEY"
```

The smoke creates a backup, verifies checksum/integrity, and runs restore dry-run. It skips real restore unless `--execute-restore --confirm-execute-restore` is provided.

## Docker

When running via `docker compose up`, the engine service mounts a Docker named volume (`acp-data`) at `/data`. The environment variables `ACP_DB_PATH=/data/local-team.db` and `ACP_BACKUP_DIR=/data/backups` are set automatically. This means:

- **Data persists across container restarts** (`docker compose restart`).
- **Data persists across container recreation** (`docker compose down && docker compose up`).
- **Data is lost only if the volume is explicitly removed** (`docker compose down -v` or `docker volume rm`).

To back up Docker-persisted data:

```bash
# Copy the database out of the volume
docker compose cp engine:/data/local-team.db ./local-team.db

# Or use the API (requires auth)
curl -H "Authorization: Bearer <key>" http://localhost:8080/api/v1/export > export.json
```

## Operational Runbook

### Health Checks

| Endpoint | Scope | Purpose |
|----------|-------|---------|
| `GET /api/v1/health` | none | Basic liveness probe |
| `GET /api/v1/ready` | none | Readiness with store connectivity |
| `GET /api/v1/metrics` | `health:read` | Operational metrics |
| `GET /api/v1/scheduler/status` | `health:read` | Scheduler state |
| `GET /api/v1/storage/integrity` | `config:admin` | SQLite integrity check |

### Key Metrics to Monitor

| Metric | Alert Condition | Action |
|--------|----------------|--------|
| `queue_length` > 0, `active_runs` = 0 | Scheduler stuck | Check `scheduler.status.last_error`; restart scheduler |
| `error_count` rising | Executor failures | Check `scheduler.status.last_error` for root cause |
| `secret_block_count` > 0 | Credential leak detected | Review blocked artifacts in audit log |
| `retry_count` rising | Executor instability | Check executor binary availability and task complexity |
| `pricing_configured` = false, `provider_enabled` = true | Missing cost tracking | Set `ACP_PROVIDER_INPUT_COST_PER_1K_USD` / `ACP_PROVIDER_OUTPUT_COST_PER_1K_USD` |

### Audit Events

| Action | Resource | When |
|--------|----------|------|
| `workflow_run.create` | run_id | Run created from plan |
| `workflow_run.completed` / `workflow_run.failed` | run_id | Run reaches terminal state |
| `supervised_patch.capture` | workspace_id | Artifact captured with secret scan results |
| `supervised_patch.export` | artifact_id | Artifact exported with approval binding |
| `supervised_patch.cleanup` | workspace_id | Workspace directory removed |
| `supervised_patch.quarantine` | workspace_id | Workspace quarantined |
| `supervised_patch.workspace_status_update` | workspace_id | State transition |

### Backup & Recovery

```bash
# Create backup
curl -X POST -H "Content-Type: application/json" \
  -H "Authorization: Bearer <key>" \
  -d '{"confirm_local_backup": true}' \
  http://localhost:8080/api/v1/backups

# Verify backup
curl -H "Authorization: Bearer <key>" \
  http://localhost:8080/api/v1/backups/<backup_id>/verify

# Restore dry-run
curl -X POST -H "Content-Type: application/json" \
  -H "Authorization: Bearer <key>" \
  -d '{"confirm_restore_dry_run": true}' \
  http://localhost:8080/api/v1/backups/<backup_id>/restore/dry-run

# Scripted smoke
uv run --no-project python scripts/acp_restore_smoke.py --token "$ACP_ADMIN_API_KEY"
```

## Safety

- The engine never writes to target repositories
- Backups require explicit `confirm_local_backup=true` and `backup:admin` scope
- Backup verification and restore dry-run require `backup:admin`; dry-run does not overwrite the live DB
- Import requires explicit `confirm_import=true` and `config:admin` scope
- Restore requires explicit `confirm_restore=true` and `backup:admin` scope
- All mutations are logged to the audit log
