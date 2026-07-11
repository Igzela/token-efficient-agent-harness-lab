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
| LangGraph importer-first pilot | Complete through real offline LangGraph 1.2.9 evidence, v2 persistence, scenario API/Dashboard, and fixed end-to-end fixtures |
| Local stateful-vs-stateless runner | Implemented with bounded stub/fake/live paths and gated local execution |
| Local runner evidence chain | Storage/API/operator evidence consumption and local artifact import implemented |
| Agent memory policy layer | Implemented inside existing AgentState, AgentStepExecutor, context packing, workflow injection, operator evidence, and scorecard state-byte paths without a new runtime/store |
| Post-R7 Wire/Type Governance Hardening | Implemented through `scripts/check_wire_codegen_drift.sh` |
| Security and release hardening | Auth scope/expiry/live-clock fixes, persistent redacted provider audit, Rust/Bun advisory gates, SHA-pinned Actions, target-correct release packaging, and atomic upgrade rollback are implemented |

## Active Tracks

| Track | Status |
|---|---|
| Agent Autonomous Maintenance Mode | Active through Terra-ready task packets |
| Full Agent Autonomy Mode | Active inside approved packet boundaries for repo-scoped, testable, observable, CI-gated, rollbackable work |
| Codex executor profile | Project default is GPT-5.6 Terra with medium reasoning for implementation, review, plan mode, CI repair, and PR work; no executor self-escalation to Sol |
| Planner–executor split | External planner owns architecture/authority/contracts; Codex executes only the earliest eligible `READY_FOR_TERRA` packet in `docs/NEXT_DECISION.md` |
| Local runner operations | Active: validate, export, import, and inspect bounded scorecard artifacts locally |
| Runner integration | Storage/API/operator evidence complete; workflow scheduling of local runner validation is implemented via `LocalRunnerValidationExecutor` in stub mode with automatic native scorecard recording on terminal tick |
| Live provider adapter | Gated ready path: Stub/Fake/Live; Live requires gates, explicit metadata, symbolic credentials, positive pricing, persistent redacted audit, bounded calls/tokens/time/cost, and a kill switch |
| Post-LGB Product Evolution Plan | PE-1 is in progress: registry/report/batch, fixed evidence, persistence/import/trends, and read-only API/SDK are implemented; Dashboard packet is ready |

## Planned Product Evolution Stages

The stages are packetized in `docs/NEXT_DECISION.md`. Codex may execute only packets marked `READY_FOR_TERRA` whose prerequisites are complete.

| Stage | Priority | Capability | Current state |
|---|---|---|---|
| PE-1 | P0 | Token Efficiency Regression Lab | In progress: Dashboard history/trend UX is `READY_FOR_TERRA`; closeout follows |
| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | Detailed packets defined; blocked on PE-1 closeout |
| PE-3 | P1 | Operator Decision Center | Detailed packets defined; blocked on PE-2 closeout |
| PE-4 | P1/P2 | Trace-backed Policy Replay | Detailed packets defined; blocked on PE-3 closeout and trace coverage |
| PE-5 | P1.5 | Release Provenance | Detailed packets defined; eligible after PE-1 closeout or explicit independent-lane activation |
| PE-6 | P2 | Fault Injection and Recovery Drills | Detailed packets defined; blocked on recovery invariants and affected stage prerequisites |

## Current Gaps

- PE-1 Dashboard history, trend, baseline/best-known configuration, reasons, and evidence links remain incomplete.
- PE-2 predictive exhaustion, explainable anomaly detection, and policy-gated high-confidence auto-pause are not implemented.
- PE-3 has no unified derived decision queue.
- `policy_simulator.rs` still relies on fixed estimates rather than trace-calibrated replay.
- SBOM, signing, and provenance attestations are not yet part of the release contract.
- There is no systematic fault-injection and recovery-drill harness.
- Project config cannot technically prevent a user or CLI flag from overriding the model; repository policy requires a non-Terra-Medium executor to stop with `model_profile_mismatch`.
- External runtime runners, scheduled external execution, CrewAI, and Microsoft Agent Framework integrations remain unauthorized unless a later explicit planner decision changes the importer-first boundary.
- Remote adapter support for the local runner remains deferred.
- Keep existing runtime/module ownership boundaries unless a later Terra-ready packet explicitly changes them.

## Handoff Guard Anchors

Branch: use latest `main`.

Tests: run focused checks and full stack verification before merge.

Full Agent Autonomy Mode remains active inside Terra-ready packet boundaries.

Codex profile: `.codex/config.toml` must keep `gpt-5.6-terra` with medium reasoning; `scripts/check_agent_handoff.py` validates the profile and packet anchors.

Post-R7 Wire/Type Governance Hardening remains implemented through `scripts/check_wire_codegen_drift.sh`.

## Active Documentation

- `AGENTS.md`
- `.codex/config.toml`
- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Do not add new roadmap/status/policy documents by default. Put current direction and Terra-ready task packets in the active surfaces above.