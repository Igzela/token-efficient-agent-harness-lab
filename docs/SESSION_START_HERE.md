# Session Start Here

Read this file first in any new AI session on this repository.

## Project Identity

Token-Efficient Agent Harness Lab is a local deterministic harness for studying event-sourced agent workflow infrastructure.

## Current State

| Milestone | Status |
|---|---|
| Stage 0–4 | Complete |
| CA-7 sealed baseline | Complete |
| Harness App MVP0–MVP8 | Complete |
| Trial 0 (real target) | Closed — `PASS` verdict |
| Trial 1 (multi-task) | Closed — `ACCEPTABLE_WITH_NOTES`, hardened to `ACCEPTABLE_FOR_MULTI_TASK_TRIAL_AFTER_HARDENING` |
| Reliability Hardening 1 | Complete |
| Demo packaging | Complete |
| Demo verification | Complete — all docs accurate and runnable |
| Trial 2 candidate selection | Complete — hermes-gateway-lab onboarded |
| Trial 2 execution | Closed — `ACCEPTABLE_WITH_NOTES` (audit BLOCKED on target, generalization finding) |
| Trial 2 final verification | Closed — `TRIAL_2_FINAL_VERIFICATION_PASS` |
| Trial 3 multi-repo generalization | Closed — `TRIAL_3_MULTI_REPO_GENERALIZATION_PASS` |
| Trial 3 target merge | Closed — all 3 target PRs merged, audit PASS_WITH_NOTES |

Tests: 1140 Rust pass. Primary cutover verification is `bash scripts/verify_rust_typescript_stack.sh`.

## Toolchain

| Layer | Tool | Notes |
|---|---|---|
| Node version | fnm + `.node-version` = 22 | CI uses `oven-sh/setup-bun@v2` (Bun includes Node) |
| JS package manager | Bun | `bun.lock` replaces pnpm-lock.yaml; `bun install`, `bun run` |
| Python runtime | uv | `uv run --no-project python ...` for local commands; pure stdlib, no deps |
| Python packaging | setuptools | `sdk/python/`; no uv.lock |
| Rust | stable toolchain | `cargo test -p engine`, `cargo fmt`, `cargo clippy` |

Additional active architecture track:

| Track | Status |
|---|---|
| Dispatch Kernel Phase 1 | Stable |
| Dispatch Kernel Phase 2 | Stable |
| Dispatch Kernel Phase 3 — Provider Adapter Boundary | Stable, CA-7 compliant, no bundled real transport |
| Dispatch Kernel Phase 4 — Adaptive Routing | Stable |
| Dispatch Kernel Phase 5 — Multi-Agent Orchestration | Stable |
| Dispatch Kernel Phase 6A — Local Durable API/Storage | Stable |
| Dispatch Kernel Phase 6B+ | Eligible only when documented in the architecture book and kept inside repository-safe boundaries |
| Language Migration Phase 0 — Wire schemas/golden parity | Implemented |
| Language Migration Phase 1 — Rust parity kernel | Implemented for `event_schema`, `task_analyzer`, `dispatch_decision`; no provider/API/dashboard/deploy work |
| Language Migration Phase 2 — Rust dispatch engine parity | Implemented for selector, budget, noop executor abstraction, evaluator, ledger, and dispatch engine; no provider/API/dashboard/deploy work |
| Language Migration Rust engine/API parity | Implemented through local axum health/ready/openapi/dispatch router; no default real providers, workers, target writes, SDK publishing, or production deploy |
| Language Migration Phase 5 — SDK + codegen | Implemented codegen helper plus TypeScript/Python REST SDK packages; no SDK publishing |
| Language Migration Phase 6 — Read-only Dashboard | Implemented Next.js dashboard with dispatch, routing, agents/workflows, costs, settings, and health views; no executable controls |
| Language Migration Phase 7 — Local Docker Deploy | Implemented local compose stack for Rust API + dashboard; no production deploy |
| Language Migration Phase 8 — Closeout | Implemented; closeout recorded in `docs/AGENT_CONTROL_PLANE_MIGRATION_CLOSEOUT.md` |
| Agent-Control-Plane Native Local Runtime | Implemented; Rust engine can serve API plus static dashboard from one local process via `ACP_DASHBOARD_DIR=dashboard/out`; Docker is optional |
| Agent-Control-Plane Local Small-Team Productization | Implemented; Rust engine persists app-owned SQLite dispatch history/config/team/API-key metadata/audit/cost state, dashboard reads live local API state, SDKs cover local state endpoints, and export/confirmed backup are available without Docker |
| Rust Provider Stack Stage 1 + Stage 2 audit/usage bridge | Implemented as explicit env-gated beta path; provider health/audit endpoints, persistent provider audit events, and dispatch usage columns exist; CI uses stub/mock paths and does not call real provider APIs |
| Rust + TypeScript Cutover | Complete; Rust `engine/` is the primary runtime/API/storage/provider-gated control plane, `dashboard/` and `sdk/typescript/` are the primary TypeScript surfaces; Python retained as REST SDK and utility scripts only |

## What This Project Is Not

- **Not CA-8.** The CA-7 baseline is sealed. No CA-8 exists.
- **Not Stage 5.** No Stage 5 implementation has been started.
- **Not a cloud production SaaS.** No real model providers, sandboxes, workers, hosted service, or deployment targets.
- **No real provider/model calls by default.** Provider adapters are explicit env-gated beta paths; CI uses stub/mock paths and does not call real provider APIs.
- **No real sandbox/process/container/VM execution.** Sandbox claims are logical file-claim tracking only.
- **No autonomous workers.** No real concurrent workers are spawned.
- **No target repo writes by default.** Target repositories are read-only. The app never writes to them.

## Must-Read Order

1. **[README.md](../README.md)** — Project identity, test command, safety boundaries, repo structure.
2. **[docs/CURRENT_STATUS.md](CURRENT_STATUS.md)** — Latest known state, completed tracks, current capabilities.
3. **[docs/NEXT_DECISION.md](NEXT_DECISION.md)** — What to do next and what is disallowed by default.
4. **[docs/MODULE_MAP.md](MODULE_MAP.md)** — Module-to-stage reference table.
5. **[docs/trials/TRIAL_1_REPORT.md](trials/TRIAL_1_REPORT.md)** — Latest trial results and hardening closeout.
6. **[docs/trials/TRIAL_2_FINAL_STATE_INDEX.md](trials/TRIAL_2_FINAL_STATE_INDEX.md)** — Trial 2 complete evidence chain and final state.
7. **[docs/demo/README.md](demo/README.md)** — Local demo walkthrough (optional).

## Default Behavior

The responsible coding agent may autonomously advance repository-safe work that keeps the project moving:

- repair stale docs and handoff drift
- fix failing tests, CI, security baseline, or deterministic regressions
- add focused tests for existing behavior
- harden completed phases when backed by concrete review findings
- implement documented dispatch-kernel phase work when the architecture book already defines the contract and the implementation does not broaden real provider behavior beyond explicit env-gated beta paths, real sandbox/process execution, target repo writes, deployment, or real worker processes

Do **not** start any of the following without explicit human approval:

- MVP9
- Trial 2
- Stage 5
- Provider/model integration
- Sandbox/process/container/VM execution
- Autonomous workers
- Target repo writes
- Approval/run/execute/deploy/merge controls

Before proposing any new track, read `docs/CURRENT_STATUS.md` and `docs/NEXT_DECISION.md` first.

## Autonomous Session Closeout

A session is not complete until it leaves a durable handoff:

1. Relevant tests or verification commands were run and recorded.
2. `uv run --no-project python scripts/check_agent_handoff.py` passes (includes toolchain drift guard).
3. Handoff docs reflect the current branch, status, test count, stable commits, limitations, and next action.
4. The commit message is in English and the active branch is pushed when the tree contains only this session's intended changes.
5. The final report states latest commit, verification, remaining risks, and the next safe action.

## Documentation Maintenance

After any commit-sized change, update only the handoff docs whose facts changed:

- `docs/CURRENT_STATUS.md` for current state, test count, stable commit, limitations, and verification
- `docs/NEXT_DECISION.md` for allowed/disallowed next paths
- `docs/MODULE_MAP.md` for module ownership changes
- `README.md`, `CLAUDE.md`, and `AGENTS.md` for agent-facing workflow or boundary changes

Do not add parallel roadmap, next-steps, closeout, status, or productization documents unless the user explicitly asks for a new artifact. Prefer shortening or deleting stale documents. If no docs changed, state the reason in the completion report.
