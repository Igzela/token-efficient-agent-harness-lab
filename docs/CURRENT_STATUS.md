# Current Status

Last updated: 2026-07-17.

## Summary

This repository is a local/small-team self-hosted Agent workflow control plane and research lab. Rust `engine/` remains the sole runtime, API, scheduler, policy, and application-owned storage implementation. The production-integration program through Agent Runtime AR-6 and product evolution PE-1 through PE-6 is merged. Durable memory, budget evidence, replay/promotion, managed external-runtime adapters, target-output authority, release provenance, and fault/recovery evidence are connected through existing bounded owners.

Controlled staging drills and the repaired disposable target-repository path passed. External live acceptance advanced: PR #230 repaired the empty-workspace-output defect and inaccurate terminal attribution in the repository-agent orchestrator, merged after exact-head CI. A new documentation-only smoke (Issue #231 → PR #232) then proved intake, Vader worktree, validated non-empty artifact, branch push, and PR creation. The smoke PR's exact-head canonical seven-job CI passed (run `29565496618`), but the GitHub-hosted `agent-ci-monitor` workflow that dispatches independent review did not fire because `workflow_run` events are not dispatched for `workflow_dispatch`-triggered `tests.yml` runs on PR branches triggered by `github-actions[bot]`. The repair PR #233 added a `workflow_dispatch` trigger to `agent-ci-monitor` with explicit `issue`/`pr`/`head_sha`/`ci_run_id` inputs and a worker finalizer step that dispatches it after CI is green; the PR was squash-merged into `main` at `282cfc6246cdd0bdd607f70a951e5709e73f9ffa` after all seven CI jobs passed on the exact head `62de8eb389b488f80a756160fac7ab6f4113cef2`, with Issue #208 still emergency-stopped. A follow-up smoke (Issue #234) was dispatched through `agent-controller` and reached the expected fail-closed terminal state (`CONTROL_STATE_ERROR: agent control emergency stop is active`) on run `29572989439`, proving the chain halts correctly while the emergency stop is active. The smoke PR was never opened. Auto-merge remains off. The current authenticated OpenRouter embedding catalog still omits the potentially chargeable `request` price; merged PR #227 keeps that path pre-send-blocked. No provider POST, provider-backed benchmark, public release, or production installation has been completed.

A one-time bounded temporary lift of the Issue #208 emergency stop then ran a fresh documentation-only end-to-end smoke (Issue #235 → PR #236) and reached a bounded terminal state with a bounded smoke failure: the worker pushed branch `agent/issue-235` (head `042d2560cb626a96b0e8b4f477bbbc087902f1a1`) and opened PR #236 with exactly one allowed file, but the worker's finalize step failed with `CI_VERIFICATION_ERROR: exact-head CI run did not become observable` before the worker-dispatched CI run (`29573762624`) became observable to its read-acquisition check. All seven CI jobs on `29573762624` later completed `success` on the exact head `042d2560`. `agent-ci-monitor` was then dispatched via the PR #233 `workflow_dispatch` trigger (run `29574631602`) and produced `action: trigger_review`. The follow-up `agent-controller` `dispatch-review` (run `29574651817`) was rejected with `issue_not_active` because the worker's failure-handler had already moved Issue #235 to `agent-blocked` during the CI-observation race. The independent review could not bind to the exact head, but the full chain Vader → artifact → branch → PR → exact-head seven-job CI → trusted monitor `workflow_dispatch` was proven end-to-end. PR #236 was not merged. Auto-merge remained disabled. Issue #208 was restored to `agent-control` + `agent-emergency-stop` immediately after the bounded terminal result. Vader released capacity (`busy: false`).

A new research direction is documented: bounded recursive execution, then a controlled OpenCode external adapter, then an evidence-gated Harness evolution laboratory. This is an approved forward plan only. `PE7-BOUNDED-RECURSIVE-EXECUTION-1` (the AR7 runtime-extension slice) and the later PE7 packets are not implemented, no evolution gate exists, no candidate Harness has been generated or promoted, and the repository does not claim recursive self-improvement.

## Verified Repository State

- current documentation work started from `main` commit `0ff756acb213ca68944d1317a77f85c7b0b1c2a1`;
- PR #214 merged the active PE-5/PE-6 post-seal repair;
- PR #207 merged the disabled-by-default event-driven repository-maintenance orchestrator;
- PR #216 repaired Codex last-message handling and runner-readiness validation;
- PR #220 merged production Agent Runtime and tool-policy integration;
- PR #221 merged durable memory plus the PE-2/PE-4 production loop;
- PR #222 merged the managed external runtime, benchmark, orchestrator evidence repair, and local acceptance seal;
- PR #223 and PR #224 merged provider embedding receipt, transport, authorization, identity, and pricing safety repairs;
- PR #226 repaired target-output duplicate delivery and restart idempotency;
- exact-head CI evidence recorded by the merged PRs remains the implementation evidence; documentation changes do not create new implementation evidence;
- PR #225 remains open and presentation-only, touching the Dashboard visual system rather than runtime or authority;
- repository Actions may create/approve pull requests and the required secret names exist, but Issue #208 remains emergency-stopped with orchestration and auto-merge disabled;
- the unique Vader runner unit is `actions.runner.Igzela-token-efficient-agent-harness-lab.Vader.service` under user `igzela`; after switching Mihomo group `节点选择` away from the broken `台湾家宽-IEPL 02` pin to a working egress, `tokenghub` TLS and broker session creation succeed and repository readiness reports online/idle. Egress stability is runtime-only and can regress on Clash Verge reload or node congestion;
- PR #230 repaired the repository-agent empty-workspace-output defect (Codex returned prose without editing due to an 80KB prompt with conflicting governance constraints) and split terminal attribution so successful-exit/no-changes now records `reason_code: no_workspace_changes` instead of generic `model_execution_failure`. The repair merged;
- bounded replacement smoke Issue #231 created PR #232 (`docs/agent-smoke-replacement-20260717-c.md` only) and exact-head canonical seven-job CI passed (run `29565496618`), but the GitHub-hosted `agent-ci-monitor` workflow that dispatches independent review did not fire because `workflow_run` is not dispatched for `workflow_dispatch`-triggered bot CI. PR #233 repaired the dispatch-trigger path (squashed into `main` at `282cfc6246cdd0bdd607f70a951e5709e73f9ffa`); a fail-closed smoke (Issue #234) reached the expected terminal state on run `29572989439`. A one-time bounded temporary lift of the emergency stop then ran a fresh end-to-end smoke (Issue #235 → PR #236) that proved Vader → artifact → branch → PR → exact-head seven-job CI green (run `29573762624`) → `agent-ci-monitor` `workflow_dispatch` (run `29574631602`, `action: trigger_review`), but the follow-up `dispatch-review` controller run `29574651817` was rejected with `issue_not_active` because the worker's failure-handler had already moved Issue #235 to `agent-blocked` during a CI-observation race. The smoke PR remains unmerged and Issue #208 is emergency-stopped;
- the current authenticated OpenRouter embedding catalog proves the fixed model identity and explicit zero `prompt`/`completion` prices, but omits the potentially chargeable `request` price; merged PR #227 records Applicable/NotApplicable/Unknown receipt evidence without treating omitted fields as zero, and the live request remains blocked before POST;
- disposable repository `Igzela/acp-target-accept-20260716-1145` no longer resolves via GitHub API/GraphQL (already absent or previously deleted); do not delete any other repository.

## Repository-Agent Smoke Status

The first GPT Web smoke, Issue #217, proved this partial chain:

```text
GPT Web request
-> bounded Agent Task Issue
-> intake and unique claim
-> controller dispatch
-> Vader worker
-> validated artifact
-> branch push
```

PR creation then failed because Actions PR creation was disabled at that time. PR #222 added setting preflight, bounded worker timeout, terminal phase/reason evidence, and capacity-safe cleanup. The repository setting, runner online/idle readiness, empty-workspace repair, and `agent-ci-monitor` `workflow_dispatch` trigger (PR #233) are merged. The replacement smoke (Issue #235 → PR #236) reached a bounded terminal state with a bounded smoke failure: the worker did not wait long enough for its own dispatched CI to become observable, so the failure-handler moved Issue #235 to `agent-blocked` before the `agent-controller` `dispatch-review` could run. The chain Vader → artifact → branch → PR → exact-head seven-job CI green → trusted monitor `workflow_dispatch` is proven end-to-end; the independent review did not bind to the exact head because of the CI-observation race.

Operational consequence:

- do not dispatch production repository work through the orchestrator;
- keep Issue #208 emergency-stopped and both enable labels absent;
- keep Mihomo egress on a node that completes both `tokenghub` TLS and ChatGPT Codex API access; re-verify after any Clash reload;
- rerun the bounded readiness checker before any re-enable;
- complete one documentation-only replacement smoke through PR creation, exact-head CI, independent review, and manual merge decision before removing the restriction. The Issue #235 smoke reached PR creation, exact-head seven-job CI green, and the trusted monitor `workflow_dispatch` path; the follow-up `dispatch-review` could not bind to the exact head because the worker moved Issue #235 to `agent-blocked` during a CI-observation race. A further repair is required to make the worker wait for its own dispatched CI to become observable before recording the terminal state, before the replacement smoke can complete through independent review;

The intended interface remains ordinary language in GPT Web. The assistant, not the user, owns internal Issue/workflow parameters once the path is accepted.

## Capability Status

| Capability | Current status |
|---|---|
| Dispatch kernel and V2 output authority | Complete |
| Adaptive Fusion through AF-7 | Implemented; single/fallback/Fusion paths exist, while final authority convergence and complexity/risk-driven automatic selection remain deferred |
| Agent Runtime through AR-6 | Production-managed through typed `agent_step` plans, one-action decisions, scheduler leases, atomic receipts, bounded concurrency, proposals, handoff, review/debate, and operator evidence |
| Trusted Local Autonomous Execution through IAE-3 | Complete within its documented local/default-off gates |
| PE-1 Token Efficiency Regression Lab | Connected through scorecard persistence, reports, batches, trends, APIs, SDKs, and Dashboard |
| Durable cross-run memory | Connected with versioned scope, conflict/tombstone/expiry semantics, bounded retrieval, runtime context injection, audit, backup, and SQLite/PostgreSQL parity; provider embedding remains fail-closed |
| PE-2 Budget Intelligence | Connected to normalized owner-backed usage, immutable forecast/anomaly evidence, typed pause, and recovery |
| PE-3 Operator Decision Center | Connected to typed inspect, acknowledge, approval, retry, pause/recovery, and exact-snapshot rollback owners; no generic executor |
| PE-4 Trace-backed Policy Replay | Connected to recorder-owned traces, immutable replay, shadow/canary evidence, explicit evidence-chain promotion, and rollback |
| PE-5 Release Provenance | Repaired v2 boundary merged; no new public release or production installation was exercised by the repair |
| PE-6 Fault Injection and Recovery Drills | Repaired owner-evidence boundary merged; controlled disposable staging drills passed; no production resource was targeted |
| Managed LangGraph external runtime | One bounded adapter invocation per Rust-leased node; fixture/guarded-live modes; Python owns no queue or product authority |
| Native/LangGraph efficiency benchmark | Deterministic fixture evidence connected; no provider-backed result verified |
| Target-repository output | Disposable acceptance passed after PR #226 with unchanged target `main`, one external delivery, and duplicate/restart reuse |
| GitHub/Vader repository orchestrator | Implemented/default-off; PR #230 merged the empty-workspace repair; smoke #231 reached PR #232 + green exact-head CI, but independent review dispatch failed `action_required` |
| Dashboard | Functional; PR #225 is an independent presentation-only redesign |
| Post-R7 wire/type governance | Implemented through `scripts/check_wire_codegen_drift.sh` |
| PE7 bounded recursive execution | Approved in active documents; not implemented; no recursive tree admission or lineage schema exists yet |
| PE7 Harness evolution laboratory | Approved as a future default-off fixture/local lane; blocked on recursive execution; no candidate archive, evaluator vault, or promotion path exists yet |
| PE7 meta-improver experiment | Deferred/blocked; no Level-2 or `Improvement@K` evidence exists |

## Existing Architecture Relevant to PE7

The repository already has most of the control-plane prerequisites:

- `AgentAction::ProposeChildTask`, `ChildTaskProposal`, `agent_proposals`, and workflow graph mutation;
- one-step `AgentStepExecutor` decisions and AR-4 scheduler concurrency bounds;
- exactly-once `agent_action_receipts` across retry, restart, and concurrent claims;
- bounded durable memory and metadata-only retrieval evidence;
- owner-backed dispatch, workflow, provider, tool, budget, replay, and scorecard evidence;
- isolated app-owned workspaces/worktrees, verification, approval, patch/branch output, and compensation;
- explicit operator decisions, snapshots, promotion, rollback, kill switches, and audit.

The missing recursive-execution capabilities are persistent root/parent/depth identity, whole-tree budgets, ancestor-cycle and duplicate-objective detection, strict child capability reduction, recursive-specific gates/reason codes, and bounded tree evidence.

The missing Harness-evolution capabilities are structured mutation proposals, isolated candidate version identity, lineage/archive storage, equal-budget baselines, sealed holdout discipline, evaluator-integrity enforcement, candidate Pareto selection, and PR-only promotion binding. These gaps are planned in `docs/NEXT_DECISION.md`; they must not be described as implemented.

## Connected Production Boundaries

### Agent Runtime and tools

An authenticated confirmed `agent_step` plan creates ordinary workflow state. The Rust scheduler is the sole owner of admission, lease, retry, cooldown, pause/resume, restart, and concurrency. A provider decision performs one admitted call and returns one strict bounded `agent_action.v1`; it cannot create an internal loop. Command/CLI nodes use the same app-owned tool-policy wrapper. Configured allowlists are authoritative, approval-required tools consume one exact authorization, and post-effect uncertainty remains explicit and non-retryable.

### Durable memory, budget, and replay

The scheduler builds bounded context from run state, recent history, run-scoped digest, and immutable retrieved references. Provider embeddings reuse the existing symbolic-credential, catalog, pricing, reservation, receipt, audit, timeout, circuit-breaker, and kill-switch owners and remain blocked without complete current evidence. Budget and replay producers use persisted cursors and rotating retry sets inside the existing scheduler, not a second queue. Replay remains read-only until explicit current-state-bound promotion.

### External runtime and target output

A `langgraph_external` node performs one adapter invocation under a Rust lease. Fixture mode is deterministic and network-free. Target output remains isolated and approval-bound; it may create a controlled branch/patch/PR path but cannot write registered target `main`, merge, deploy, or release.

### Recursive/evolution boundary

`PE7-BOUNDED-RECURSIVE-EXECUTION-1` may proceed independently only in deterministic local/fixture mode. It may not enable Issue #208, depend on the repository-agent smoke path, call a provider, mutate a real target repository, or alter evaluator, permission, budget, audit, target-output, merge, release, or rollback boundaries. OpenCode adapter and Harness-evolution packets remain blocked until recursive execution is implemented and verified.

## Confirmed Integration Gaps

1. Complete one documentation-only replacement smoke through PR creation, exact-head CI, and independent review while keeping auto-merge off; keep Issue #208 emergency-stopped until that succeeds.
2. Obtain current provider catalog evidence that satisfies exact model identity and every modeled applicable charge dimension before any provider POST.
3. Confirm disposable repository deletion remains limited to the already-absent `Igzela/acp-target-accept-20260716-1145` identity only.
4. Implement and verify `PE7-BOUNDED-RECURSIVE-EXECUTION-1` before OpenCode adapter or Harness evolution.

The first three gaps continue to block production repository-agent use and provider-backed acceptance. They do not prevent fixture/local recursive-execution implementation.

## Open Work Coordination

- PR #225 is the only known open PR and is presentation-only.
- `PE7-BOUNDED-RECURSIVE-EXECUTION-1` is the first eligible independent implementation packet.
- `PE7-HARNESS-EVOLUTION-LAB-1` follows only after the recursive packet merges and active state is refreshed.
- provider-backed evolution, model-weight updates, evaluator/task-generator co-evolution, automatic multi-lineage recombination, and production continuous self-update remain deferred.
- no new public tag, release, deployment, production installation, destructive production fault, provider call, protected-branch write, or persistent signing secret is authorized by this direction.

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

Do not create another roadmap, status, policy, packet, or closeout document by default. Current direction belongs in `docs/NEXT_DECISION.md`; current facts belong here; durable implemented architecture belongs in `docs/ARCHITECTURE_BOOK.md`; only proven operator procedures belong in `docs/RUNBOOK.md`.
