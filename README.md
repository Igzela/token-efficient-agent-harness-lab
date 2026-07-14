# Token-Efficient Agent Harness Lab

[![CI](https://github.com/Igzela/token-efficient-agent-harness-lab/actions/workflows/tests.yml/badge.svg)](https://github.com/Igzela/token-efficient-agent-harness-lab/actions/workflows/tests.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-1654%20passing-brightgreen.svg)](#how-to-run-tests)
[![Site](https://img.shields.io/badge/site-landing%20page-0ea5e9)](https://token-efficient-agent-harness-lab.vercel.app)

**Agent workflows you can measure, pause, and prove.**

Self-hosted control plane for token-efficient, event-sourced agent systems: deterministic budget evidence, regression scorecards, adaptive routing, audited dispatch, and a local operator dashboard that fails closed instead of inventing success.

| | |
|---|---|
| **Landing page** | [token-efficient-agent-harness-lab.vercel.app](https://token-efficient-agent-harness-lab.vercel.app) |
| **Engine** | Rust · axum · SQLite (PostgreSQL optional) |
| **Surfaces** | Dashboard · TypeScript SDK · Python SDK |
| **Posture** | Local-first research lab · MIT · not a cloud SaaS |

### Why this exists

Most agent demos optimize the first happy path. This lab optimizes the **evidence trail**:

- **Token efficiency regression (PE-1)** — hash-bound scorecards you can recompute offline
- **Budget intelligence (PE-2)** — forecasts, explainable anomalies, policy-gated auto-pause
- **Operator decision center (PE-3)** — derived decision queues from posted evidence
- **Adaptive fusion + trusted-local execution** — bounded multi-model routing behind explicit gates
- **Real repository output (V2)** — audited patch/PR production against real git targets

> **Boundary:** local / small-team research tool — not multi-tenant SaaS, not a free provider proxy, no container/VM isolation claim. Provider and adaptive paths use the fail-closed IAE trusted-local profile (legacy explicit gates remain for compatibility).

Architecture, data flows, API surface, and safety boundaries: [`docs/ARCHITECTURE_BOOK.md`](docs/ARCHITECTURE_BOOK.md) · product roadmap packets: [`docs/NEXT_DECISION.md`](docs/NEXT_DECISION.md).

## GPT Web Repository Agent

The intended user interface is ordinary language in GPT Web. The user should not need to remember workflow names or manually provide Issue numbers, dispatch IDs, PR numbers, head SHAs, CI run IDs, or retry counters.

A normal request can be:

> Use the repository agent to implement this task. Keep the scope narrow and auto-merge off, review the resulting PR, and ask before merging.

The GPT Web assistant owns the internal translation:

1. refresh actual repository, CI, runner, and control state;
2. create one bounded Agent Task Issue with measurable acceptance criteria and an exact `agent-orchestrator-scope:v1` allowed-path list;
3. activate only the orchestrator authority required for the task while leaving auto-merge disabled by default;
4. observe Vader Codex execution, validated artifact finalization, branch/PR binding, exact-head CI, and independent review;
5. inspect the final diff and evidence and merge only under the user's explicit authority;
6. restore emergency stop on scope drift, credential exposure, contradictory state, duplicate dispatch, stale binding, or unexpected mutation.

**Current restriction:** this interface is documented but not yet accepted for production repository tasks. Live smoke Issue #217 reached intake, dispatcher claim, and the Vader worker, then ended `agent-blocked` before creating a branch or PR. Issue #208 is emergency-stopped and both enable labels are absent. `PR207-SMOKE-REPAIR-1` must diagnose and repair the worker failure, and `PR207-SMOKE-VERIFY-1` must complete a replacement smoke through PR creation, exact-head CI, and independent review before normal use resumes.

Machine-facing behavior is normative in [`AGENTS.md`](AGENTS.md); current evidence and restrictions are in [`docs/CURRENT_STATUS.md`](docs/CURRENT_STATUS.md); the repair sequence is in [`docs/NEXT_DECISION.md`](docs/NEXT_DECISION.md); operator recovery details remain in [`docs/RUNBOOK.md`](docs/RUNBOOK.md).

## Quick Start

```bash
git clone https://github.com/Igzela/token-efficient-agent-harness-lab.git
cd token-efficient-agent-harness-lab
cargo build -p engine

cd dashboard && bun install --frozen-lockfile && bun run build:static && cd ..
ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine
# Open http://127.0.0.1:8080
```

**Prerequisites:** Rust stable · [Bun](https://bun.sh/) · Python 3.10+ with [uv](https://docs.astral.sh/uv/) (scripts). See [CONTRIBUTING.md](CONTRIBUTING.md).

### Real output pilots (optional proof)

End-to-end verified branches across disposable Python, Rust, and Node repositories:

```bash
scripts/real_output_pilots.py
```

Uses an authenticated local Claude CLI, runs each repo’s tests, records approval, and pushes `acp/*` branches to local bare remotes without moving target `main`.

## Installation

### Option 1: Verified pre-built binary

Choose an exact release tag and its 40-hex source commit from
[GitHub Releases](https://github.com/Igzela/token-efficient-agent-harness-lab/releases).
Download the installer and its release-distributed SLSA bundle, verify those
exact local bytes, and only then execute the installer:

```bash
VERSION='<EXACT_VERSION_TAG>'
SOURCE_COMMIT='<40_HEX_RELEASE_COMMIT>'
BASE="https://github.com/Igzela/token-efficient-agent-harness-lab/releases/download/${VERSION}"
curl -fLO "${BASE}/install-from-release.sh"
curl -fLO "${BASE}/install-from-release.sh.slsa.bundle.json"
gh attestation verify ./install-from-release.sh \
  --bundle ./install-from-release.sh.slsa.bundle.json \
  --predicate-type https://slsa.dev/provenance/v1 \
  --repo Igzela/token-efficient-agent-harness-lab \
  --signer-workflow Igzela/token-efficient-agent-harness-lab/.github/workflows/release.yml \
  --source-ref "refs/tags/${VERSION}" \
  --source-digest "${SOURCE_COMMIT}" \
  --cert-oidc-issuer https://token.actions.githubusercontent.com \
  --deny-self-hosted-runners
bash ./install-from-release.sh \
  --version "${VERSION}" \
  --source-commit "${SOURCE_COMMIT}" \
  --bootstrap-bundle ./install-from-release.sh.slsa.bundle.json
```

The bootstrap verifies the separately attested verifier and all three exact
archive bundles before bounded extraction. It does not support `latest`, a
branch URL, stdin, or `curl | bash`. See [the runbook](docs/RUNBOOK.md#release-upgrade-and-rollback)
for the trust boundary and rollback procedure.

Available targets: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`.

### Option 2: Local development container

```bash
git clone https://github.com/Igzela/token-efficient-agent-harness-lab.git
cd token-efficient-agent-harness-lab
docker compose --profile combined up --build -d
```

This source-built container path is for local development and does not claim
the production release-attestation boundary above.

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

Provider API execution requires explicit endpoint/auth/budget configuration; CI uses stub/mock paths and does not call real provider APIs. A ready trusted-local profile activates bounded provider execution, adaptive routing, experiments, promotion, default routing, and acknowledged task advancement for internal local operation. Managed CLI execution is default-off and can register only the Codex `workspace-write` adapter after explicit enablement, authentication, exact app-owned workspace binding, policy authorization, and an inactive kill switch. Claude Code remains unavailable because its nested tools cannot be mediated by that boundary. The local dashboard remains guarded; dangerous actions require confirmation and audit logging.

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

Python SDK is retained for compatibility:

```python
from agent_control_plane_sdk import AgentControlPlaneClient

client = AgentControlPlaneClient("http://127.0.0.1:8080")
dashboard = client.dashboard()
recent_docs_runs = client.dispatches(limit=25, search="docs")
bundle = client.dispatch("Summarize docs without provider calls")
```

## Safety Boundaries

- No real model calls without explicit endpoint, credential, auth, pricing, and budget configuration; the trusted-local profile fails closed when any prerequisite is missing.
- No unbounded unattended agents; bounded trusted-local task advancement can run after readiness validation and explicit acknowledgement, while standalone legacy gates remain available.
- No real sandbox/process/container/VM isolation runtime; V2-1 is limited to app-owned workspace confinement unless separately approved.
- Supervised patch execution remains app-owned and gated. V2-3 adds an optional controlled git worktree plus approval-bound patch export or `acp/*` branch push; it does not modify the registered target working tree or `main`.
- Managed CLI execution is default-off and requires `ACP_ENABLE_CLI_EXECUTION=1`, authenticated operation, an exact app-owned workspace binding, policy approval/receipt enforcement, and an inactive `ACP_CLI_EXECUTION_KILL_SWITCH`. Only the Codex `workspace-write` adapter is registered. Claude Code remains explicitly unavailable because its nested Edit/Write/Bash tools cannot yet be confined or mediated by the app-owned policy boundary.
- `ACP_EXECUTION_MODE` controls only the legacy direct-dispatch surface: `off` (default) or separately gated `provider`. Direct `cli` and `auto` modes are retired and fail at startup. Provider/CLI hybrid workflow execution is owned by the Rust scheduler through `ACP_SCHEDULER_EXECUTOR=auto` or `pool`, where exact provider nodes retain their cost gates and policy-wrapped CLI-capable workflow nodes retain leases, concurrency, timeouts, approvals, receipts, and audit. A CLI route additionally requires an exact app-owned workspace binding; it never falls back to the engine checkout. When the CLI does not report exact model-bound pricing, cost remains unavailable and is not valid budget evidence.
- Bounded supervised worker concurrency is implemented behind legacy dual scheduler/worker gates or the IAE-2 trusted-local task-advancement acknowledgement, with bounded worker count, pinned adaptive execution, authenticated pause/resume/kill controls, heartbeat, leases, and audit. Workers consume existing queued runs and do not create unbounded goals or loops.
- Provider failover/fusion exists only inside the bounded, authenticated Adaptive Fusion path, enabled by legacy gates or a ready trusted-local profile.
- Cloud SaaS, multi-tenant hosting, cloud production Web UI, hosted deployment, and remote SaaS service remain out of scope.
- Target-repository output is implemented behind `ACP_ENABLE_TARGET_REPO_OUTPUT=1`, `dispatch:execute`, explicit confirmation, approval/integrity/secret gates, remote allowlists, and `ACP_TARGET_REPO_OUTPUT_KILL_SWITCH=1`; direct target working-tree or `main` writes and apply/merge/deploy authority remain out of scope. IAE may change trusted-local provider defaults without changing target-output authority.
- The dashboard is a local operations console with guarded app-owned controls. V2-5 adds the product-level Mission Control output path over the guarded backend contract: create plan/run, tick, create workspace, capture patch, approve, export patch, or push an `acp/*` branch.
- Adaptive Fusion supports guarded candidate selection, bounded parallel-panel fusion, safe observations, controlled experiments, auto promotion, completion routing, policy evidence, and rollback. The IAE-1 profile can compose these gates after readiness validation; every path remains bounded, killable, audited, and redacted.
- IAE-3 exposes effective authority, spend/traffic/worker bounds, safe observation aggregates, redacted recent audit actions, confirmed scheduler pause/resume/kill, and existing policy rollback in the Adaptive Fusion operator surface without exposing model content or credentials.
- No destructive runtime filesystem behavior.
- V2 real capabilities require the phase plan in `docs/NEXT_DECISION.md`, explicit gates, audit events, tests, and rollback/kill paths.

## Repository Structure

```text
engine/                  Rust engine, dispatch runtime, storage, provider gates, and local axum API
codegen/                 Wire-contract type generation helpers
dashboard/               Next.js local agent-control-plane dashboard with static export support
deploy/                  Optional local Dockerfiles for API and dashboard
sdk/typescript/          TypeScript REST SDK package
sdk/python/              Python REST SDK package
wire_contract/v1/        Frozen dispatch JSON schemas for cross-language parity
tools/                   Security baseline checker, relocated utility tests
scripts/                 Verification, packaging, and smoke-test scripts
```

## Active Documentation

Daily agent work uses a small active set:

- [`docs/ARCHITECTURE_BOOK.md`](docs/ARCHITECTURE_BOOK.md) — architecture and safety boundaries
- [`docs/CURRENT_STATUS.md`](docs/CURRENT_STATUS.md) — current status, limits, and verification snapshot
- [`docs/NEXT_DECISION.md`](docs/NEXT_DECISION.md) — single forward plan
- [`docs/MODULE_MAP.md`](docs/MODULE_MAP.md) — source ownership and verification routing
- [`docs/REAL_WORLD_TESTING_PLAYBOOK.md`](docs/REAL_WORLD_TESTING_PLAYBOOK.md) — PR/CI/maintenance workflow
- [`docs/RUNBOOK.md`](docs/RUNBOOK.md) — operator procedures

Historical phase plans, closeouts, validation reports, and low-frequency reference docs are retained in release-tagged git history; `docs/archive/README.md` is the working-tree index.

## Agent Maintenance

Full Agent Autonomy Mode authorizes coding agents to propose, implement, test, review, merge, and iterate high-risk architecture work. This includes new architecture directions, authority-boundary changes, default execution/profile changes, auth/security redesign, database migrations, release/tag/deploy workflow changes, target-output authority changes, and superseding accepted decisions. Changes should remain repo-scoped, testable, observable, reviewable, and rollbackable. Only five hard stops remain: committing real secrets, falsifying test/CI evidence, intentionally hiding failures, removing rollback paths, or performing irreversible external destruction without a recovery path. Documentation-only corrections may be committed directly to `main`. Agents must run `scripts/check_agent_handoff.py` before committing.

R7 remains the architecture baseline. A later architecture direction must be explicitly documented, tested, observable, and rollbackable.

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions, code style, and PR guidelines.

## License

This project is licensed under the MIT License — see [LICENSE](LICENSE) for details.
