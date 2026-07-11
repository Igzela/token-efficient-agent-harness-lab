# Current Status

Last updated: 2026-07-11.

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
| Token-efficiency scorecards | Canonical hash and derived-metric integrity enforced; native v1 and generic v2 artifacts share one store/API boundary |
| LangGraph importer-first pilot | PR #149 importer and PR #165 direction are complete through real offline LangGraph 1.2.9 evidence, v2 persistence, scenario API/Dashboard, and fixed end-to-end fixtures |
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
| Runner integration | Storage/API/operator evidence complete; workflow scheduling of local runner validation is implemented via `LocalRunnerValidationExecutor` in stub mode with automatic native scorecard recording on terminal tick |
| Live provider adapter | Gated ready path: `local_runner_provider.rs` supports Stub/Fake/Live; Live requires gates, explicit metadata, symbolic credentials, positive pricing, persistent redacted audit, bounded calls/tokens/time/cost, and a kill switch |
| Post-LGB Product Evolution Plan | PE-1 is in progress: the bounded v1 registry, deterministic single-scenario and registry-wide batch report cores, checked fixed evidence pairs, and idempotent bounded report persistence are implemented with canonical hashes and zero mutation authority |

## Planned Product Evolution Stages

These stages are authorized but not yet complete. They must advance through scoped PRs and reuse existing runtime, storage, policy, operator, release, and recovery boundaries.

| Stage | Priority | Capability | Current state |
|---|---|---|---|
| PE-1 | P0 | Token Efficiency Regression Lab | In progress: registry, single-scenario/batch report cores, checked evidence pairs, and idempotent LocalProductStore persistence complete; next is file import and bounded trend behavior |
| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | Planned after PE-1: forecast exhaustion, explain anomalies, pause only on high-confidence policy-backed signals |
| PE-3 | P1 | Operator Decision Center | Planned: derived action queue over existing approvals, evidence, budget risk, failures, scheduler controls, and rollback candidates |
| PE-4 | P1/P2 | Trace-backed Policy Replay | Planned: replace fixed heuristic estimates with versioned trace-backed calibration, coverage checks, shadow replay, and guarded canary progression |
| PE-5 | P1.5 | Release Provenance | Planned: SBOM, artifact/container signing, provenance attestations, installer verification, and release evidence |
| PE-6 | P2 | Fault Injection and Recovery Drills | Planned: bounded failure scenarios with recovery invariants, state-loss checks, fail-closed validation, and recovery-time evidence |

## Current Gaps

- PE-1 has a versioned, hash-bound registry, deterministic single-scenario and registry-wide batch report cores, checked baseline/candidate evidence for the LangGraph, native deterministic, and local-stub scenarios, plus bounded SQLite/PostgreSQL persistence with idempotent repeat recording and tamper validation. File import, history/trends, API, SDK, and Dashboard remain incomplete.
- Budget controls are enforced, but predictive exhaustion, explainable anomaly detection, and high-confidence automatic pause are not implemented.
- Operator evidence and controls exist, but there is no unified derived decision queue.
- `policy_simulator.rs` still relies on fixed success, latency, review, and relative-cost estimates rather than trace-calibrated replay.
- Release hardening exists, but SBOM, signing, and provenance attestations are not yet part of the release contract.
- Recovery paths exist for selected operations, but there is no systematic fault-injection and recovery-drill harness.
- External runtime runners, scheduled external execution, CrewAI, and Microsoft Agent Framework integrations remain unauthorized unless a later explicit decision changes the importer-first boundary.
- Remote adapter support for the local runner remains deferred.
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
