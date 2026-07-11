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
| PE-1 Token Efficiency Regression Lab | Complete through deterministic registry/report/batch recomputation, bounded evidence, SQLite/PostgreSQL persistence, idempotent import, trends, read-only API/SDK, Dashboard history, and acceptance seal |
| Post-R7 Wire/Type Governance Hardening | Implemented through `scripts/check_wire_codegen_drift.sh` |
| Security and release hardening | Auth scope/expiry/live-clock fixes, persistent redacted provider audit, Rust/Bun advisory gates, SHA-pinned Actions, target-correct release packaging, and atomic upgrade rollback are implemented |

## Active Tracks

| Track | Status |
|---|---|
| Agent Autonomous Maintenance Mode | Active through execution-ready task packets and bounded prerequisite repairs |
| Full Agent Autonomy Mode | Active for repo-scoped planning, design, implementation, review, CI repair, documentation, and merge when changes are testable, observable, CI-gated, and rollbackable |
| Model selection | User/tool controlled; repository policy does not require or forbid a model or reasoning tier |
| Autonomous decision authority | The coding agent may resolve bounded architecture, authority, schema, migration, security, release, and recovery decisions from repository evidence and record material decisions before implementation |
| Local runner operations | Active: validate, export, import, and inspect bounded scorecard artifacts locally |
| Runner integration | Storage/API/operator evidence complete; workflow scheduling of local runner validation is implemented via `LocalRunnerValidationExecutor` in stub mode with automatic native scorecard recording on terminal tick |
| Live provider adapter | Gated ready path: Stub/Fake/Live; Live requires gates, explicit metadata, symbolic credentials, positive pricing, persistent redacted audit, bounded calls/tokens/time/cost, and a kill switch |
| Post-LGB Product Evolution Plan | PE-1 is complete; PE2-CONTRACT-1 through PE2-READ-1 are implemented; PE2-PAUSE-1 is the next eligible packet |

## Planned Product Evolution Stages

The stages are packetized in `docs/NEXT_DECISION.md`. The agent should execute packets marked `READY_FOR_EXECUTION` whose prerequisites are complete, while repairing bounded prerequisite defects or contract drift when needed.

| Stage | Priority | Capability | Current state |
|---|---|---|---|
| PE-1 | P0 | Token Efficiency Regression Lab | Complete and acceptance-sealed |
| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | In progress: evidence, forecasts, anomalies, persistence, and read surfaces implemented; policy-gated pause next |
| PE-3 | P1 | Operator Decision Center | Detailed packets defined; blocked on PE-2 closeout |
| PE-4 | P1/P2 | Trace-backed Policy Replay | Detailed packets defined; blocked on PE-3 closeout and trace coverage |
| PE-5 | P1.5 | Release Provenance | Detailed packets defined; eligible after PE-1 closeout only by explicit independent-lane activation |
| PE-6 | P2 | Fault Injection and Recovery Drills | Detailed packets defined; blocked on recovery invariants and affected stage prerequisites |

## PE-1 Acceptance Evidence

- deterministic single-scenario and registry-wide report recomputation is covered by focused Python tests;
- tamper, threshold, quality-failure, missing-baseline, missing-best-known, incomparable, cross-version, and registry-coverage cases are covered;
- SQLite and PostgreSQL regression artifact persistence, repeat import, deterministic trends, and audit behavior are covered;
- list/detail/trend HTTP routes and Python/TypeScript SDK readers are covered;
- Dashboard tests cover all six outcomes, absent evidence roles, empty and one-point histories, transitions, encoded paths, and exclusion of raw fields;
- PR #177 CI run 29137424748 passed all seven jobs, including PostgreSQL, Rust, TypeScript/Dashboard, Python, Docker, native runtime, full-stack cutover, formatting, clippy, and dependency audits;
- PE-1 remains report-only: no CI blocking, provider calls, routing mutation, policy mutation, pause authority, or target-repository writes were added.

## PE-2 Contract Evidence

- `budget_forecast_evidence.v1` and `budget_anomaly_finding.v1` are additive Rust contracts under the existing budget owner;
- scope, evidence windows, freshness, sample counts, coverage, pricing completeness, duplicate counts, confidence, stable reason codes, bounded references, and deterministic hashes are explicit;
- `supported`, `insufficient_evidence`, and `invalid_evidence` remain distinct outcomes;
- observed values are structurally separate from estimates and anomaly measurements;
- malformed, oversized, tampered, sparse, contradictory, unpriced, missing-dimension, and invalid-window evidence fails closed or remains explicitly insufficient;
- no persistence, API, SDK, Dashboard, provider, reservation, policy, pause, or target-output behavior changes are part of the contract packet.

## PE-2 Forecast Evidence

- deterministic forecasts use only bounded posted observations and explicit half-open evidence windows;
- observed token/cost totals remain separate from linear horizon estimates and exhaustion time;
- sparse, stale, mixed required dimensions, missing dimensions, excessive duplicates, conflicting duplicates, and incomplete pricing return explicit bounded outcomes;
- provider audit adaptation does not invent run, workspace, model, content hash, or non-USD pricing facts;
- focused tests cover zero usage, bursty usage, mixed workloads, boundary time, deterministic ordering, duplicate reconciliation, conflicting evidence, and concurrent reads;
- no persistence, API, SDK, Dashboard, budget/reservation mutation, policy, pause, or target-output behavior changes are part of the forecast packet.

## PE-2 Anomaly Evidence

- deterministic rules cover cost, token, retry, latency, context-growth, and model-mix findings over explicit equal-duration windows;
- supported normal evidence remains `detected=false`, while sparse, stale, mixed, incomplete, or excessive-duplicate evidence remains explicitly insufficient;
- coverage metadata derives `missing_fields` only from fields absent in filtered evidence and preserves observed dimensions on applicable invalid-evidence paths;
- conflicting duplicates and malformed metric evidence fail closed as versioned `invalid_evidence` findings with bounded references and reason codes;
- thresholds are explicit and deterministic; equality does not create a false positive and no adaptive or opaque score is introduced;
- no persistence, API, SDK, Dashboard, provider substitution, budget/reservation mutation, policy, pause, termination, or target-output behavior changes are part of the anomaly packet.

## Current Gaps

- PE-2 policy-gated high-confidence auto-pause is not implemented.
- PE-3 has no unified derived decision queue.
- `policy_simulator.rs` still relies on fixed estimates rather than trace-calibrated replay.
- SBOM, signing, and provenance attestations are not yet part of the release contract.
- There is no systematic fault-injection and recovery-drill harness.
- External runtime runners, scheduled external execution, CrewAI, and Microsoft Agent Framework integrations remain unauthorized unless a later evidence-backed decision changes the importer-first boundary.
- Remote adapter support for the local runner remains deferred.
- Keep existing runtime/module ownership boundaries unless an execution-ready packet or documented replacement decision explicitly changes them.

## PE-2 Read Evidence

- schema v18 adds immutable, hash-bound budget forecast/anomaly envelopes to the existing SQLite/PostgreSQL `LocalProductStore` boundary;
- persistence is validated, idempotent, deterministically ordered, tamper-detecting, and bounded to limit 100 with offset capped at 10,000;
- read-only `dispatch:read` HTTP/OpenAPI plus Python/TypeScript SDK readers preserve encoded artifact paths and existing callers;
- the existing Benchmark Dashboard route exposes explicit supported, insufficient, invalid, empty, and error states with bounded evidence IDs, hashes, and reason codes;
- no pause, policy, provider, budget/reservation, scheduler, termination, or target-output authority is added.

## Handoff Guard Anchors

Branch: use latest `main` unless continuing an owned PR.

Tests: run focused checks and full stack verification before merge.

Full Agent Autonomy Mode remains active for repo-scoped, testable, observable, CI-gated, rollbackable work.

Execution governance: `scripts/check_agent_handoff.py` validates packet, autonomy, documentation, schema, wire, toolchain, and secret-scan anchors without enforcing a model choice.

Post-R7 Wire/Type Governance Hardening remains implemented through `scripts/check_wire_codegen_drift.sh`.

## Active Documentation

- `AGENTS.md`
- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Do not add new roadmap/status/policy documents by default. Put current direction and execution-ready task packets in the active surfaces above.
