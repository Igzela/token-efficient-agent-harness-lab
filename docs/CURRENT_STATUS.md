# Current Status

Last updated: 2026-07-07.

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
| Post-R7 Wire/Type Governance Hardening | Implemented through `scripts/check_wire_codegen_drift.sh` |

## Active Tracks

| Track | Status |
|---|---|
| Agent Autonomous Maintenance Mode | Active |
| Full Agent Autonomy Mode | Active for repo-scoped, testable, observable, CI-gated, rollbackable work |
| Local runner operations | Active: document, validate, and use the #154 local runner before extending it |
| Runner integration | Next: decide whether to connect the runner to workflow/storage/operator evidence or keep it script-only |

## Current Gaps

- The local runner is script-level only; it is not yet connected to Rust workflow scheduling or app storage.
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
