# Token-Efficient Agent Harness Lab

## What This Project Is

Token-Efficient Agent Harness Lab is a local deterministic harness for studying event-sourced agent workflow infrastructure from Stage 0 through Stage 4. It includes JSONL event validation, projections, project/task workflow primitives, quality gates, controlled intelligence stubs, and Stage 4 runtime-control abstractions.

Current status: Stage 0-4 complete, Harness App MVP0-MVP8 complete, Trials 0-5 closed, and the agent-control-plane cutover is complete for the Rust + TypeScript stack. The primary local runtime is Rust `engine/` with axum API, SQLite state, provider safety gates, permission governance, cost governance, data operations, native packaging, dashboard controls, production-like local beta operations checks, a read-only planner API that stores non-executable `WorkflowGraph` plans with recommendation-only advisory metadata, inert durable workflow run/node/edge/event/approval metadata, and Batch 7 Slice A/B/C/D supervised patch workspace/artifact metadata in app-owned SQLite plus read-only HTTP/SDK visibility and docs-only approval-binding design. Supervised execution runtime remains NO-GO: no workspace directory creation, patch generation, approval/export gate implementation, export runtime, target writes, sandbox/process/container/VM execution, workers, provider calls, or apply/push/merge/deploy controls. The primary UI and SDK surface is TypeScript (`dashboard/` and `sdk/typescript/`). Python is retained as the Python REST SDK and utility scripts only; the legacy Python reference implementation has been retired. Security hardening complete (1204 Rust tests pass).

**New sessions should start with [docs/SESSION_START_HERE.md](docs/SESSION_START_HERE.md).**

Coding agents may autonomously advance safe repository work inside the documented boundaries. They must keep the smallest necessary handoff surface current after each commit-sized change, then run `uv run --no-project python scripts/check_agent_handoff.py` before commit.

Local-team productization work is tracked in [docs/NEXT_DECISION.md](docs/NEXT_DECISION.md); do not add parallel roadmap documents.

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

Current result: 1204 Rust tests pass. Python SDK tests run separately under `sdk/python/`.

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
- No real agents.
- No real sandbox/process/container/VM isolation runtime.
- Batch 6 sandbox/workspace/approval/rollback/artifact-capture contracts plus Batch 7 Slice A/B/C/D supervised patch metadata storage, read-only HTTP/SDK visibility, and approval-binding design are not runtime execution capability.
- Existing local CLI executor subprocess invocation is a separate, explicit opt-in exception via `ACP_ENABLE_CLI_EXECUTION=1`.
- No production concurrency or real concurrent workers.
- No provider failover.
- No cloud production Web UI, hosted deployment, or remote SaaS service.
- Local dashboard views remain non-executable and target repositories remain read-only.
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

## Next Recommended Work

Keep the repo moving through the autonomous maintainer loop: repair CI/docs/test drift, maintain wire governance, keep docs current, and fix focused regressions. The R-series is sealed at R7. R8 is not approved. No further file splitting is approved. Supervised autonomous beta Batch 7 Slice A/B/C/D is app-owned metadata plus read-only HTTP/SDK visibility and approval-binding design; runtime remains NO-GO. Any work that adds cloud hosting, broadens model provider integration, adds sandbox isolation, expands subprocess execution beyond the existing CLI executor path, mutates target repos, adds workspace creation, patch generation, hosted deployment, wires approval/run/deploy/merge/apply/run controls, or adds real autonomous workers still requires explicit approval.

Python legacy reference implementation has been retired. Python is now retained only as the REST SDK (`sdk/python/`) and utility scripts (`scripts/`, `tools/`, `codegen/`).
