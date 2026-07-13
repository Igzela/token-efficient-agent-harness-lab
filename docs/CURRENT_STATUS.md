# Current Status

Last updated: 2026-07-13.

## Summary

This repository is a local/small-team self-hosted agent workflow control plane. Rust `engine/` remains the sole runtime, API, and application-owned storage implementation. Active documents describe current facts and forward execution; merged PRs and repository history retain detailed stage history.

The repository has broad feature coverage, but a 2026-07-13 call-site and owner-path audit found that several recently acceptance-sealed capabilities are not yet connected end to end. The next objective is integration repair, not a new product phase.

## Verified Repository State

- current observed `main` merge commit: `0d8127e3d779e54c58caf5d93e7589dd1a6df616`;
- PR #214 (`PE56-POST-SEAL-REPAIR-1`) is merged;
- exact final PR head: `ed5e033a5206d2ddfea2d48381217d0a04b4ceb3`;
- exact-head CI run `29250861586` completed successfully;
- the available GitHub connector did not expose a separate post-merge `main` workflow run for the merge commit, so no additional post-merge CI claim is made here.

## Capability Status

| Capability | Current status |
|---|---|
| Dispatch kernel and V2 output authority | Complete |
| Adaptive Fusion through AF-7 | Implemented; evidence-chain promotion entry remains disconnected as described below |
| Agent Runtime through AR-6 | Complete and sealed |
| Trusted Local Autonomous Execution through IAE-3 | Complete |
| PE-1 Token Efficiency Regression Lab | Complete and connected through scorecard persistence, read APIs, Dashboard, reports, batches, and trends |
| PE-2 Budget Intelligence and Anomaly Auto-Pause | Contracts, persistence, read surfaces, and pause consumers exist; runtime evidence production is not connected |
| PE-3 Operator Decision Center | Complete and connected to existing approval, workflow, retry, pause, and recovery owners |
| PE-4 Trace-backed Policy Replay | Replay, shadow, canary, and promotion validators exist; production replay generation and safe promotion entry are not connected |
| PE-5 Release Provenance | Post-seal repair merged in PR #214; exact-head CI passed; no real public release or production installation was exercised |
| PE-6 Fault Injection and Recovery Drills | Post-seal repair merged in PR #214; owner-emitted drills are wired into existing test/CI paths; no destructive external testing is authorized |
| Post-R7 wire/type governance | Implemented through `scripts/check_wire_codegen_drift.sh` |

## Confirmed Integration Gaps

### PE-2 runtime evidence producer

`build_budget_forecast` and `detect_budget_anomaly` are implemented and tested, and `LocalProductStore` can persist/read their artifacts. Auto-pause and operator decisions can consume a persisted anomaly artifact. However, no production runtime, HTTP, CLI, or scheduler owner currently derives forecast/anomaly evidence from posted provider/workflow usage and records it.

Required chain:

```text
owner-backed usage evidence
→ deterministic forecast/anomaly derivation
→ immutable budget evidence artifact
→ existing read/API/Dashboard surfaces
→ existing policy-gated pause and operator recovery owners
```

The repair must reuse existing provider audit, workflow run, budget evidence, pause, audit, and recovery owners. It must not create a second scheduler, pause authority, storage model, or synthetic success path.

### PE-4 replay and promotion entry

`record_offline_replay`, shadow comparison, canary binding, and `promote_adaptive_fusion_policy_with_evidence_chain` exist. Their safe evidence-chain path has no production HTTP, operator, CLI, or runtime entry. The current online observation path still calls the legacy auto-promotion method, which intentionally returns `complete_evidence_chain_required`.

Required chain:

```text
owner-backed dispatch-history traces
→ replay eligibility and offline replay artifact
→ derived shadow comparison
→ bounded canary evidence
→ explicit confirmation and permission
→ evidence-chain promotion through the existing policy/snapshot/rollback owner
```

Do not restore the old observation-only promotion shortcut.

### Tool discovery benchmark

The tool registry and PE-1 regression evidence exist independently. There is no deterministic static-all versus retrieve-Top-K tool selection benchmark, no required-tool recall/selection precision evidence, and no bridge from tool discovery results into PE-1 scorecards and regression reports.

Required chain:

```text
existing tool descriptors
→ deterministic bounded retrieval/Top-K selection
→ paired static-all and retrieved-tool runs
→ quality, recall, precision, token, latency, and cost evidence
→ existing PE-1 scorecard/regression owners
```

This is a benchmark and evidence feature first. It does not authorize dynamic production tool execution.

### Local runner boundary

The workflow-owned `LocalRunnerValidationExecutor` intentionally uses the Stub provider and persists bounded scorecards. Live provider execution remains an explicit local CLI/operator path. This separation is a current safety boundary, not a defect. Do not connect ordinary workflow execution directly to a live provider without a separate authority decision, explicit confirmation, budget binding, and kill path.

## Open Work Coordination

PR #207 is an independent, disabled-by-default GitHub Actions/Codex repository-maintenance orchestrator. It remains open and must not merge in its currently audited state. Known code-level blockers include invalid PR-number extraction, CI-repair import/path failure, old-head verification after a repair push, a `setup-controls` command mismatch, review-state capacity leaks, a missing Issue scope marker/pre-dispatch validation, and duplicate exact-head CI dispatch after PAT pushes.

PR #207 must remain emergency-stopped and inactive. Its repair must preserve the current PE-4, PE-5, PE-6, integration-gap, and ownership documentation when refreshing from `main`.

## Active Execution Order

1. Repair PR #207 on its existing branch and obtain fresh exact-head CI and independent review. Do not merge without separate user authorization.
2. Implement `PE2-RUNTIME-PRODUCER-1` on a new focused branch/PR after refreshing from the then-current `main`.
3. Implement `PE4-EVIDENCE-ENTRY-1` on a separate focused branch/PR.
4. Implement `TOOL-DISCOVERY-BENCH-1` on a separate focused branch/PR.
5. Keep real release publication, production installation, destructive external fault injection, persistent signing secrets, and automatic live-provider workflow execution unauthorized unless explicitly approved later.

The normative packet definitions and acceptance gates are in `docs/NEXT_DECISION.md`.

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