# Current Status

Last updated: 2026-07-13.

## Summary

This repository is a local/small-team self-hosted agent workflow control plane. Rust `engine/` is the sole runtime, API, and storage implementation. Active documents describe current facts and forward execution; merged PRs and repository history retain detailed stage history.

## Complete and Acceptance-Sealed Tracks

| Track | Status |
|---|---|
| Dispatch kernel and V2 output authority | Complete |
| Adaptive Fusion | Complete through AF-7 |
| Agent Runtime | Complete and sealed through AR-6 |
| Trusted Local Autonomous Execution | Complete through IAE-3 |
| PE-1 Token Efficiency Regression Lab | Complete and acceptance-sealed |
| PE-2 Budget Intelligence and Anomaly Auto-Pause | Complete and acceptance-sealed |
| PE-3 Operator Decision Center | Complete and independently acceptance-sealed after PE3-REPAIR-1 and PE3-CLOSE-1 |
| PE-4 Trace-backed Policy Replay | Complete and acceptance-sealed under PE4-POST-CLOSE-REPAIR-1 |
| Post-R7 wire/type governance | Implemented through `scripts/check_wire_codegen_drift.sh` |
| Security and release baseline | Auth, audit, action-pin, dependency-audit, target-correct packaging, and atomic upgrade rollback controls are implemented |

## PE-4 Final Acceptance Evidence

PE-4's earlier v1/v2 replay and closeout semantics are historical and non-authorizing. The current accepted implementation is `PE4-POST-CLOSE-REPAIR-1`:

- PR: #206
- starting `main`: `0f92dadc6cf1cb712231dbb917bf9904f8346d86`
- exact final PR head: `80d9f9342956e1fd5931b59dcc426908d450b32b`
- merge commit: `f2a736a39e5de82d60da2a0b64d1c255d55ec326`
- exact-final-head CI: run `29190482093`, all seven required jobs passed
- post-merge `main` CI: run `29190797214`, all seven required jobs passed

The accepted boundary uses owner-backed `dispatch_history` provenance, `policy_replay_contract.v3`, `trace_replay_evidence.v2`, `offline_policy_replay.v2`, `dispatch_history_trace_owner.v1`, `judge_calibration.v1`, aligned SQLite/PostgreSQL schema v21 provenance columns, inclusive 90% coverage, explicit rejection severity, paired calibration thresholds, bounded canonical serialization, empirical-support OOD checks, and historical artifact non-authorization. Existing permission, confirmation, audit, pause, canary, promotion, snapshot, compensation, and rollback owners remain authoritative.

There is no pending PE-4 documentation-head or post-merge CI requirement. The evidence above is final.

## Active Product Evolution

| Stage | Priority | Capability | Current state |
|---|---|---|---|
| PE-5 | P1.5 | Release Provenance | `PE56-POST-SEAL-REPAIR-1` is implemented locally; the prior seal remains under repair until the reviewed exact head and post-merge `main` pass required CI |
| PE-6 | P2 | Fault Injection and Recovery Drills | `PE56-POST-SEAL-REPAIR-1` is implemented locally; the prior seal remains under repair until the reviewed exact head and post-merge `main` pass required CI |

The detailed contracts and normative order are in `docs/NEXT_DECISION.md`.

## Current Gaps

- PE-5's prior acceptance remains under repair pending required CI. The repair implements separate exact-local SLSA, SPDX, and `release_provenance.v2` manifest bundles; immutable attested bootstrap assets; real Cargo/Bun dependency inventory; explicit first/previous-release state; bounded archive validation; and verified transactional install/rollback outcomes. The grouped `PE5-CONTRACT-1` through `PE5-PUBLISH-1` milestones and PRs #210-#211 remain historical only where their semantics are weaker.
- `policy_simulator.rs` still relies on fixed estimates rather than trace-calibrated replay; this remains outside PE-5/PE-6 unless a packet demonstrates a prerequisite impact.
- PE-6's prior acceptance remains under repair pending required CI. The repair implements versioned owner-emitted evidence, monotonic observed duration, category outcomes derived only from scenario-specific checks, and a real PostgreSQL pre-commit interruption with atomic state/audit retry and cleanup. Stronger historical registry claims were either implemented or narrowed. PRs #212-#213 remain historical only where their semantics are weaker.
- External destructive testing, production provider calls, real target-repository corruption, and persistent signing secrets remain unauthorized.
- Historical v1 release fixtures remain readable for compatibility tests but are structurally limited to `verified_fixture`, never production `verified`. No external production identity, real tag, public release, or production installation was exercised by this repair.
- PostgreSQL owner drills are available only through the existing GitHub Actions `pg-tests` disposable service path; environments without that exact service identity report explicit `unsupported` evidence rather than pass, including arbitrary local `ACP_TEST_DATABASE_URL` values.
- Remote adapter support for the local runner and new external runtime frameworks remain deferred.

## Open Work Coordination

PR #207 is an independent, disabled-by-default GitHub Actions/Codex repository-maintenance orchestrator. It does not modify the Rust engine or the PE-5/PE-6 release-provenance and recovery owners. It does modify CI scripts/workflows, tests, and `docs/ARCHITECTURE_BOOK.md`, `docs/CURRENT_STATUS.md`, `docs/MODULE_MAP.md`, and `docs/RUNBOOK.md`.

Before #207 merges, it must refresh from current `main` and preserve:

- the final PE-4 evidence above;
- PE-5 activation and PE-6 routing;
- the PE-5/PE-6 ownership recorded in `docs/MODULE_MAP.md`;
- all orchestrator-specific additions from its own branch.

Any conflict is a documentation/CI-maintenance integration issue, not an engine-kernel conflict. #207 must remain emergency-stopped and inactive until its own exact-head CI, review, credentials, runner, and activation requirements are satisfied.

## Active Documentation

- `AGENTS.md`
- `CLAUDE.md`
- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Do not create another roadmap, status, policy, packet, or closeout document by default. Current direction belongs in `docs/NEXT_DECISION.md`; current facts belong here.
