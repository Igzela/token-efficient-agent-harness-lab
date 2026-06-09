# Token-Efficient Agent Harness Lab

[![CI](https://github.com/user/token-efficient-agent-harness-lab/actions/workflows/tests.yml/badge.svg)](https://github.com/user/token-efficient-agent-harness-lab/actions/workflows/tests.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-1379%20passing-brightgreen.svg)](#running-tests)

A local deterministic harness and self-hosted macro-orchestrator control plane for studying event-sourced agent workflow infrastructure. Includes a Rust engine with axum API, SQLite state, TypeScript dashboard and SDK, and Python SDK.

> **This is a local research tool, not a cloud SaaS.** It does not call real model providers by default, run autonomous agents, or isolate work in sandboxes.

## Quick Start

```bash
# Clone and build
git clone https://github.com/user/token-efficient-agent-harness-lab.git
cd token-efficient-agent-harness-lab
cargo build -p engine

# Build and serve dashboard
cd dashboard && bun install --frozen-lockfile && bun run build:static && cd ..
ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine
# Open http://127.0.0.1:8080
```

**Prerequisites:** Rust stable toolchain, [Bun](https://bun.sh/) (for dashboard), Python 3.10+ with [uv](https://docs.astral.sh/uv/) (for scripts).

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed setup instructions.

## What This Project Is Not

## What This Project Is Not

This repository is not a cloud production SaaS or autonomous-agent runtime. It does not call real model providers by default, run real agents, isolate work in real sandboxes, spawn production concurrent workers, provide provider failover, write target repositories, or provide hosted deployment. OpenAI-compatible and Anthropic provider adapters exist behind explicit environment configuration for local beta use; CI uses stub/mock paths and does not call real provider APIs. The local dashboard reads app-owned state from the local engine; dangerous local admin API actions require explicit confirmation and audit logging.

## Toolchain

| Layer | Tool | Notes |
|---|---|---|
| Node | `.node-version` = 22 | fnm-friendly, not mandatory; CI uses `oven-sh/setup-bun@v2` |
| JS package manager | **Bun** | Required for dashboard and TypeScript SDK verification |
| Python runtime | **uv** | `uv run --no-project python ...` for all local Python commands |
| Python packaging | setuptools | `sdk/python/`; no `uv.lock` |
| Rust | stable toolchain | `cargo test -p engine`, `cargo fmt`, `cargo clippy` |

## How To Verify The Rust + TypeScript Stack

```bash
bash scripts/verify_rust_typescript_stack.sh
```

This is the primary cutover verification. It runs `scripts/check_wire_codegen_drift.sh`, checks Rust formatting, clippy, Rust tests, TypeScript SDK tests/build, dashboard lint/typecheck/build/static export, then starts the Rust engine with the exported dashboard and smokes `/api/v1/health`, `/api/v1/dashboard`, `/api/v1/dispatch`, dispatch/audit search, the structured backup auth boundary, and the dashboard root.

## How To Run Tests

```bash
cargo test -p engine
cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests
```

Current result: 1379 Rust tests pass. Python SDK tests run separately under `sdk/python/`.

## How To Run Without Docker

API only:

```bash
cargo run -p engine
```

API plus dashboard from the same Rust process:

```bash
cd dashboard && bun install --frozen-lockfile && bun run build:static
cd ..
ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine
```

Then open `http://127.0.0.1:8080`. By default the engine creates app-owned local state at `.agent-control-plane/local-team.db` and local backups under `.agent-control-plane/backups/`. Docker remains available for optional local compose verification, but it is not required for local use.

Check local setup readiness:

```bash
uv run --no-project python scripts/acp_local_doctor.py
```

Generate a local protected-mode admin key and startup command:

```bash
uv run --no-project python scripts/bootstrap_local_auth.py
```

Custom local paths:

```bash
ACP_DB_PATH=/tmp/acp/local-team.db \
ACP_BACKUP_DIR=/tmp/acp/backups \
ACP_DASHBOARD_DIR=dashboard/out \
cargo run -p engine
```

Optional local API key boundary:

```bash
ACP_REQUIRE_AUTH=1 \
ACP_ADMIN_API_KEY=<local-harness-key> \
ACP_DASHBOARD_DIR=dashboard/out \
cargo run -p engine
```

`<local-harness-key>` must use the local `harness_` plus 64 hex characters shape. Do not commit real keys; the key is read from the environment only. Without `ACP_REQUIRE_AUTH`, the default local loopback mode remains open for single-machine first run.

Optional provider adapter beta path:

```bash
ACP_PROVIDER_TYPE=stub \
ACP_DASHBOARD_DIR=dashboard/out \
cargo run -p engine
```

`ACP_PROVIDER_TYPE=openai_compatible` and `ACP_PROVIDER_TYPE=anthropic` are present for local beta validation only. They require `ACP_ENABLE_PROVIDER_EXECUTION=1`, explicit provider environment configuration, `ACP_REQUIRE_AUTH=1`, a local admin API key, and narrow network exposure. Do not commit provider credentials. Real provider execution remains default-off and is not used in CI.

Production-like local beta profile:

```bash
cp .env.production-like.local.example .env.production-like.local
uv run --no-project python scripts/bootstrap_local_auth.py --json
# Fill ACP_ADMIN_API_KEY in .env.production-like.local and export the provider secret locally.
export ACP_CN_ANTHROPIC_API_KEY=<provider-secret>
scripts/start_production_like_local.sh
```

This profile keeps auth, cost caps, audit, backups, and explicit provider execution enabled for local beta trials. Optional `ACP_PROVIDER_INPUT_COST_PER_1K_USD` and `ACP_PROVIDER_OUTPUT_COST_PER_1K_USD` values make `estimated_cost_usd` visible; without them the API/dashboard report `pricing_configured=false` instead of implying a real zero cost. It is still local-only and is not a cloud production deployment.

PostgreSQL integration tests (requires a running PostgreSQL instance):

```bash
ACP_TEST_DATABASE_URL=postgres://user:pass@localhost:5432/testdb cargo test -p engine --features pg-tests
```

Operational checks:

```bash
uv run --no-project python scripts/acp_ops_check.py --token "$ACP_ADMIN_API_KEY"
uv run --no-project python scripts/acp_restore_smoke.py --token "$ACP_ADMIN_API_KEY"
uv run --no-project python scripts/acp_secret_scan.py
```

`acp_restore_smoke.py` creates a local backup, verifies checksum/integrity, and runs restore dry-run by default. Real restore requires `--execute-restore --confirm-execute-restore`.

## Local API Examples

```bash
curl http://127.0.0.1:8080/api/v1/health
curl http://127.0.0.1:8080/api/v1/dashboard
curl 'http://127.0.0.1:8080/api/v1/dispatches?limit=25&search=docs'
curl http://127.0.0.1:8080/api/v1/export
curl -X POST http://127.0.0.1:8080/api/v1/dispatch \
  -H 'content-type: application/json' \
  -d '{"raw_request":"Summarize docs without provider calls","request_source":"api"}'
```

Confirmed local backup requires local auth to be enabled and an admin key:

```bash
curl -X POST http://127.0.0.1:8080/api/v1/backups \
  -H 'content-type: application/json' \
  -H "authorization: $(printf 'Bearer %s' "$ACP_ADMIN_API_KEY")" \
  -d '{"label":"manual","confirm_local_backup":true}'
```

Verify a backup and dry-run restore without modifying the live local database:

```bash
curl -H "authorization: $(printf 'Bearer %s' "$ACP_ADMIN_API_KEY")" \
  http://127.0.0.1:8080/api/v1/backups/backup-0001/verify

curl -X POST http://127.0.0.1:8080/api/v1/backups/backup-0001/restore/dry-run \
  -H 'content-type: application/json' \
  -H "authorization: $(printf 'Bearer %s' "$ACP_ADMIN_API_KEY")" \
  -d '{"confirm_restore_dry_run":true}'
```

## TypeScript SDK Example

```ts
import { AgentControlPlaneClient } from "@token-efficient-agent-harness/agent-control-plane-sdk";

const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080" });
const dashboard = await client.dashboard();
const recentDocsRuns = await client.dispatches({ limit: 25, search: "docs" });
const bundle = await client.dispatch({
  raw_request: "Summarize docs without provider calls",
  request_source: "api",
});
```

Python SDK is retained for compatibility:

```python
from agent_control_plane_sdk import AgentControlPlaneClient

client = AgentControlPlaneClient("http://127.0.0.1:8080")
dashboard = client.dashboard()
recent_docs_runs = client.dispatches(limit=25, search="docs")
bundle = client.dispatch("Summarize docs without provider calls")
```

## Safety Boundaries

- No real model calls by default; the local beta provider path remains explicit and env-gated.
- No unattended real agents; explicit supervised local workflow execution exists behind opt-in gates.
- No real sandbox/process/container/VM isolation runtime.
- Supervised patch execution is limited to app-owned detached workspaces, explicit workflow tick/executor selection, artifact capture, approval binding, and export gating. It is not target-repo mutation or production autonomy.
- Existing local CLI executor subprocess invocation is a separate, explicit opt-in exception via `ACP_ENABLE_CLI_EXECUTION=1`.
- No production concurrency or real concurrent workers.
- No provider failover.
- No cloud production Web UI, hosted deployment, or remote SaaS service.
- Dashboard/SDK controls operate on app-owned workflow/workspace/artifact state; target repositories remain read-only.
- No destructive runtime filesystem behavior.

## Repository Structure

```text
engine/                  Rust deterministic kernel, dispatch engine, storage, provider gates, and local axum API
codegen/                 Wire-contract type generation helpers
dashboard/               Next.js local agent-control-plane dashboard with static export support
deploy/                  Optional local Dockerfiles for API and dashboard
sdk/typescript/          TypeScript REST SDK package
sdk/python/              Python REST SDK package
wire_contract/v1/        Frozen dispatch JSON schemas for cross-language parity
tools/                   Security baseline checker, relocated utility tests
scripts/                 Verification, packaging, and smoke-test scripts
docs/stage0/             Stage 0 architecture fixtures and task book data
docs/stage1/             Event store, validator, kernel, CLI, task-record docs
docs/stage2/             Quality runtime specs and acceptance
docs/stage3/             Controlled intelligence stub specs and acceptance
docs/stage4/             Runtime abstraction specs and acceptance
web/dashboard/           Local non-executable Harness app dashboard
docs/MODULE_MAP.md       Module-to-stage reference
docs/AGENT_CONTROL_PLANE_MIGRATION_CLOSEOUT.md Agent-control-plane migration closeout
```

## Stage Summary

- Stage 0: architecture, fixtures, task packs, and known validator issue.
- Stage 1: Event Store, JSONL Validator, projections, kernel, CLI, task records.
- Stage 2: scoring, gates, evaluation, baselines, trajectory, quality digest.
- Stage 3: advisor/model gateway stubs, routing, controlled eval, sampling, skills.
- Stage 4: DAG mutation, sandbox claims, scheduling, checkpoint/recovery planning, artifact lifecycle, health, dashboard data model.

## Harness App MVPs

- MVP0: read-only harness instance auditor.
- MVP1: static audit dashboard.
- MVP2: local read-only control plane.
- MVP3: deterministic non-executable planning kernel.
- MVP4: read-only plan review workbench for plan history, summary, comparison, and advisory review actions.
- MVP5: non-persistent review guidance preview for stored plans, evidence requirements, and token-efficiency guidance.
- MVP6: read-only planning portfolio triage for review priority, bottlenecks, and token hotspots.
- MVP7: read-only operations and debug dashboard for component status, data flow, storage health, recent errors, and debug actions.
- MVP8: operations console simplification that keeps the first screen focused on status, health, errors, and two primary actions while moving tools into collapsed sections.

Demo packaging: [`docs/demo/README.md`](docs/demo/README.md)

## CA-7 Sealed Baseline Status

Controlled Adaptive Orchestrator Kernel minimum threshold reached (CA-0 through CA-7 all passed). The current harness policy baseline is sealed. Future policy changes require the policy candidate lifecycle and governance approval path.

Full closeout report: [`docs/CA7_CONTROLLED_ADAPTIVE_CLOSEOUT_REPORT.md`](docs/CA7_CONTROLLED_ADAPTIVE_CLOSEOUT_REPORT.md)

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions, code style, and PR guidelines.

## License

This project is licensed under the MIT License — see [LICENSE](LICENSE) for details.
