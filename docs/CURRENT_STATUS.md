# Current Status

Last updated: 2026-07-08.

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
| Post-R7 Wire/Type Governance Hardening | Implemented through `scripts/check_wire_codegen_drift.sh` |

## Active Tracks

| Track | Status |
|---|---|
| Agent Autonomous Maintenance Mode | Active |
| Full Agent Autonomy Mode | Active for repo-scoped, testable, observable, CI-gated, rollbackable work |
| Local runner operations | Active: validate, export, import, and inspect bounded scorecard artifacts locally |
| Runner integration | Storage/API/operator evidence complete; workflow scheduling of local runner validation is implemented via `LocalRunnerValidationExecutor` in stub mode (12 tests) with automatic native scorecard recording on tick completion |
| Live provider adapter | Gated ready path: `local_runner_provider.rs` supports Stub/Fake/Live with gate-aware OpenAI-compatible provider construction; Live requires env gates plus explicit local-runner provider metadata and fails closed without them |

## Current Gaps

- The `LocalRunnerValidationExecutor` executes stub-mode stateful-vs-stateless runs deterministically as a workflow node (12 tests). Automatic native scorecard recording happens via the existing tick-level path when the run becomes terminal.
- The live provider adapter (`local_runner_provider.rs`, `FakeProvider`) delegates Python non-stub runs to the Rust CLI binary (`local-runner-exec`). Rust live mode is a gated OpenAI-compatible path: it requires `ACP_ENABLE_PROVIDER_EXECUTION=1` or a trusted local profile, plus `ACP_LOCAL_RUNNER_PROVIDER_TYPE=openai_compatible`, `ACP_LOCAL_RUNNER_BASE_URL`, `ACP_LOCAL_RUNNER_MODEL`, `ACP_LOCAL_RUNNER_API_KEY_ENV`, and the referenced credential environment variable. Missing gates or metadata fail closed before provider calls.
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
