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

### Real output pilots

Use this path to prove real agent output across three independent disposable repositories:

```bash
scripts/real_output_pilots.py
```

What it does:

- starts the engine on `127.0.0.1` with a temporary SQLite DB
- discovers an authenticated local Claude CLI
- creates Python, Rust, and Node git repositories plus local bare remotes
- creates plan/run/workspace records through the API
- executes each natural-language task inside its app-owned git worktree
- runs real verification, records approval, and pushes one `acp/*` branch per repository
- writes a compact summary with run/artifact/approval/verification/branch evidence

Boundary: no provider API calls, no secrets, no target `main` write, and no merge/deploy/apply authority.

### Shortest Local Operator Path

Use this path when you need the smallest local loop: start the engine, authenticate if required, then create and run a task from the dashboard.

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

5. Installed Claude/Codex CLIs are discovered by default. Select a CLI executor in the task workflow; set `ACP_ENABLE_CLI_EXECUTION=0` when local subprocess execution must be disabled.

Provider execution is enabled by the recommended ready trusted-local profile, or by the standalone legacy `ACP_ENABLE_PROVIDER_EXECUTION=1` gate. Target output remains off unless `ACP_ENABLE_TARGET_REPO_OUTPUT=1`; when enabled, it is limited to a controlled app-owned git worktree, patch export, or approval-bound `acp/*` branch push. It never writes the registered target working tree or `main`.

### Adaptive Fusion provider routing

The recommended trusted-local path enables bounded internal provider execution, adaptive routing, experiments, promotion, and default routing after protected mode, configured auth, positive cost caps, endpoint pricing, and symbolic provider credentials validate:

```bash
export ACP_REQUIRE_AUTH=1
export ACP_ADMIN_API_KEY=<harness_...>
export ACP_TRUSTED_LOCAL_PROFILE=1
export ACP_COST_PER_DISPATCH_USD=1.00
export ACP_COST_DAILY_USD=10.00
export FAST_PROVIDER_KEY=<secret>
export QUALITY_PROVIDER_KEY=<secret>
export ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON='[
  {"endpoint_id":"fast","provider_type":"openai_compatible","base_url":"https://api.example.com/v1","model":"fast-model","credential_env":"FAST_PROVIDER_KEY","timeout_ms":30000,"input_cost_per_1k_usd":0.001,"output_cost_per_1k_usd":0.002},
  {"endpoint_id":"quality","provider_type":"anthropic","base_url":"https://api.anthropic.com","model":"quality-model","credential_env":"QUALITY_PROVIDER_KEY","timeout_ms":60000,"input_cost_per_1k_usd":0.003,"output_cost_per_1k_usd":0.015}
]'
```

At startup, the profile fails closed unless auth, endpoint parsing, credential availability, strictly positive pricing, and both cost caps pass. The dashboard Adaptive Fusion gate panel reports `ready`, `blocked` with stable blocker codes, or `off`. Legacy `ACP_ENABLE_PROVIDER_EXECUTION`, `ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION`, experiment, promotion, and default-routing flags remain supported for independent operation without the profile.

To let the local scheduler advance already-created queued adaptive workflow runs, add the separate acknowledgement:

```bash
export ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT=1
export ACP_SUPERVISED_WORKER_COUNT=1
export ACP_SCHEDULER_MAX_CONCURRENT=4
export ACP_SCHEDULER_INTERVAL_MS=2000
export ACP_SCHEDULER_LEASE_TIMEOUT_MS=300000
```

This path requires the trusted-local profile to be ready and pins `ACP_SCHEDULER_EXECUTOR` to `adaptive_provider` (or uses that default). It fails closed for another executor, malformed/non-positive numeric values, more workers than concurrency, more than 32 workers/concurrent claims, polling outside 250–60000 ms, or leases outside 1000–3600000 ms. It consumes only existing queued runs with explicit bounded adaptive execution plans. It does not create tasks/goals, invoke CLI/command/noop workers, write target repositories, merge, release, or deploy.

The Adaptive Fusion panel reports task advancement as `ready`, `blocked`, or `off`, including stable blockers, worker count, executor, and maximum concurrency. Use the existing authenticated scheduler control endpoint for pause/resume/kill; the adaptive fusion, experiment, promotion, supervised-worker, and target-output kill switches remain independent.

The same panel includes an IAE operator evidence section:

- effective provider, adaptive, default-routing, experiment, promotion, and task-advancement authority
- per-dispatch/daily cost caps, current daily cost and remaining budget
- experiment traffic, cost, token, call, time, and concurrency ceilings
- promotion rollout and worker concurrency bounds
- safe observation counts, success/failure totals, aggregate cost, and latest timestamp
- scheduler running/paused/killed state with confirmed pause/resume/kill controls
- recent adaptive and scheduler control audit actions loaded with `audit:read` and `redact=true`

Only audit action, resource, and timestamp are rendered. Audit details, raw prompts/outputs/transcripts, credentials, repository content, and private paths are not displayed. Policy rollback continues to use the existing snapshot confirmation dialog.

Endpoint JSON stores only credential environment names. Remote HTTP is rejected; HTTPS is required except for loopback test/local adapters. Explicit workflow execution accepts bounded `single`, `ordered_fallback`, or `fusion` plans. Fusion panels may run with bounded concurrency up to 3; judge and synthesizer remain serial. Tick the run with `executor=adaptive_provider`, `max_retries=0`, and a key with `dispatch:execute`.

The AF-6 completion endpoint generates and selects a bounded candidate automatically:

```bash
curl -s -X POST http://127.0.0.1:8080/api/v1/adaptive-fusion/completions \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"prompt":"Summarize the release checks","task_class":"docs","objective":"efficient","risk_level":"low"}'
```

The default response contains only output and usage. Set `"include_routing_metadata":true` only when candidate, policy, experiment, and observation identifiers are needed for operator diagnosis. Request prompt, output, transcript, metadata, secrets, repository content, and private paths are not persisted in adaptive observations.

The dashboard Adaptive Fusion tab exposes the same guarded completion endpoint for operator testing. Use the completion test panel only after the provider registry, auth, cost ceilings, and kill procedure are configured. Routing metadata stays hidden unless the operator enables it for that request. The gate panel is read-only and shows provider/adaptive/auth/default-routing gates, experiment and promotion state, pause/kill switches, active policy count, and rollback snapshot count.

With a ready trusted-local profile, controlled experiments and auto promotion are active through the same validated profile. To operate them independently without the profile, use both legacy experiment gates:

```bash
export ACP_ENABLE_ADAPTIVE_EXPERIMENTS=1
export ACP_ADAPTIVE_EXPERIMENTS_ACTIVE=1
```

Experiments are deterministic and assign no traffic while either gate is off. With both gates enabled, the default traffic fraction is 1% and values above 5% are rejected. Risk, cost, token, call, elapsed-time, concurrency, pause, and kill controls still apply. Pause with `ACP_ADAPTIVE_EXPERIMENTS_PAUSED=1`; stop with `ACP_ADAPTIVE_EXPERIMENTS_KILL_SWITCH=1`.

For independent automatic promotion without the profile, use both legacy promotion gates:

```bash
export ACP_ENABLE_ADAPTIVE_AUTO_PROMOTION=1
export ACP_ADAPTIVE_AUTO_PROMOTION_ACTIVE=1
```

Promotion remains blocked until configured sample, confidence, quality, cost, latency, failure-rate, and evidence-freshness guards pass. Each activation stores a hash-bound snapshot and previous policy hash for rollback. Stop it with `ACP_ADAPTIVE_AUTO_PROMOTION_KILL_SWITCH=1`.

The trusted-local profile delegates eligible ordinary `/api/v1/dispatch` requests to adaptive completion routing. Without the profile, enable delegation independently with:

```bash
export ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING=1
```

Do not activate the profile or independent gate until completion routing has been validated with the intended endpoint registry, auth, cost limits, experiment state, promotion state, and kill procedure.

Emergency startup stop:

```bash
export ACP_ADAPTIVE_FUSION_KILL_SWITCH=1
```

This blocks adaptive calls without deconfiguring trusted-local readiness. Clearing/resetting the runtime kill allows controlled recovery without rebuilding the executor. A runtime kill handle also stops subsequent panel/fallback/final stages; an already-running provider call drains or cancels under its remaining timeout.

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

Produces `dist/agent-control-plane-v<VERSION>-x86_64-unknown-linux-gnu.tar.gz` with the engine binary, dashboard, install script, and upgrade script.

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
| `ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION` | (off) | Enables explicit bounded `adaptive_provider` workflow ticks |
| `ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON` | (none) | Up to eight fixed provider/model endpoint definitions with credential env references |
| `ACP_ADAPTIVE_FUSION_KILL_SWITCH` | (off) | Blocks adaptive provider execution when set to `1` at startup |
| `ACP_ADAPTIVE_COMPLETION_MAX_COST_USD` | `1.0` | Per adaptive completion candidate cost ceiling |
| `ACP_ADAPTIVE_COMPLETION_MAX_TOKENS` | `32768` | Per adaptive completion aggregate token ceiling |
| `ACP_ADAPTIVE_COMPLETION_MAX_LATENCY_MS` | `300000` | Per adaptive completion elapsed-time ceiling |
| `ACP_ENABLE_ADAPTIVE_EXPERIMENTS` | (off) | First gate for deterministic online candidate experiments |
| `ACP_ADAPTIVE_EXPERIMENTS_ACTIVE` | (off) | Second gate for online experiments |
| `ACP_ADAPTIVE_EXPERIMENT_TRAFFIC_RATE` | `0.01` | Experiment traffic fraction; values above `0.05` are rejected |
| `ACP_ADAPTIVE_EXPERIMENTS_PAUSED` | (off) | Temporarily blocks new experiment assignment |
| `ACP_ADAPTIVE_EXPERIMENTS_KILL_SWITCH` | (off) | Emergency stop for experiment assignment |
| `ACP_ENABLE_ADAPTIVE_AUTO_PROMOTION` | (off) | First gate for evidence-driven automatic promotion |
| `ACP_ADAPTIVE_AUTO_PROMOTION_ACTIVE` | (off) | Second gate for automatic promotion |
| `ACP_ADAPTIVE_AUTO_PROMOTION_KILL_SWITCH` | (off) | Emergency stop for automatic promotion |
| `ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING` | (off) | Delegates ordinary dispatch requests to adaptive completion routing |
| `ACP_ENABLE_ADAPTIVE_POLICY_PROMOTION` | (off) | Enables AF-4 adaptive policy promotion API gate |
| `ACP_ADAPTIVE_POLICY_PROMOTION_ACTIVE` | (off) | Second AF-4 promotion activation gate; human confirmation and `team:admin` still required |
| `ACP_ENABLE_ADAPTIVE_EXPLORATION` | (off) | Enables AF-4 bounded exploration gate |
| `ACP_ADAPTIVE_EXPLORATION_ACTIVE` | (off) | Second AF-4 exploration activation gate; high/critical-risk tasks remain excluded |
| `ACP_ADAPTIVE_EXPLORATION_KILL_SWITCH` | (off) | Blocks AF-4 exploration assignment when set to `1` |
| `ACP_ENABLE_CLI_EXECUTION` | `1` | Set to `0` to disable local Claude/Codex CLI discovery |
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
