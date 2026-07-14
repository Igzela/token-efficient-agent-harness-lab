# Current Status

Last updated: 2026-07-14.

## Summary

This repository is a local/small-team self-hosted agent workflow control plane. Rust `engine/` remains the sole runtime, API, and application-owned storage implementation. Active documents describe current facts and forward execution; merged PRs and repository history retain detailed stage history.

The repository has broad feature coverage, but several existing capabilities still require integration repair. In addition, the GitHub Issues/Actions → Vader Codex repository-maintenance orchestrator is merged but is not yet accepted for production task use: the first live GPT Web smoke reached the Vader worker and then failed closed before branch or PR creation.

## Verified Repository State

- PR #214 (`PE56-POST-SEAL-REPAIR-1`) is merged at `0d8127e3d779e54c58caf5d93e7589dd1a6df616`;
- PR #214 exact final head `ed5e033a5206d2ddfea2d48381217d0a04b4ceb3` passed exact-head CI run `29250861586`;
- PR #207 merged the disabled-by-default event-driven repository-maintenance orchestrator at `23187bb83dc32165d8982c79be1a1f7f818380a0`;
- PR #216 repaired Codex last-message handling and runner-readiness validation and merged at `2a42c011164765ba6c2dbe940c5a73900a7bb4b1`;
- PR #216 exact head `7210cd1943b075ef07c561f4804bca8230cffd60` passed canonical CI run `29308693744` with all seven required jobs successful;
- Issue #208 currently has only `agent-control` and `agent-emergency-stop`; orchestration and auto-merge enable labels are absent.

## Repository-Agent Smoke Status

GPT Web created bounded smoke Issue #217 with the sole allowed path `docs/agent-smoke-test.md`. The live chain successfully performed:

```text
GPT Web request
→ bounded Agent Task Issue
→ agent-ready intake
→ dispatcher claim
→ agent-worker workflow dispatch
→ agent-running / Vader worker entry
```

The task then transitioned to `agent-blocked` without creating `agent/issue-217`, a pull request, or exact-head CI evidence. The control Issue was returned to emergency stop and orchestration was disabled.

The exact worker failure cause is not yet established from durable repository evidence. Issue comments record only the claimed and dispatched states; they do not contain the workflow run/job identity or a bounded terminal failure reason. Do not infer a Codex, runner, network, token, or finalizer cause without reading the actual Actions and runner diagnostics.

Operational consequence:

- do not dispatch a production repository task through this orchestrator yet;
- keep Issue #208 emergency-stopped;
- repair timeout/failure observability and the demonstrated worker failure through `AGENT-SMOKE-REPAIR-1`;
- run a replacement bounded smoke through PR creation, exact-head CI, and independent review before declaring the GPT Web path operational.

The intended user interface remains natural language in GPT Web. The assistant, not the user, owns creation of the bounded Issue and the internal workflow parameters. This contract is documented in `README.md` and `AGENTS.md`, but activation remains blocked by the failed smoke.

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
| GitHub/Vader repository orchestrator | Code merged and runner path reached; first live smoke #217 blocked before branch/PR creation, so production use is disabled |
| Post-R7 wire/type governance | Implemented through `scripts/check_wire_codegen_drift.sh` |

## Confirmed Integration Gaps

### Repository-agent worker completion and evidence

The control, intake, dispatcher, and worker-entry path are live, but the first bounded task did not reach a branch or PR. The repair must identify the actual failed workflow step and add enough bounded evidence to distinguish queue delay, runner loss, Codex timeout/nonzero exit, artifact rejection, finalizer failure, and control-state interruption without exposing raw prompts, model output, credentials, or unbounded logs.

Required chain:

```text
bounded task
→ durable workflow/run identity
→ bounded worker timeout and terminal reason
→ capacity release
→ validated artifact
→ branch and PR
→ exact-head CI
→ independent review
```

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

## Active Execution Order

1. Execute `AGENT-SMOKE-REPAIR-1`: diagnose the actual #217 worker failure, add bounded timeout and durable failure/run evidence where missing, repair the root cause, and obtain full exact-head CI on one focused PR. Keep the orchestrator stopped throughout repair.
2. Execute `AGENT-SMOKE-VERIFY-1`: repeat the one-file smoke and require branch/PR creation, in-scope diff, exact-head seven-job CI, and independent review with auto-merge disabled.
3. Implement `PE2-RUNTIME-PRODUCER-1` on a new focused branch/PR.
4. Implement `PE4-EVIDENCE-ENTRY-1` on a separate focused branch/PR.
5. Implement `TOOL-DISCOVERY-BENCH-1` on a separate focused branch/PR.
6. Keep real release publication, production installation, destructive external fault injection, persistent signing secrets, and automatic live-provider workflow execution unauthorized unless explicitly approved later.

The normative packet definitions and acceptance gates are in `docs/NEXT_DECISION.md`.

## Active Documentation

- `README.md`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Do not create another roadmap, status, policy, packet, or closeout document by default. Current direction belongs in `docs/NEXT_DECISION.md`; current facts belong here.
