# Session Start Here

> **Note:** This file is a concise human/new-session summary. Claude Code should use `CLAUDE.md` as default entrypoint. Other agents should use `AGENTS.md` as default entrypoint. Read this file only when a human or new assistant needs a broad project summary.

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

Tests: 1390 Rust pass. Primary cutover verification is `bash scripts/verify_rust_typescript_stack.sh`.

Macro-Orchestrator Phase 1-5 repair batch is complete. Self-Hosted GA Readiness Track SG-1 through SG-5 is complete. HA Hardening Track is COMPLETE (HA-1 through HA-6): scheduler resilience with persistent heartbeat, automated backup with retention, deep health with resource monitoring, circuit breaker, TLS inbound, and SQLite encryption at rest. Docs simplified from 136 to 16 files.

| Active Track | Status |
|---|---|
| Real-World Testing Mode | **ACTIVE** — PR #27 merged (`7fdd5f2`), Dynamic Global Regulator plan is strategic background |
| Agent Autonomous Maintenance Mode | **ACTIVE** — agents may maintain docs, CI, tests, low-risk PR flow, and branch+PR repo-safe work under playbook gates |
| HybridExecutor | Complete — ACP_EXECUTION_MODE (off/provider/cli/auto), 1390 tests |
| HA Hardening Track | **COMPLETE** — All 6 phases done (HA-1 through HA-6), 1378 tests |

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
| Supervised Autonomous Beta Planning | Batch 0-6 governance/module/model/read-only-planner/durable-state/advisory/design-gate work recorded. `WorkflowGraph` is canonical planning model. Batch 7 Slice A-F implemented: app-owned workspace/artifact metadata, read-only HTTP/SDK/dashboard visibility, approval-binding contract, and supervised execution runtime primitives (NodeExecutor trait, CommandNodeExecutor with shell-metachar rejection, workflow tick, workspace lifecycle, capture_patch with source manifest diff, integrity validation, export gate, E2E closed-loop test). 1339 Rust tests pass. No target repo writes by the app runtime, no sandbox/process/container/VM execution, no real workers, no default provider calls, no push/deploy/apply controls. Agent maintenance may perform branch+PR work under `docs/REAL_WORLD_TESTING_PLAYBOOK.md` gates. |
| Dynamic Workflow Direction | Complete; Batches 1-7 plus scheduler dynamic-mode recovery are implemented. Opt-in dynamic mode can observe a failed node, mutate the persisted graph with fix/test nodes, mark the failed node recovered, resume the run, and complete follow-up execution. 1339 Rust tests pass. |
| Macro-Orchestrator Direction | Current product direction. Phase 1-5 repair batch and Self-Hosted GA Readiness Track SG-1 through SG-5 COMPLETE. Track done. |

## Autonomous Session Closeout

Before committing, run `uv run --no-project python scripts/check_agent_handoff.py` to verify the handoff surface is consistent. Update only the authoritative docs whose facts changed. Prefer prune/archive/link over adding more prose.

## What This Project Is Not

- **Not CA-8.** The CA-7 baseline is sealed. No CA-8 exists.
- **Not Stage 5.** No Stage 5 implementation has been started.
- **Not a cloud production SaaS or coding-agent runtime.** No default-on real model providers, sandbox isolation runtime, workers, hosted service, or production deployment targets.
- **No real provider/model calls by default.** Provider adapters are explicit env-gated beta paths; CI uses stub/mock paths and does not call real provider APIs.
- **No sandbox/process/container/VM isolation runtime.** Sandbox claims are logical file-claim tracking only. Existing local CLI executor subprocess invocation is a separate, explicit opt-in exception via `ACP_ENABLE_CLI_EXECUTION=1`.
- **No autonomous app-runtime workers.** The app does not spawn unrestricted workers. Agent autonomous maintenance is a repository workflow mode governed by `docs/REAL_WORLD_TESTING_PLAYBOOK.md`, not an app-runtime worker feature.
- **No target repo writes by the app runtime by default.** Target repositories remain protected from direct app writes. Agent maintenance may create branches, commits, PRs, and low-risk merges only through branch+PR workflow under playbook gates.

Production-grade hosted/self-hosted productization track was explicitly approved by user on 2026-06-06. This track extends existing supervised autonomous beta infrastructure with real CLI executor integration, persistent scheduling, dashboard controls, SDK productization, and security hardening. It does NOT create parallel runtime kernels. See `docs/NEXT_DECISION.md` for phase details and done-when criteria.
