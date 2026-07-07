# Current Status

Last updated: 2026-07-07.

## Product Boundary

This repo is a local/small-team self-hosted agent workflow control plane. Rust `engine/` is the sole runtime/API/storage implementation. The dashboard is a guarded local operations console. SDKs expose the app-owned REST surface.

The system is not a cloud SaaS, hosted multi-tenant service, direct-deploy tool, or default-on unattended agent runtime.

## Current Complete Tracks

| Track | Status |
|---|---|
| Dispatch kernel | Complete |
| Rust runtime migration | Complete |
| V2 Real Production Output | Complete through V2-5 |
| Real Output Closeout | Complete; `v0.1.0` published |
| Adaptive Fusion | Complete through AF-7 |
| Agent Runtime | Complete and sealed at AR-6 |
| Trusted Local Autonomous Execution | Complete through IAE-3 |
| Token-efficiency scorecards | Native persistence/API/dashboard/importer/pilot path implemented |

## Current Active Tracks

| Track | Status |
|---|---|
| Agent Autonomous Maintenance Mode | Active for docs, CI, tests, review, and bounded shipping |
| Full Agent Autonomy Mode | Active for repo-scoped architecture and authority evolution when testable, observable, CI-gated, and rollbackable |
| Provider-gated real experiment runner | Approved next direction; must extend existing runtime modules and remain local, explicit, budgeted, observable, killable, testable, and rollbackable |

## Current Capabilities

- Deterministic dispatch, workflow state, app-owned execution metadata, guarded local controls, SDKs, and audit evidence.
- Provider execution through the trusted-local profile or legacy explicit gate, with configured provider identity, auth, budget, token/call/time/concurrency limits, redaction, audit, pause/kill controls, and rollback.
- Adaptive Fusion candidate generation, experiments, policy evidence, promotion, guarded completion routing, and operator evidence.
- V2 target-output path through app-owned worktree, verification, approval, patch/branch output, and optional GitHub PR creation.
- Bounded Agent Runtime semantics through AR-0 to AR-6: identity, state, mailbox, step executor, planning/handoff, concurrency, review/debate, and operator evidence.
- Token-efficiency path: scorecard validator, native scorecard export/persistence/API/dashboard, LangGraph trace importer, native deterministic stateful-vs-stateless pilot, and read-only comparisons.

## Current Gaps

- Real provider-backed stateful-vs-stateless experiment runner is not implemented yet. It is allowed as the next direction, but must use existing runtime modules and gates.
- No second Agent Runtime, scheduler, DAG kernel, mailbox, storage layer, or hidden side channel is authorized.
- No hard process/container/VM sandbox exists.
- Cloud/multi-tenant hosting, direct target `main` writes, automatic merge/deploy/release, and unbounded loops remain out of scope.

## Active Documentation

Keep the active docs small. Do not add new roadmap/status/policy documents unless the user explicitly asks for a separate artifact.

- `docs/ARCHITECTURE_BOOK.md` — architecture and safety baseline.
- `docs/CURRENT_STATUS.md` — current product state and gaps.
- `docs/NEXT_DECISION.md` — single forward plan and authority decisions.
- `docs/MODULE_MAP.md` — source ownership and verification routing.
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` — branch/PR/CI/maintenance workflow.
- `docs/RUNBOOK.md` — operator procedures.

Historical phase plans, old closeouts, stale indexes, and low-frequency status snapshots should stay out of the working tree unless they are required for current operation.
