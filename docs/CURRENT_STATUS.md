# Current Status

Last updated: 2026-07-20.

## Summary

Default daily path remains local Agent → focused branch → PR → exact-head CI → independent review → manual squash merge (`SHIP-PR-DELIVERY-1`, PR #240). Open-source public surfaces through PR #253 are merged. Clean-environment external validation (`OSS-EXTERNAL-VALIDATION-1`, PR #256) is merged: `./scripts/external_validation.sh`, hosted Ubuntu/macOS self-validation, and `external_validation_report.v1` (engineering usability only — not external adoption evidence).

**Repository-agent (Issue → Vader → Codex CLI) path is parked.** Vader runner is `online`/`idle` after Mihomo left broken `台湾家宽-IEPL 02` for `香港-IEPL 01`. Replacement live-seal is **not** actively driven: parking Issue **#254** (same smoke *class* as #231→PR #232 and #235→PR #236; those PRs stay closed). Last smoke failed at ChatGPT Codex HTTP 403 (worker `29739968760`), not runner offline. Issue #208 remains `agent-control` + `agent-emergency-stop`. Active implementation continues on **PE7 local/fixture** and Ship PR work without Issue→CLI.

This repository is a local/small-team self-hosted Agent workflow control plane and research lab. Rust `engine/` remains the sole runtime, API, scheduler, policy, and application-owned storage implementation. The production-integration program through Agent Runtime AR-6 and product evolution PE-1 through PE-6 is merged. Durable memory, budget evidence, replay/promotion, managed external-runtime adapters, target-output authority, release provenance, and fault/recovery evidence are connected through existing bounded owners.

Controlled staging drills and the repaired disposable target-repository path passed. PR #237 is merged and accepted on exact reviewed head `068b2e9ac4bde16daea25bcb4846f7e26ba6cca9` with seven-job CI run `29628449688` and two independent complete-diff reviews. The remaining repository-agent acceptance work is operational: restore the existing Vader runner and complete one replacement documentation-only smoke through PR creation, exact-head CI, and independent review. Issue #208 remains stopped only until that bounded smoke begins; its temporary enable/restore is already authorized. Auto-merge remains off. The current authenticated OpenRouter embedding catalog still omits the potentially chargeable `request` price; merged PR #227 keeps that path pre-send-blocked. No provider POST, provider-backed benchmark, public release, or production installation has been completed.

A one-time bounded temporary lift of the Issue #208 emergency stop then ran a fresh documentation-only end-to-end smoke (Issue #235 → PR #236) and reached a bounded terminal state with a bounded smoke failure: the worker pushed branch `agent/issue-235` (head `042d2560cb626a96b0e8b4f477bbbc087902f1a1`) and opened PR #236 with exactly one allowed file, but the worker's finalize step failed with `CI_VERIFICATION_ERROR: exact-head CI run did not become observable` before the worker-dispatched CI run (`29573762624`) became observable to its read-acquisition check. All seven CI jobs on `29573762624` later completed `success` on the exact head `042d2560`. `agent-ci-monitor` was then dispatched via the PR #233 `workflow_dispatch` trigger (run `29574631602`) and produced `action: trigger_review`. The follow-up `agent-controller` `dispatch-review` (run `29574651817`) was rejected with `issue_not_active` because the worker's failure-handler had already moved Issue #235 to `agent-blocked` during the CI-observation race. The independent review could not bind to the exact head, but the full chain Vader → artifact → branch → PR → exact-head seven-job CI → trusted monitor `workflow_dispatch` was proven end-to-end. PR #236 was not merged. Auto-merge remained disabled. Issue #208 was restored to `agent-control` + `agent-emergency-stop` immediately after the bounded terminal result. Vader released capacity (`busy: false`).

PR #237 (squash-merged into `main` at `1947d4b555bd14b7f104c1fc9aba31747099cb88` after all seven CI jobs passed on the exact head `068b2e9ac4bde16daea25bcb4846f7e26ba6cca9`, run `29628449688`) repaired the repository-agent CI cancellation and capacity-leak blocker that the Issue #235 smoke exposed. The prior chain shared one `agent-orchestrator-state` concurrency group across all seven workflows and set `cancel-in-progress: ${{ inputs.command == 'emergency-stop' }}` on `agent-controller`; because GitHub Actions does not run `if: always()` failure-handlers for jobs cancelled by a concurrency group, an emergency-stop cancelled unrelated in-flight workflows and leaked their claimed capacity. PR #237 reverted every workflow to per-resource concurrency groups with `cancel-in-progress: false`, so emergency-stop now only sets the control-state flag and each workflow reconciles its own claim through its own `if: always()` finalizer. The PR also split `ci_verifier` into `acquire_exact_run` (bind the run without waiting) plus `wait_for_run_completion` (bounded poll with per-cycle binding revalidation), made production CI identity fail-closed on every missing field in `_validate_run_identity`, added durable `claimed` → `dispatched` claim lifecycle records written before label mutation, and added `reconcile_claimed_dispatch` plus idempotent `release_and_record_ci_terminal` terminal compensation that preserves `dispatched` claims for their own child-workflow compensation. Two independent complete-diff reviews passed. Issue #208 remained `agent-control` + `agent-emergency-stop` throughout; no provider POST, no unrelated dispatch, and auto-merge stayed off. A fresh documentation-only replacement smoke remains to be run after the existing Vader runner is restored; runner and egress restoration are authorized operational repair, not a governance hard stop. Issue #208 remains stopped until the bounded smoke begins.

A new research direction is documented: bounded recursive execution, then a controlled OpenCode external adapter, then an evidence-gated Harness evolution laboratory. `PE7-BOUNDED-RECURSIVE-EXECUTION-1` is merged and accepted via PR #239. `PE7-OPENCODE-EXTERNAL-ADAPTER-1` is merged and accepted via PR #255 (fixture-first default-off). Harness evolution remains unavailable as a product claim; no evolution gate exists, no candidate Harness has been generated or promoted, and the repository does not claim recursive self-improvement.

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
- the unique Vader runner unit is `actions.runner.Igzela-token-efficient-agent-harness-lab.Vader.service` under user `igzela`; after Mihomo `节点选择` left `台湾家宽-IEPL 02` for `香港-IEPL 01` and the service was restarted (2026-07-20), GitHub runner readiness reports `online`/`idle`/`ready:true`. ChatGPT Codex API access from this host currently returns HTTP 403 on tested HK nodes (blocks the Vader Codex worker even when the runner is online). Egress can regress on Clash Verge reload or node congestion;
- PR #230 repaired the repository-agent empty-workspace-output defect (Codex returned prose without editing due to an 80KB prompt with conflicting governance constraints) and split terminal attribution so successful-exit/no-changes now records `reason_code: no_workspace_changes` instead of generic `model_execution_failure`. The repair merged;
- bounded replacement smoke Issue #231 created PR #232 (`docs/agent-smoke-replacement-20260717-c.md` only) and exact-head canonical seven-job CI passed (run `29565496618`), but the GitHub-hosted `agent-ci-monitor` workflow that dispatches independent review did not fire because `workflow_run` is not dispatched for `workflow_dispatch`-triggered bot CI. PR #233 repaired the dispatch-trigger path (squashed into `main` at `282cfc6246cdd0bdd607f70a951e5709e73f9ffa`); a fail-closed smoke (Issue #234) reached the expected terminal state on run `29572989439`. A one-time bounded temporary lift of the emergency stop then ran a fresh end-to-end smoke (Issue #235 → PR #236) that proved Vader → artifact → branch → PR → exact-head seven-job CI green (run `29573762624`) → `agent-ci-monitor` `workflow_dispatch` (run `29574631602`, `action: trigger_review`), but the follow-up `dispatch-review` controller run `29574651817` was rejected with `issue_not_active` because the worker's failure-handler had already moved Issue #235 to `agent-blocked` during a CI-observation race. The smoke PR remains unmerged and Issue #208 is emergency-stopped;
- PR #237 (squash-merged into `main` at `1947d4b555bd14b7f104c1fc9aba31747099cb88` after all seven CI jobs passed on exact head `068b2e9ac4bde16daea25bcb4846f7e26ba6cca9`, run `29628449688`) repaired the CI cancellation/capacity-leak and CI-observation-race blockers exposed by the Issue #235 smoke. Two independent complete-diff reviews passed. Issue #208 remained `agent-control` + `agent-emergency-stop` throughout; no provider POST and no unrelated dispatch occurred; auto-merge stayed off. A fresh documentation-only replacement smoke remains to be run after the existing Vader runner is restored; runner and egress restoration are authorized operational repair, not a governance hard stop. Issue #208 remains stopped until the bounded smoke begins.
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

- restore the existing Vader service/egress, pass the repository-owned readiness checker, then run the bounded replacement smoke;
- keep Issue #208 stopped until that smoke begins, temporarily enable it only for the bounded smoke, and restore the emergency stop immediately afterward;
- keep Mihomo egress on a node that completes both `tokenghub` TLS and ChatGPT Codex API access; re-verify after any Clash reload;
- rerun the bounded readiness checker before any re-enable;
- complete one documentation-only replacement smoke through PR creation, exact-head CI, independent review, and manual merge decision before accepting the repository-agent path for normal bounded work. The Issue #235 smoke reached PR creation, exact-head seven-job CI green, and the trusted monitor `workflow_dispatch` path; the follow-up `dispatch-review` could not bind to the exact head because the worker moved Issue #235 to `agent-blocked` during a CI-observation race. PR #237 repaired that race at the orchestrator level by splitting `ci_verifier` into `acquire_exact_run` + `wait_for_run_completion` with per-cycle binding revalidation and by reverting to per-resource concurrency groups so emergency-stop no longer cancels unrelated workflows. The replacement smoke requires the uniquely named Vader runner to be restored to `online`/`idle`; recovery is authorized.

The intended interface remains ordinary language in GPT Web. The assistant, not the user, owns internal Issue/workflow parameters. Existing runner and egress recovery, bounded smoke execution, eligible manual merge, and continuation across `READY_FOR_EXECUTION` packets require no repeated permission.

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
| GitHub/Vader repository orchestrator | Implemented/default-off; PR #237 is merged and accepted; replacement smoke remains the final live-acceptance task after authorized Vader runner recovery |
| Dashboard | Functional; PR #225 is an independent presentation-only redesign |
| Post-R7 wire/type governance | Implemented through `scripts/check_wire_codegen_drift.sh` |
| PE7 bounded recursive execution | Implemented and merged via PR #239; default-off recursive admission, persistence, bounded evidence, and kill switch under existing scheduler/storage owners; Harness evolution still unavailable |
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

`PE7-BOUNDED-RECURSIVE-EXECUTION-1` is implemented and merged; it proceeded only in deterministic local/fixture mode and never enabled Issue #208, used the repository-agent smoke path, called a provider, mutated a real target repository, or altered evaluator, permission, budget, audit, target-output, merge, release, or rollback boundaries. The OpenCode adapter packet is now eligible; the Harness-evolution packet remains blocked until the OpenCode adapter is implemented and verified, and any evolution work remains fixture/local-only behind the same boundaries.

## Confirmed Integration Gaps

1. **Parked:** repository-agent replacement smoke (Issue **#254** / `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1`) until Codex/ChatGPT works on the Vader host without hard-fix or TLS weakening. Keep Issue #208 emergency-stopped. Do not treat this as the default development path.
2. Obtain current provider catalog evidence that satisfies exact model identity and every modeled applicable charge dimension before any provider POST.
3. Confirm disposable repository deletion remains limited to the already-absent `Igzela/acp-target-accept-20260716-1145` identity only.
Gaps (1)–(2) still block production repository-agent use and provider-backed acceptance. They do **not** block Ship PR work or fixture/local PE7 (recursive execution merged; OpenCode adapter is the active READY implementation lane).

## Open Work Coordination

- PR #225 remains open and presentation-only; PE7 recursive execution (#239), Ship PR (#240), public-surface PRs #241–#253, and clean-environment external validation (#256) are squash-merged.
- `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1` is **parked** on Issue #254 (runner restored; Codex 403 blocks smoke). Historical smoke PRs #232/#236 remain closed. Issue #208 emergency-stopped. No active Issue→CLI work.
- `PE7-OPENCODE-EXTERNAL-ADAPTER-1` COMPLETE via PR #255 (fixture-first default-off).
- `OSS-EXTERNAL-VALIDATION-1` COMPLETE via PR #256 (exact reviewed head `a3f6744616a75b36d185534993f21c2839b1ea76`; seven-job exact-head CI runs `29749430115` / `29749437488`; Ubuntu+macOS external-validation run `29749430155`; independent complete-diff review on PR comments). Not external adoption evidence.
- `PE7-HARNESS-EVOLUTION-LAB-1` is the next PE7 lab packet when selected; evolution product claims remain unavailable.
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
