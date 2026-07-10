# Current Status

Last updated: 2026-07-10.

## Summary

This repo is a local/small-team self-hosted agent workflow control plane. Rust `engine/` is the sole runtime/API/storage implementation. Active docs stay small and current.

## Complete Tracks

| Track | Status |
|---|---|
| Dispatch kernel | Complete |
| Phase 4 | Historical and complete |
| V2 Real Production Output | Complete |
| Adaptive Fusion | Complete through AF-7 |
| Agent Runtime | Complete and sealed at AR-6 |
| Trusted Local Autonomous Execution | Complete through IAE-3 |
| Token-efficiency scorecards | Native and comparison paths implemented |
| Local stateful-vs-stateless runner | Implemented as a script-level stub runner in #154 |
| Local runner evidence chain | Storage/API/operator evidence consumption and local artifact import implemented |
| Agent memory policy layer | Implemented inside existing AgentState, AgentStepExecutor, context_pack, workflow context_injection, operator evidence, and scorecard state-byte paths without new storage/runtime |
| Post-R7 Wire/Type Governance Hardening | Implemented through `scripts/check_wire_codegen_drift.sh` |
| Security and release hardening | Auth scope/expiry/live-clock fixes, persistent redacted provider audit, Rust/Bun advisory gates, SHA-pinned GitHub Actions, target-correct release packaging, and atomic upgrade rollback are implemented |

## Active Tracks

| Track | Status |
|---|---|
| Agent Autonomous Maintenance Mode | Active |
| Full Agent Autonomy Mode | Active for repo-scoped, testable, observable, CI-gated, rollbackable work |
| Local runner operations | Active: validate, export, import, and inspect bounded scorecard artifacts locally |
| Runner integration | Storage/API/operator evidence complete; workflow scheduling of local runner validation is implemented via `LocalRunnerValidationExecutor` in stub mode (13 tests) with automatic native scorecard recording on tick completion |
| Live provider adapter | Gated ready path: `local_runner_provider.rs` supports Stub/Fake/Live; Live requires gates, explicit metadata, symbolic credentials, positive pricing, persistent redacted audit, bounded calls/tokens/time/cost, and a kill switch |

## Current Gaps

- The `LocalRunnerValidationExecutor` executes stub-mode stateful-vs-stateless runs deterministically as a workflow node (13 tests). Automatic native scorecard recording happens via the existing tick-level path when the run becomes terminal.
- The live provider adapter (`local_runner_provider.rs`, `FakeProvider`) delegates Python non-stub runs to the Rust CLI binary (`local-runner-exec`). Rust live mode requires `ACP_ENABLE_PROVIDER_EXECUTION=1` or a trusted local profile, explicit `ACP_LOCAL_RUNNER_*` metadata, the referenced credential variable, positive `ACP_PROVIDER_*_COST_PER_1K_USD` pricing, and a writable audit database. It reserves worst-case per-call tokens/cost before invocation, shares run and daily budgets across both modes, records bounded redacted audit events, enforces timeout/kill, and fails closed on missing evidence.
- Dynamic workflow tick/mutation limits are reconstructed from durable events; feedback suggestions affect pool routing only when the executor is registered and available, while explicit executor configuration remains pinned.
- CI audits Rust, dashboard, and TypeScript SDK lockfiles. GitHub Actions are SHA-pinned and checked by `scripts/check_github_action_pins.sh`; release publication is gated by full-stack, PostgreSQL, SDK, advisory, handoff, packaging, and upgrade rollback checks.
- `FakeProvider` (`provider/fake.rs`) provides deterministic test output with zero cost. `is_enabled() == true` (it is a valid test provider, not disabled).
- `external_calls` in `runner_metadata` correctly reflects the number of provider invocations (steps count) for both Rust and Python paths.
- Native scorecard artifacts are automatically recorded when the workflow run completes (proven by integration tests). The artifact contains bounded metadata and workflow-level step projections only (no raw local-runner steps, prompts, outputs, or transcripts).
- Python runner (`scripts/provider_gated_real_runner.py`) accepts `--provider {stub,fake,live}` (aligned with Rust binary). Non-stub modes delegate to the Rust binary. Config validation no longer checks binary existence; binary resolution happens only at `build_pair` time.
- Remote adapter support for this runner is deferred to a later focused change.
- Keep the existing runtime/module ownership boundaries unless a later active-doc decision changes them.

## Handoff Guard Anchors

Branch: use latest `main`.
Tests: run focused checks and full stack verification before merge.
Full Agent Autonomy Mode remains active.
Post-R7 Wire/Type Governance Hardening remains implemented through `scripts/check_wire_codegen_drift.sh`.

## Active Documentation

- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Do not add new roadmap/status/policy documents by default. Put current direction in the active docs above.
