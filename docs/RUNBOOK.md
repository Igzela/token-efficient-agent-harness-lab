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

### Real Pilot 1: one-command local output

Use this path first when you want proof that a local checkout can produce a real patch and `acp/*` branch without understanding the internal gates:

```bash
uv run --no-project python scripts/real_pilot_1.py
```

What it does:

- starts the engine on `127.0.0.1` with a temporary SQLite DB
- enables only guarded target output for a local filesystem remote
- creates a real temporary git target repo and local bare remote
- creates plan/run/workspace records through the API
- executes a small command in the app-owned git worktree
- captures artifact evidence, records approval, exports a patch file, and pushes an `acp/*` branch
- prints dashboard/API URL, evidence dir, patch file, branch commit, and rollback command

Boundary: no provider calls, no Claude/Codex CLI execution, no secrets, no target `main` write, no deploy/release/apply authority.

### Shortest Local Operator Path

Use this path when you need the smallest safe local loop: start the engine, authenticate if required, run a noop dispatch, then decide whether to opt into the guarded CLI flow.

1. Build the dashboard:

```bash
cd dashboard && bun install --frozen-lockfile && bun run build:static && cd ..
```

2. Start default local noop mode:

```bash
ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine
```

3. If protected mode is required, generate an admin key and restart with auth:

```bash
uv run --no-project python scripts/bootstrap_local_auth.py --json
ACP_REQUIRE_AUTH=1 ACP_ADMIN_API_KEY=<harness_...> ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine
```

Paste the generated `harness_...` key into the dashboard auth panel. A missing key blocks protected tabs; a key with insufficient scopes returns 403 and the dashboard reports which scope is missing.

4. Prove the safe noop path:

```bash
curl -s -X POST http://127.0.0.1:8080/api/v1/dispatch \
  -H "Content-Type: application/json" \
  -d '{"raw_request":"Summarize docs without provider calls","request_source":"api"}'
```

5. Optional guarded CLI flow: set `ACP_ENABLE_CLI_EXECUTION=1` and select a CLI executor only for local trials where the operator accepts subprocess execution. If the gate is off, CLI-backed actions remain unavailable and the dashboard shows the CLI gate as off/default-safe.

Provider execution remains off unless `ACP_ENABLE_PROVIDER_EXECUTION=1`. Target output remains off unless `ACP_ENABLE_TARGET_REPO_OUTPUT=1`; when enabled, it is limited to a controlled app-owned git worktree, patch export, or approval-bound `acp/*` branch push. It never writes the registered target working tree or `main`.

### 2.1 V2-3 Target Repo Output

Required production-like settings:

```bash
export ACP_ENABLE_TARGET_REPO_OUTPUT=1
export ACP_TARGET_REPO_REMOTE_ALLOWLIST=origin
export ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST=github.com
export ACP_TARGET_REPO_GIT_TOKEN_ENV=GITHUB_TOKEN
export GITHUB_TOKEN=<token-with-repository-branch-push-access>
```

Optional username: `ACP_TARGET_REPO_GIT_USERNAME` (default `x-access-token`). Local filesystem remotes are test-only and require `ACP_TARGET_REPO_ALLOW_LOCAL_REMOTE=1`.

1. Create the supervised workspace with `"workspace_mode":"git_worktree"` using a key with `dispatch:execute`.
2. Execute only inside the returned app-owned workspace.
3. Complete workflow verification, then capture the artifact; review `review_diff`, `evidence_bundle`, secret status, and content-bound `patch_hash`. Target output accepts bounded text files only.
4. Record an approval on the same run, bound to source revision, patch hash, and changed files.
5. POST `/api/v1/supervised-patch/artifacts/{artifact_id}/output` with `confirm_target_output:true` and mode `export_patch` or `push_branch`.

Emergency stop:

```bash
export ACP_TARGET_REPO_OUTPUT_KILL_SWITCH=1
```

This blocks new worktree/output actions. Existing pushed branches remain auditable; cleanup removes app-owned worktrees but does not delete remote branches.

### 2.2 Clone and Enter Repository

```bash
git clone https://github.com/<org>/token-efficient-agent-harness-lab.git
cd token-efficient-agent-harness-lab
```

### 2.3 Generate Admin API Key

```bash
uv run --no-project python scripts/bootstrap_local_auth.py --json
```

This prints a `harness_<64 hex chars>` key and the env vars to set. Save the key; you will need it for the env file and all authenticated API calls.

### 2.4 Create Environment File

```bash
cp .env.production-like.local.example .env.production-like.local
```

Edit `.env.production-like.local` and fill in:

- `ACP_ADMIN_API_KEY` — the key from step 2.2
- `ACP_CN_ANTHROPIC_API_KEY` — your provider secret (if using a real provider)

### 2.5 Export Provider Key (if using provider)

```bash
export ACP_CN_ANTHROPIC_API_KEY="<your-provider-secret>"
```

### 2.6 Build Dashboard

```bash
cd dashboard && bun install --frozen-lockfile && bun run build:static && cd ..
```

### 2.7 Start the Engine

```bash
bash scripts/start_production_like_local.sh
```

The script validates auth, provider config, and dashboard presence before starting `cargo run -p engine`.

### 2.8 Verify

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

Start bounded workers with both gates:

```bash
ACP_ENABLE_SCHEDULER=1 \
ACP_ENABLE_SUPERVISED_WORKERS=1 \
ACP_SUPERVISED_WORKER_COUNT=2 \
cargo run -p engine
```

```bash
curl -s -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  http://127.0.0.1:8080/api/v1/scheduler/status
```

Pause, resume, or kill new worker claims:

```bash
curl -s -X POST http://127.0.0.1:8080/api/v1/scheduler/control \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"action":"pause","confirm_control":true}'
```

Use `resume` to continue. Use `kill` or set `ACP_SUPERVISED_WORKERS_KILL_SWITCH=1` for emergency stop. Kill blocks new claims immediately; an already-running command/provider/CLI call drains under its configured timeout. These actions require `dispatch:execute` and are audited.

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

Check: are both gates enabled, expected workers present, heartbeats fresh, ticks succeeding, and pause/kill state correct?

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
