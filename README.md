# Token-Efficient Agent Harness Lab

[![CI](https://github.com/Igzela/token-efficient-agent-harness-lab/actions/workflows/tests.yml/badge.svg)](https://github.com/Igzela/token-efficient-agent-harness-lab/actions/workflows/tests.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Site](https://img.shields.io/badge/site-landing%20page-0ea5e9)](https://token-efficient-agent-harness-lab.vercel.app)

**Evidence and safety infrastructure for coding-agent repository workflows.**

Prove that agent-generated patches, reviews, and CI results belong to the exact commit being shipped — with budgets, audit, recovery, and a local operator dashboard that fails closed instead of inventing success.

| | |
|---|---|
| **Landing page** | [token-efficient-agent-harness-lab.vercel.app](https://token-efficient-agent-harness-lab.vercel.app) |
| **Engine** | Rust package `engine` · binary `agent-control-plane` · axum · SQLite (PostgreSQL optional) |
| **Surfaces** | Dashboard · TypeScript SDK · Python SDK |
| **Posture** | Local-first research lab · MIT · not a cloud SaaS |
| **Status** | Active development. Verified capabilities and limits: [`docs/CURRENT_STATUS.md`](docs/CURRENT_STATUS.md) |

### The failure this prevents

A green CI check is not enough if it belongs to an older PR head. This lab binds dispatch, CI, review, and merge evidence to an exact commit and rejects stale identity.

### What the full control plane adds

- Deterministic budget evidence and regression scorecards
- Audited dispatch, pause, recovery, and operator decisions
- Bounded adaptive routing behind explicit fail-closed gates
- Supervised repository patch/PR output (never silent target-`main` writes)
- Local dashboard that refuses invented success

> **Boundary:** local / small-team research tool — not multi-tenant SaaS, not a free provider proxy, no container/VM isolation claim. Provider, managed-CLI, adaptive, and autonomous paths remain default-off and use explicit fail-closed authority, identity, budget, lease, audit, and kill controls. Current routing and eligibility are owned by `docs/NEXT_DECISION.md`; historical phase labels do not define the forward plan.

Maintainer and Agent entry: [`START_HERE.md`](START_HERE.md) · architecture: [`docs/ARCHITECTURE_BOOK.md`](docs/ARCHITECTURE_BOOK.md) · forward plan: [`docs/NEXT_DECISION.md`](docs/NEXT_DECISION.md) · implementation rules: [`AGENTS.md`](AGENTS.md) · support: [`SUPPORT.md`](SUPPORT.md).

## Quick Start

Verified on a source checkout with Rust stable, [Bun](https://bun.sh/), and [uv](https://docs.astral.sh/uv/) (Python 3.11+ for the SDK; 3.10+ scripts where noted). See [CONTRIBUTING.md](CONTRIBUTING.md).

### Five-minute no-provider demo (recommended first success)

No API key, no real provider, no target-repository writes. Builds the engine/dashboard if needed, runs a fixture dispatch, binds proof to the current git revision, and rejects a stale-head scenario:

```bash
git clone https://github.com/Igzela/token-efficient-agent-harness-lab.git
cd token-efficient-agent-harness-lab
./scripts/demo.sh
```

Leave the UI up, or clean a kept session:

```bash
./scripts/demo.sh --keep      # engine under .acp-demo-state/
./scripts/demo.sh --cleanup   # stop and remove .acp-demo-state/
```

### Exact-Head CI proof action (growth wedge)

Use only the fail-closed PR head check without installing the full control plane:

```yaml
- uses: Igzela/token-efficient-agent-harness-lab/actions/exact-head-check@main
  with:
    github-token: ${{ github.token }}
    pull-request: ${{ github.event.pull_request.number }}
    expected-head: ${{ github.event.pull_request.head.sha }}
```

Details: [`actions/exact-head-check/README.md`](actions/exact-head-check/README.md) · example: [`examples/github-actions/exact-head-check.yml`](examples/github-actions/exact-head-check.yml).

### Clean-environment validation (strangers / fresh machines)

Reproduce the public path from a clean checkout with disposable build artifacts, the no-provider demo, exact-revision proof, stale-head rejection, and Exact-Head Action offline self-validation. No API key, no provider call, no target-repository write:

```bash
./scripts/external_validation.sh
./scripts/external_validation.sh --report /tmp/external_validation_report.json
```

Hosted matrix (Ubuntu + macOS) is workflow `external-validation` (self-validation artifacts only — not external adoption evidence). Report schema: `external_validation_report.v1`. Feedback: [external validation form](https://github.com/Igzela/token-efficient-agent-harness-lab/issues/new?template=external_validation.yml).

### Manual loopback start

```bash
cargo build -p engine
cd dashboard && bun install --frozen-lockfile && bun run build:static && cd ..
ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine
# Open http://127.0.0.1:8080
```

Optional readiness check:

```bash
uv run --no-project python scripts/acp_local_doctor.py
```

### Adapter support (read carefully)

| Concept | Status |
|---|---|
| **Codex `workspace-write` managed CLI adapter** | Supported default-off adapter; auth, workspace binding, policy, and kill switch required |
| **Claude CLI pilot harness** | Currently unavailable while Claude admission is fail-closed; historical experimental script only, never managed-acceptance evidence |
| **Claude Code as managed runtime** | **Disabled / fail-closed** — exact identity and bounded process checks exist, but provider-independent worktree-only filesystem mediation is not proved; no managed model request is allowed |

Do not run or treat the pilot script as managed acceptance while Claude admission is fail-closed. Maintainer details: [`docs/RUNBOOK.md`](docs/RUNBOOK.md).

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

The Cargo **package** name is `engine`; the installed **binary** is `agent-control-plane`:

```bash
cargo install --git https://github.com/Igzela/token-efficient-agent-harness-lab \
  --locked engine --bin agent-control-plane
agent-control-plane
```

Static note: re-verify this install form on a clean machine before treating it as the primary public path; prefer Option 1 (attested release) or Option 3 (source build) when supply-chain evidence matters.

There is **no** verified public `docker run …:latest` image path. For containers, use the source-built compose path in Option 2 only.

## What This Project Is Not

This repository is not a cloud production SaaS, hosted multi-tenant service, or direct-deploy tool. Accepted real-repository patch/PR, trusted-local Provider, and bounded autonomous-task capabilities are described in `docs/CURRENT_STATUS.md`; their next eligible work and prerequisites are owned only by `docs/NEXT_DECISION.md`. Historical `V2` and `IAE` labels may remain in compatibility surfaces or Git history, but they are not current routing authorities.

Provider API execution requires explicit endpoint/auth/budget configuration; CI uses stub/mock paths and does not call real provider APIs. A ready trusted-local profile activates bounded provider execution, adaptive routing, experiments, promotion, default routing, and acknowledged task advancement for internal local operation. Managed CLI execution is default-off. Codex retains its existing `workspace-write` adapter. Claude Code registration is currently disabled because provider-independent worktree-only filesystem mediation is unproved; exact identity, model, usage, and process checks alone do not establish read confinement. The local dashboard remains guarded; dangerous actions require confirmation and audit logging.

Internal maintainer paths (repository-agent / Issue orchestrator, emergency stop, runner recovery) live in [`AGENTS.md`](AGENTS.md) and [`docs/CURRENT_STATUS.md`](docs/CURRENT_STATUS.md). They are not the public product entry.

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

Current test counts are reported by CI and release evidence, not by hard-coded badges in this README. Python SDK tests run separately under `sdk/python/` (Python 3.11+).

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
- No real sandbox/process/container/VM isolation runtime; filesystem mediation is limited to the accepted app-owned workspace boundaries unless separately approved and proved.
- Supervised patch execution remains app-owned and gated. The optional controlled git worktree plus approval-bound patch export or `acp/*` branch push does not modify the registered target working tree or `main`.
- Managed CLI execution is default-off and requires `ACP_ENABLE_CLI_EXECUTION=1`, authenticated operation, an exact app-owned workspace binding, policy approval/receipt enforcement, and an inactive `ACP_CLI_EXECUTION_KILL_SWITCH`. Codex uses the existing `workspace-write` adapter. Claude Code remains unavailable: the current binary lacks provider-independent worktree-only filesystem proof, and no environment/model setting may override that fail-closed decision. First-party credentials remain symbolic environment inputs only and are never copied into evidence.
- `ACP_EXECUTION_MODE` controls only the legacy direct-dispatch surface: `off` (default) or separately gated `provider`. Direct `cli` and `auto` modes are retired and fail at startup. Provider/CLI hybrid workflow execution is owned by the Rust scheduler through `ACP_SCHEDULER_EXECUTOR=auto` or `pool`, where exact provider nodes retain their cost gates and policy-wrapped CLI-capable workflow nodes retain leases, concurrency, timeouts, approvals, receipts, and audit. A CLI route additionally requires an exact app-owned workspace binding; it never falls back to the engine checkout. CLI-reported dollar estimates are not authoritative billing evidence; canonical cost stays unavailable unless an authoritative billing owner is bound.
- Bounded supervised worker concurrency is implemented behind explicit scheduler/worker gates or the trusted-local task-advancement acknowledgement, with bounded worker count, pinned adaptive execution, authenticated pause/resume/kill controls, heartbeat, leases, and audit. Workers consume existing queued runs and do not create unbounded goals or loops.
- Provider failover/fusion exists only inside the bounded, authenticated Adaptive Fusion path, enabled by legacy gates or a ready trusted-local profile.
- Cloud SaaS, multi-tenant hosting, cloud production Web UI, hosted deployment, and remote SaaS service remain out of scope.
- Target-repository output is implemented behind `ACP_ENABLE_TARGET_REPO_OUTPUT=1`, `dispatch:execute`, explicit confirmation, approval/integrity/secret gates, remote allowlists, and `ACP_TARGET_REPO_OUTPUT_KILL_SWITCH=1`; direct target working-tree or `main` writes and apply/merge/deploy authority remain out of scope. Trusted-local Provider defaults cannot change target-output authority.
- The dashboard is a local operations console with guarded app-owned controls. Its Mission Control output path uses the guarded backend contract to create plan/run, tick, create workspace, capture patch, approve, export patch, or push an `acp/*` branch.
- Adaptive Fusion supports guarded candidate selection, bounded parallel-panel fusion, safe observations, controlled experiments, auto promotion, completion routing, policy evidence, and rollback. A ready trusted-local profile may compose these gates only after readiness validation; every path remains bounded, killable, audited, and redacted.
- The Adaptive Fusion operator surface exposes effective authority, spend/traffic/worker bounds, safe observation aggregates, redacted recent audit actions, confirmed scheduler pause/resume/kill, and existing policy rollback without exposing model content or credentials.
- No destructive runtime filesystem behavior.
- Accepted capabilities require the explicit gates, audit events, tests, rollback/kill paths, and current packet eligibility in `docs/NEXT_DECISION.md`.

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

Repository maintenance authority is governed by [`START_HERE.md`](START_HERE.md), [`AGENTS.md`](AGENTS.md), and [`docs/REAL_WORLD_TESTING_PLAYBOOK.md`](docs/REAL_WORLD_TESTING_PLAYBOOK.md). Agents may plan, implement, test, review, repair CI, and manually merge eligible repository work only inside the current documented packet scope and after the required exact-head review, canonical CI, objection, compatibility, recovery, and rollback gates pass. A missing or conflicting architecture, authority, schema, security, evaluator, release, or recovery decision stops at `DECISION_REQUIRED`; an agent may not silently supersede it.

This maintenance authority does not grant a runtime, candidate, experiment, or generated plan Provider spend, target output, merge, release, deployment, or production-adoption authority. Documentation-only changes use a branch/PR by default; direct-to-`main` documentation requires explicit current authority plus handoff and diff validation. The complete hard-stop set is owned by the testing playbook—there is no five-stop shortcut. The durable architecture baseline is [`docs/ARCHITECTURE_BOOK.md`](docs/ARCHITECTURE_BOOK.md); historical phase labels remain in Git history rather than owning current direction.

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) for focused verification tiers (docs contributors are not required to run the full Rust matrix), [SUPPORT.md](SUPPORT.md) for help routing and response policy, and [CHANGELOG.md](CHANGELOG.md) / [CITATION.cff](CITATION.cff) for user-facing history and citation.

## License

This project is licensed under the MIT License — see [LICENSE](LICENSE) for details.
