# Token-Efficient Agent Harness Lab

## What This Project Is

Token-Efficient Agent Harness Lab is a local deterministic harness for studying event-sourced agent workflow infrastructure from Stage 0 through Stage 4. It includes JSONL event validation, projections, project/task workflow primitives, quality gates, controlled intelligence stubs, and Stage 4 runtime-control abstractions.

Current status: Stage 0-4 complete, Harness App MVP0-MVP8 complete, Trials 0-3 closed, Dispatch Kernel Phases 1-6B-3 stable, Phase 7 SDK + DocGenerator implemented, Rust engine parity through the local axum API router implemented, Phase 5 codegen plus TypeScript/Python REST SDK packages implemented, Phase 6 dashboard implemented, Phase 7 local Docker deploy implemented, Phase 8 migration closeout recorded, native local runtime implemented, and local small-team productization now provides SQLite-backed dispatch history, config/team state, cost summaries, audit log, export, and confirmed local backup. Rust provider stack Stage 1 and Stage 2 provider audit/usage persistence are implemented as explicit env-gated beta paths. Security hardening complete (2089 Python tests, 1031 Rust test cases enumerated by `cargo test -p engine -- --list`).

**New sessions should start with [docs/SESSION_START_HERE.md](docs/SESSION_START_HERE.md).**

Coding agents may autonomously advance safe repository work inside the documented boundaries. They must keep `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`, and this README current after each commit-sized change, then run `python3 scripts/check_agent_handoff.py` before commit.

Local-team productization work is tracked in [docs/PRODUCTIZATION_PLAN.md](docs/PRODUCTIZATION_PLAN.md); keep that as the single roadmap instead of adding new roadmap documents.

## What This Project Is Not

This repository is not a cloud production SaaS or autonomous-agent runtime. It does not call real model providers by default, run real agents, isolate work in real sandboxes, spawn production concurrent workers, provide provider failover, write target repositories, or provide hosted deployment. OpenAI-compatible and Anthropic provider adapters exist behind explicit environment configuration for local beta use; CI uses stub/mock paths and does not call real provider APIs. The local dashboard reads app-owned state from the local engine; dangerous local admin API actions require explicit confirmation and audit logging.

## How To Run Tests

```bash
PYTHONPATH=src python3 -m unittest discover -s tests
cargo test -p engine
```

Current result: 2089 Python tests pass; 1041 Rust `engine` parity/component/API test cases are enumerated by `cargo test -p engine -- --list`, and `cargo test -p engine` passes.

## How To Run Without Docker

API only:

```bash
cargo run -p engine
```

API plus dashboard from the same Rust process:

```bash
cd dashboard && corepack pnpm install --frozen-lockfile && corepack pnpm build:static
cd ..
ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine
```

Then open `http://127.0.0.1:8080`. By default the engine creates app-owned local state at `.agent-control-plane/local-team.db` and local backups under `.agent-control-plane/backups/`. Docker remains available for optional local compose verification, but it is not required for local use.

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

`ACP_PROVIDER_TYPE=openai_compatible` and `ACP_PROVIDER_TYPE=anthropic` are present for local beta validation only. They require explicit provider environment configuration and should be paired with `ACP_REQUIRE_AUTH=1`, a local admin API key, and narrow network exposure. Do not commit provider credentials. Real provider execution remains default-off and is not used in CI.

## Local API Examples

```bash
curl http://127.0.0.1:8080/api/v1/health
curl http://127.0.0.1:8080/api/v1/dashboard
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

## SDK Examples

TypeScript:

```ts
import { AgentControlPlaneClient } from "@token-efficient-agent-harness/agent-control-plane-sdk";

const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080" });
const dashboard = await client.dashboard();
const bundle = await client.dispatch({
  raw_request: "Summarize docs without provider calls",
  request_source: "api",
});
```

Python:

```python
from agent_control_plane_sdk import AgentControlPlaneClient

client = AgentControlPlaneClient("http://127.0.0.1:8080")
dashboard = client.dashboard()
bundle = client.dispatch("Summarize docs without provider calls")
```

## How To Run The CLI

Example event validation command:

```bash
PYTHONPATH=src python3 -m harness_core.cli validate-events docs/stage0/events.jsonl
```

`docs/stage0/events.jsonl` intentionally contains a known bad line and is preserved as a validator fixture.

## Safety Boundaries

- No real model calls.
- No real agents.
- No real sandbox/process/container/VM isolation.
- No production concurrency or real concurrent workers.
- No provider failover.
- No cloud production Web UI, hosted deployment, or remote SaaS service.
- Local dashboard views remain non-executable and target repositories remain read-only.
- No destructive runtime filesystem behavior.

## Repository Structure

```text
src/harness_core/        Python harness modules
engine/                  Rust deterministic parity kernel, dispatch engine, and local axum API router
codegen/                 Wire-contract type generation helpers
dashboard/               Next.js local agent-control-plane dashboard with static export support
deploy/                  Optional local Dockerfiles for API and dashboard
sdk/typescript/          TypeScript REST SDK package
sdk/python/              Python REST SDK package
tests/                   Deterministic unit tests and fixtures
wire_contract/v1/        Frozen dispatch JSON schemas for Python/Rust parity
tests/integration/parity/ Stdlib parity runner for dispatch golden fixtures
docs/stage0/             Stage 0 architecture fixtures and task book data
docs/stage1/             Event store, validator, kernel, CLI, task-record docs
docs/stage2/             Quality runtime specs and acceptance
docs/stage3/             Controlled intelligence stub specs and acceptance
docs/stage4/             Runtime abstraction specs and acceptance
web/dashboard/           Local non-executable Harness app dashboard
docs/MODULE_MAP.md       Module-to-stage reference
docs/ROADMAP.md          Completed stages and optional future tracks
docs/TEST_MATRIX.md      Test coverage matrix
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

Keep the repo moving through the autonomous maintainer loop: repair verification drift, keep docs current, fix focused regressions, and harden the local small-team path when evidence identifies concrete gaps. Any work that adds cloud hosting, real model provider integration, real sandbox execution, target-repo mutation, hosted deployment, or real autonomous workers still requires explicit approval.

Python reference implementation remains in `src/harness_core/` until an explicit future removal or relocation decision is approved.
