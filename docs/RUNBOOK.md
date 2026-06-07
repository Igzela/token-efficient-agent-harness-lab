# Agent Control Plane — Self-Hosted GA Runbook

Operator procedures for deploying, operating, backing up, upgrading, and triaging the Agent Control Plane on a local machine or LAN.

---

## 1. Prerequisites

### Required Toolchain

| Tool | Version | Purpose |
|------|---------|---------|
| Rust | stable (1.75+) | Engine binary |
| Bun | 22+ | Dashboard build |
| uv | latest | Python script runner |
| Node | 22+ | Dashboard tooling |

### Verify Toolchain

```bash
uv run --no-project python scripts/acp_local_doctor.py
```

This checks Rust, Bun, Node, uv, ports, disk space, and SQLite availability. All checks must pass before proceeding.

---

## 2. First-Time Setup

### 2.1 Clone and Enter Repository

```bash
git clone https://github.com/<org>/token-efficient-agent-harness-lab.git
cd token-efficient-agent-harness-lab
```

### 2.2 Generate Admin API Key

```bash
uv run --no-project python scripts/bootstrap_local_auth.py --json
```

This prints a `harness_<64 hex chars>` key and the env vars to set. Save the key; you will need it for the env file and all authenticated API calls.

### 2.3 Create Environment File

```bash
cp .env.production-like.local.example .env.production-like.local
```

Edit `.env.production-like.local` and fill in:

- `ACP_ADMIN_API_KEY` — the key from step 2.2
- `ACP_CN_ANTHROPIC_API_KEY` — your provider secret (if using a real provider)

### 2.4 Export Provider Key (if using provider)

```bash
export ACP_CN_ANTHROPIC_API_KEY="<your-provider-secret>"
```

### 2.5 Build Dashboard

```bash
cd dashboard && bun install --frozen-lockfile && bun run build:static && cd ..
```

### 2.6 Start the Engine

```bash
bash scripts/start_production_like_local.sh
```

The script validates auth, provider config, and dashboard presence before starting `cargo run -p engine`.

### 2.7 Verify

```bash
curl http://127.0.0.1:8080/api/v1/health
```

Expected: JSON with `"status": "ok"`.

---

## 3. Daily Operations

### 3.1 Health Check

```bash
uv run --no-project python scripts/acp_ops_check.py --token $ACP_ADMIN_API_KEY
```

Runs a suite of checks against the running engine: health endpoint, storage integrity, backup presence, and more. All checks should report `pass`.

### 3.2 Metrics

```bash
curl http://127.0.0.1:8080/api/v1/metrics
```

Returns request counts, dispatch stats, and cost data.

### 3.3 Dashboard

Open in browser:

```
http://127.0.0.1:8080
```

The dashboard provides run status, decision traces, queue state, executor pool health, and cost visibility.

### 3.4 Scheduler Status

```bash
curl -s -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  http://127.0.0.1:8080/api/v1/scheduler/status
```

### 3.5 Executor Pool

```bash
curl -s -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  http://127.0.0.1:8080/api/v1/executor-pool
```

---

## 4. Backup and Restore

### 4.1 Create Backup

```bash
curl -s -X POST http://127.0.0.1:8080/api/v1/backups \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"confirm_local_backup": true}'
```

Returns a backup ID.

### 4.2 Verify Backup

```bash
curl -s -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  http://127.0.0.1:8080/api/v1/backups/<BACKUP_ID>/verify
```

### 4.3 Restore Dry-Run

```bash
curl -s -X POST \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  http://127.0.0.1:8080/api/v1/backups/<BACKUP_ID>/restore/dry-run
```

Checks that the backup is valid and restorable without modifying the live database.

### 4.4 Full Restore Smoke Test

```bash
uv run --no-project python scripts/acp_restore_smoke.py --token $ACP_ADMIN_API_KEY
```

End-to-end: creates a backup, restores it, and verifies database integrity.

---

## 5. Upgrade Flow

### 5.1 Run Release Drill

```bash
uv run --no-project python scripts/ga_release_checklist.py --token $ACP_ADMIN_API_KEY
```

### 5.2 Build Release Tarball

```bash
bash scripts/package-release.sh <VERSION>
```

Example:

```bash
bash scripts/package-release.sh 0.1.0
```

Produces `dist/agent-control-plane-v<VERSION>-linux-x86_64.tar.gz` with the engine binary, dashboard, install script, and upgrade script.

### 5.3 Install / Upgrade

```bash
bash scripts/upgrade.sh --prefix /usr/local
```

This stops the running engine (if any), copies the new binary to `<prefix>/bin/agent-control-plane`, and restarts it.

### 5.4 Smoke Test the Tarball

```bash
bash scripts/smoke_release.sh <VERSION>
```

Extracts the tarball to a temp directory, installs, starts the engine, hits health/metrics endpoints, and tears down.

---

## 6. Rollback Drill

```bash
uv run --no-project python scripts/ga_rollback_drill.py --token $ACP_ADMIN_API_KEY
```

The drill exercises: health → backup → verify → restore dry-run → storage integrity → metrics snapshot → second health check. All steps are non-destructive.

Manual rollback procedure (if needed):

1. Create a backup (section 4.1).
2. Run the upgrade (section 5.3).
3. Verify health (section 3.1).
4. Restore from the pre-upgrade backup (section 4.3 dry-run, then full restore).
5. Verify health again.

---

## 7. Incident Triage

When the system is unhealthy or behaving unexpectedly, run these checks in order.

### 7.1 Health

```bash
uv run --no-project python scripts/acp_ops_check.py --token $ACP_ADMIN_API_KEY
```

### 7.2 Storage Integrity

```bash
curl -s -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  http://127.0.0.1:8080/api/v1/storage/integrity
```

### 7.3 Recent Audit Log

```bash
curl -s -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  "http://127.0.0.1:8080/api/v1/audit?limit=50"
```

Look for errors, unexpected mutations, or auth failures.

### 7.4 Scheduler Status

```bash
curl -s -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  http://127.0.0.1:8080/api/v1/scheduler/status
```

Check: is the scheduler running? Are ticks succeeding?

### 7.5 Executor Pool

```bash
curl -s -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  http://127.0.0.1:8080/api/v1/executor-pool
```

Check: are executors healthy? Any stuck leases?

### 7.6 Recent Decisions

```bash
curl -s -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  "http://127.0.0.1:8080/api/v1/decisions?limit=20"
```

Check: are decisions being recorded? Any error signals in `degraded_reason` or `policy_signals`?

### 7.7 Queue Status

```bash
curl -s -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  http://127.0.0.1:8080/api/v1/queue/status
```

Check: queue depth, backpressure state, priority distribution.

### 7.8 Metrics

```bash
curl -s -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  http://127.0.0.1:8080/api/v1/metrics
```

Check: request rates, error rates, cost accumulators.

---

## 8. Secret Scan

Before any provider trial or commit that touches config/env files:

```bash
uv run --no-project python scripts/acp_secret_scan.py
```

Scans the repository for leaked tokens (Anthropic, OpenRouter, OpenAI, Google, AWS, local admin keys). Must return zero findings before proceeding.

---

## 9. Configuration Reference

All environment variables are documented in `.env.example`. Key variables:

| Variable | Default | Purpose |
|----------|---------|---------|
| `ACP_PROFILE` | `local` | Startup profile: `local` or `production` |
| `ACP_REQUIRE_AUTH` | (off) | Set to `1` to require API key auth |
| `ACP_ADMIN_API_KEY` | (none) | Admin key, must match `harness_<64 hex>` |
| `ACP_DB_PATH` | `.agent-control-plane/local-team.db` | SQLite database path |
| `ACP_BACKUP_DIR` | `<db_parent>/backups` | Backup directory |
| `ACP_DASHBOARD_DIR` | `dashboard/out` | Static dashboard assets path |
| `ACP_PROVIDER_TYPE` | `stub` | Provider: `stub`, `openai_compatible`, `anthropic` |
| `ACP_ENABLE_PROVIDER_EXECUTION` | (off) | Set to `1` for real provider calls |
| `ACP_ENABLE_CLI_EXECUTION` | (off) | Set to `1` for CLI executor (claude/codex) |
| `ACP_SCHEDULER_EXECUTOR` | `noop` | Executor type: `noop`, `command`, `claude_code_cli`, `codex_cli` |
| `ACP_CORS_ORIGINS` | `*` | Comma-separated allowed origins |
| `ACP_COST_PER_DISPATCH_USD` | (unlimited) | Per-dispatch cost cap |
| `ACP_COST_DAILY_USD` | (unlimited) | Daily cost cap |
| `HOST` | `127.0.0.1` | Bind address |
| `PORT` | `8080` | Listen port |

See `.env.example` for the full list including scheduler tuning, CLI binary paths, and provider price overrides.

---

## 10. Release Checklist

Run before every release or major deployment:

```bash
uv run --no-project python scripts/ga_release_checklist.py --token $ACP_ADMIN_API_KEY
```

The checklist validates: secret scan, health/readiness, storage integrity, backup create+verify, restore dry-run, metrics, dashboard build, and auth enforcement.

Manual checks (if the script is unavailable):

1. **Secret scan** — `uv run --no-project python scripts/acp_secret_scan.py` (zero findings)
2. **Ops health** — `uv run --no-project python scripts/acp_ops_check.py --token $ACP_ADMIN_API_KEY` (all pass)
3. **Backup verify** — create backup, verify it (section 4.1-4.2)
4. **Restore dry-run** — dry-run restore from latest backup (section 4.3)
5. **Storage integrity** — `GET /api/v1/storage/integrity` (no corruption)
6. **Dashboard build** — `cd dashboard && bun run build:static` (succeeds)
7. **Full test suite** — `cargo test -p engine` (all pass)
8. **Lint/format** — `cargo fmt --check && cargo clippy -p engine --all-targets -- -D warnings`
9. **TypeScript** — `cd dashboard && bun run build` (strict + lint pass)
10. **Wire codegen drift** — `bash scripts/check_wire_codegen_drift.sh` (no drift)
