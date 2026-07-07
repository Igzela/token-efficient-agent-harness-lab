# Current Status

Last updated: 2026-07-07.

## Summary

This repo is a local/small-team self-hosted agent workflow control plane. Rust `engine/` is the sole runtime/API/storage implementation. The active docs are kept small and current.

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
| Post-R7 Wire/Type Governance Hardening | Implemented through `scripts/check_wire_codegen_drift.sh` |

## Active Tracks

| Track | Status |
|---|---|
| Agent Autonomous Maintenance Mode | Active |
| Full Agent Autonomy Mode | Active for repo-scoped, testable, observable, CI-gated, rollbackable work |
| Real local stateful-vs-stateless runner | Approved next direction; not implemented yet |

## Current Gaps

- Real stateful-vs-stateless runner is not implemented yet.
- No second runtime, scheduler, graph kernel, mailbox, or storage family is authorized.
- Hosted/multi-tenant use and unbounded loops remain out of scope.

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
