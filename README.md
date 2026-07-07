# Token-Efficient Agent Harness Lab

[![CI](https://github.com/Igzela/token-efficient-agent-harness-lab/actions/workflows/tests.yml/badge.svg)](https://github.com/Igzela/token-efficient-agent-harness-lab/actions/workflows/tests.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-1654%20passing-brightgreen.svg)](#running-tests)

A local deterministic harness and self-hosted macro-orchestrator control plane for studying event-sourced agent workflow infrastructure. Includes a Rust engine with axum API, SQLite state, TypeScript dashboard and SDK, and Python SDK.

> **This is a local research tool, not a cloud SaaS.** Provider and adaptive execution use the fail-closed IAE trusted-local profile for bounded internal operation; legacy explicit gates remain available for compatibility. No container/VM isolation is provided.

For the full system architecture, data flows, API surface, and safety boundaries, see [`docs/ARCHITECTURE_BOOK.md`](docs/ARCHITECTURE_BOOK.md).

## Quick Start

### Real output pilots: produce verified branches

For an end-to-end proof across Python, Rust, and Node repositories:

```bash
scripts/real_output_pilots.py
```

The script uses an authenticated local Claude CLI to modify three disposable real git repositories, runs each repository's tests, captures verification evidence, records approval, and pushes three distinct `acp/*` branches to local bare remotes. Every target `main` ref is checked before and after.

```bash
# Clone and build
git clone https://github.com/Igzela/token-efficient-agent-harness-lab.git
cd token-efficient-agent-harness-lab
cargo build -p engine

# Build and serve dashboard
cd dashboard && bun install --frozen-lockfile && bun run build:static && cd ..
ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine
# Open http://127.0.0.1:8080
```

**Prerequisites:** Rust stable toolchain, [Bun](https://bun.sh/) (for dashboard), Python 3.10+ with [uv](https://docs.astral.sh/uv/) (for scripts).

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed setup instructions.

## Installation

### Option 1: Pre-built Binary (easiest)

Download the latest release for your platform from [GitHub Releases](https://github.com/Igzela/token-efficient-agent-harness-lab/releases):

```bash
# Linux x86_64
curl -fLO https://github.com/Igzela/token-efficient-agent-harness-lab/releases/download/v0.1.0/agent-control-plane-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
tar -xzf agent-control-plane-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
sudo ./agent-control-plane-v0.1.0-x86_64-unknown-linux-gnu/install.sh
```

Or use the one-line installer:

```bash
curl -fsSL https://raw.githubusercontent.com/Igzela/token-efficient-agent-harness-lab/main/scripts/install-from-release.sh | bash
```

Available targets: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`.

### Option 2: Docker

```bash
# One command — engine + dashboard in a single container
docker run -d -p 8080:8080 -v acp-data:/data igzela/agent-control-plane:latest

# Or with docker compose
git clone https://github.com/Igzela/token-efficient-agent-harness-lab.git
cd token-efficient-agent-harness-lab
docker compose --profile combined up -d
```

### Option 3: From Source

```bash
git clone https://github.com/Igzela/token-efficient-agent-harness-lab.git
cd token-efficient-agent-harness-lab
cargo build --release -p engine
cd dashboard && bun install --frozen-lockfile && bun run build:static && cd ..
ACP_DASHBOARD_DIR=dashboard/out ./target/release/agent-control-plane
```

### Option 4: cargo install (from git)

```bash
cargo install --git https://github.com/Igzela/token-efficient-agent-harness-lab agent-control-plane
agent-control-plane
```

## What This Project Is Not

This repository is not a cloud production SaaS, hosted multi-tenant service, or direct-deploy tool. V2 provides auditable real-repository patch/PR production. IAE authorizes bounded trusted-local provider and autonomous task execution through the phase plan in `docs/NEXT_DECISION.md`.

Provider API execution requires explicit endpoint/auth/budget configuration; CI uses stub/mock paths and does not call real provider APIs. A ready trusted-local profile activates bounded provider execution, adaptive routing, experiments, promotion, default routing, and acknowledged task advancement for internal local operation. Installed local Claude/Codex CLIs are discovered by default, but execution still requires an explicit workflow action. The local dashboard remains guarded; dangerous actions require confirmation and audit logging.

High-risk provider-gated real experiment runners are allowed for local research when they follow [`docs/PROVIDER_GATED_EXPERIMENT_RUNNER.md`](docs/PROVIDER_GATED_EXPERIMENT_RUNNER.md): explicit opt-in, protected auth, budget ceilings, provider/model identity, pause/kill controls, redacted app-owned evidence, no provider calls in CI, and a rollback path.

## Toolchain

| Layer | Tool | Notes |
|---|---|
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

Current result: 1654 Rust tests pass. Python SDK tests run separately under `sdk/python/`.

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

`ACP_PROVIDER_TYPE=openai_compatible` and `ACP_PROVIDER_TYPE=anthropic` support guarded local execution. For internal local operation, use a ready `ACP_TRUSTED_LOCAL_PROFILE=1`; the legacy `ACP_ENABLE_PROVIDER_EXECUTION=1` gate remains available for compatibility. Both paths require explicit provider configuration, `ACP_REQUIRE_AUTH=1`, a local admin API key, positive cost caps, and narrow network exposure. Do not commit provider credentials. CI uses stub providers rather than paid endpoints. For single-provider execution, `ACP_MODEL` remains authoritative; when it is absent, the runtime reads the current project's Claude Code JSON config (`$HOME/.claude.json`, or `ACP_CLAUDE_CODE_CONFIG_PATH`) and uses its configured model or a safe recent model-usage key before falling back to `default`.

Adaptive single/fallback/fusion execution is activated by the trusted-local profile after readiness validation; `ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION=1` remains the standalone legacy gate. Configure up to eight fixed provider/model endpoints through `ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON` or the guarded dashboard/API; entries contain credential environment variable names, never credential values. Dashboard/API config applies to adaptive completion immediately and is restored for startup-bound execution after restart. Explicit endpoint JSON remains authoritative while present. Explicit workflow ticks remain supported, and AF-6 adds authenticated `POST /api/v1/adaptive-fusion/completions` with routing metadata hidden by default. Experiments, auto promotion, and default `/dispatch` delegation are composed by the ready trusted-local profile, or by their standalone legacy gates when operating without the profile. See [`docs/RUNBOOK.md`](docs/RUNBOOK.md).

Production-like local beta profile:

```bash
cp .env.production-like.local.example .env.production-like.local
uv run --no-project python scripts/bootstrap_local_auth.py --json
# Fill ACP_ADMIN_API_KEY in .env.production-like.local and export the provider secret locally.
export ACP_CN_ANTHROPIC_API_KEY=<provider-secret>
scripts/start_production_like_local.sh
```

This profile keeps auth, cost caps, audit, backups, and explicit provider execution enabled for local beta trials. Optional `ACP_PROVIDER_INPUT_COST_PER_1K_USD` and `ACP_PROVIDER_OUTPUT_COST_PER_1K_USD` values make `estimated_cost_usd` visible; without them the API/dashboard report `pricing_configured=false` instead of implying a real zero cost. It is still local-only and is not a cloud production deployment.
The example uses `ACP_TRUSTED_LOCAL_PROFILE=1` and a fixed adaptive endpoint registry so provider calls, adaptive routing, experiments, promotion, and default routing are activated as one validated local profile. `ACP_ENABLE_PROVIDER_EXECUTION=1` is still accepted as the standalone legacy provider gate when operating without trusted-local.

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
