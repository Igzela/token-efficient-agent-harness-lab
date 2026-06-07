# Session Start Here

Read this file first in any new AI session on this repository.

## Project Identity

Token-Efficient Agent Harness Lab is a local deterministic harness and self-hosted macro-orchestrator control plane for studying event-sourced agent workflow infrastructure.

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
| Trial 4 real-use pilot | Closed — `TRIAL_4_REAL_USE_PILOT_PASS_AFTER_FIXES` |
| Trial 5 CLI execution beta | Closed — `TRIAL_5_CLI_EXECUTION_BETA_PASS_AFTER_FIXES` |

Tests: 1367 Rust pass (14 circuit breaker tests). Primary cutover verification is `bash scripts/verify_rust_typescript_stack.sh`.

Macro-Orchestrator Phase 1-5 repair batch is complete. Self-Hosted GA Readiness Track SG-1 through SG-5 is complete: real dynamic CLI pilot matrix, long-run soak/failure injection, mission-control dashboard visibility, enriched policy decision signals, and runbook/release/rollback handoff readiness. HA Hardening Track started: observability, deep health, graceful shutdown, retry jitter, and circuit breaker (HA-4) are done. Docs simplified from 136 to 16 files. Next: HA-1 Scheduler Resilience (persistent heartbeat + PostgreSQL) and HA-3 Deep Health + Resource Monitoring (external monitoring).

| Active Track | Status |
|---|---|
| HA Hardening Track | Started — HA-4 circuit breaker done; HA-1 and HA-3 next |

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
| Production-like Local Beta Ops Hardening | Implemented; `.env.production-like.local.example`, guarded local startup script, metrics endpoint, Operations dashboard tab, ops check, backup verify/restore dry-run smoke, secret scan, audit redaction, provider pricing visibility, read-only advisory risk-gate repair, and scope templates are available for local trials |
| Rust Provider Stack Stage 1 + Stage 2 audit/usage bridge | Implemented as explicit env-gated beta path; provider health/audit endpoints, persistent provider audit events, and dispatch usage columns exist; CI uses stub/mock paths and does not call real provider APIs |
| Rust + TypeScript Cutover | Complete; Rust `engine/` is the primary runtime/API/storage/provider-gated control plane, `dashboard/` and `sdk/typescript/` are the primary TypeScript surfaces; Python retained as REST SDK and utility scripts only |
| Architecture Refactor R-series | Sealed at R7; R8 is not approved. No further R-series file splitting is approved |
| Post-R7 Wire/Type Governance Hardening | Implemented; `app_layer` remains dormant/unwired reference code and `scripts/check_wire_codegen_drift.sh` protects generated wire files |
| Supervised Autonomous Beta Planning | Batch 0-6 governance/module/model/read-only-planner/durable-state/advisory/design-gate work recorded. `WorkflowGraph` is canonical planning model. Batch 7 Slice A-F implemented: app-owned workspace/artifact metadata, read-only HTTP/SDK/dashboard visibility, approval-binding contract, and supervised execution runtime primitives (NodeExecutor trait, CommandNodeExecutor with shell-metachar rejection, workflow tick, workspace lifecycle, capture_patch with source manifest diff, integrity validation, export gate, E2E closed-loop test). 1339 Rust tests pass. No target repo writes, sandbox/process/container/VM execution, real workers, provider calls, push/merge/deploy/apply controls, or default-on execution. |
| Dynamic Workflow Direction | Complete; Batches 1-7 plus scheduler dynamic-mode recovery are implemented. Opt-in dynamic mode can observe a failed node, mutate the persisted graph with fix/test nodes, mark the failed node recovered, resume the run, and complete follow-up execution. 1339 Rust tests pass. |
| Macro-Orchestrator Direction | Current product direction. Phase 1-5 repair batch and Self-Hosted GA Readiness Track SG-1 through SG-5 COMPLETE. Track done. |

## What This Project Is Not

- **Not CA-8.** The CA-7 baseline is sealed. No CA-8 exists.
- **Not Stage 5.** No Stage 5 implementation has been started.
- **Not a cloud production SaaS or coding-agent runtime.** No default-on real model providers, sandbox isolation runtime, workers, hosted service, or production deployment targets.
- **No real provider/model calls by default.** Provider adapters are explicit env-gated beta paths; CI uses stub/mock paths and does not call real provider APIs.
- **No sandbox/process/container/VM isolation runtime.** Sandbox claims are logical file-claim tracking only. Existing local CLI executor subprocess invocation is a separate, explicit opt-in exception via `ACP_ENABLE_CLI_EXECUTION=1`.
- **No autonomous workers.** No real concurrent workers are spawned.
- **No target repo writes by default.** Target repositories are read-only. The app never writes to them.

Production-grade hosted/self-hosted productization track was explicitly approved by user on 2026-06-06. This track extends existing supervised autonomous beta infrastructure with real CLI executor integration, persistent scheduling, dashboard controls, SDK productization, and security hardening. It does NOT create parallel runtime kernels. See `docs/NEXT_DECISION.md` for phase details and done-when criteria.

Planning-only modules may generate non-executable plans, app-owned planning metadata, and design-gate documents. They do not grant runtime worker, execution, target-write, sandbox, deploy, apply, run, or merge authority.

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

- repair stale docs, handoff drift, and wire-codegen guard drift
- fix failing tests, CI, security baseline, or deterministic regressions
- add focused tests for existing behavior
- harden completed phases when backed by concrete review findings
- implement documented dispatch-kernel phase work when the architecture book already defines the contract and the implementation does not broaden real provider behavior beyond explicit env-gated beta paths, add sandbox isolation, expand subprocess execution beyond the existing CLI executor path, add target repo writes, deployment, or real worker processes

Do **not** start any of the following without explicit human approval:

- MVP9
- Stage 5
- Broader provider/model integration beyond the explicit env-gated local beta path
- Sandbox isolation or subprocess expansion beyond the existing CLI executor path
- Autonomous workers
- Target repo writes
- Approval/run/execute/deploy/merge controls

Before proposing any new track, read `docs/CURRENT_STATUS.md` and `docs/NEXT_DECISION.md` first.

## Implementation Strategy

**All sessions must use Workflow tool for implementation.** This is the default, not optional.

1. Write a workflow script to `.claude/workflows/<task-name>.md` with `export const meta = { name, description, phases }`.
2. Use `parallel()` for independent subtasks (e.g., Rust module + API endpoint in parallel, then SDK + Dashboard in parallel).
3. Use `pipeline()` when tasks have sequential dependencies (e.g., Wave 1 code → Wave 2 integration → Wave 3 verify).
4. Use `model: 'opus'` for implementation agents, `model: 'sonnet'` for verification/checks.
5. Launch via `Workflow({scriptPath: ".claude/workflows/<task-name>.md"})`.
6. After workflow completes, fix any issues found by verification agent, then commit/push.
7. Wait for CI green before starting the next batch.

**The only exception is trivial single-line edits** (typo fixes, doc wording, env var changes). Anything touching 2+ files goes through Workflow.

## Autonomous Session Closeout

A session is not complete until it leaves a durable handoff:

1. Relevant tests or verification commands were run and recorded.
2. `uv run --no-project python scripts/check_agent_handoff.py` passes (includes toolchain and `scripts/check_wire_codegen_drift.sh` guards).
3. Handoff docs reflect the current branch, status, test count, stable commits, limitations, and next action.
4. The commit message is in English and the active branch is pushed when the tree contains only this session's intended changes.
5. After push, **wait for CI to pass** before starting the next batch. Use `gh run list --limit 3` to check status; if CI fails, fix and re-push before continuing. A green CI is required before the next session's work is considered safe to build on.
6. The final report states latest commit, CI status, verification, remaining risks, and the next safe action.

## Documentation Maintenance

After any commit-sized change, update only the handoff docs whose facts changed:

- `docs/CURRENT_STATUS.md` for current state, test count, stable commit, limitations, and verification
- `docs/NEXT_DECISION.md` for allowed/disallowed next paths
- `docs/MODULE_MAP.md` for module ownership changes
- `README.md`, `CLAUDE.md`, and `AGENTS.md` for agent-facing workflow or boundary changes

Do not add parallel roadmap, next-steps, closeout, status, or productization documents unless the user explicitly asks for a new artifact. Prefer shortening or deleting stale documents. If no docs changed, state the reason in the completion report.
