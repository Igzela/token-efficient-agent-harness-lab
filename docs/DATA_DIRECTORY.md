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

## Safety

- The engine never writes to target repositories
- Backups require explicit `confirm_local_backup=true` and `backup:admin` scope
- Import requires explicit `confirm_import=true` and `config:admin` scope
- Restore requires explicit `confirm_restore=true` and `backup:admin` scope
- All mutations are logged to the audit log
