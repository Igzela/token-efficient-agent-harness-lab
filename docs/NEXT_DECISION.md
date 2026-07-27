# Next Decision

Last updated: 2026-07-27.

## Current Direction

The repository optimizes one outcome:

> Under non-negotiable quality, safety, traceability, compatibility, and rollback constraints, increase verifiable and reusable task delivery per unit of total lifecycle cost.

Quality, safety, integrity, authority, and rollback are hard gates. Token use, monetary cost, latency, engineering effort, maintenance surface, and expected reuse are optimization evidence only after those gates pass.

Do not substitute feature count, model/provider count, Dashboard completeness, PR creation, or fixture success for product capability or learning.

The authoritative order is:

```text
provider-free RWE authority reconciliation (#300)
→ observation-only reconciliation (#301)
→ context-capsule automation
→ one bounded live Golden Path managed acceptance
→ first frozen Real Workload Evidence baseline
→ Architecture Convergence AC1–AC7
→ identical-corpus replay
→ Level-2 GO/NO-GO
→ bounded Level-2 controller only on GO
→ separately authorized Meta Improver experiment
→ Dashboard #225 last
```

Do not skip RWE and begin Architecture Convergence or Level-2 early. Provider-free fixture completion is not live acceptance. Context-capsule automation is a transport and freshness prerequisite, not a runtime authority or evidence substitute.

## Active Routing

1. `PE7-RWE-AUTHORITY-RESTACK-1` — `IN_PROGRESS`.
2. `PE7-OBSERVATION-RESTACK-1` — `BLOCKED_PREREQUISITE`.
3. `PE7-CONTEXT-CAPSULE-AUTOMATION-1` — `BLOCKED_PREREQUISITE`.
4. `PE7-PRODUCT-GOLDEN-PATH-1` — `BLOCKED_PREREQUISITE`.
5. `PE7-REAL-WORKLOAD-EVIDENCE-1` — `BLOCKED_PREREQUISITE`.
6. `PE7-ARCHITECTURE-CONVERGENCE-1` — `BLOCKED_PREREQUISITE`.
7. `PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1` — `BLOCKED_PREREQUISITE`.
8. `PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1` — `BLOCKED_PREREQUISITE`.
9. `PE7-META-IMPROVER-EXPERIMENT-1` — `BLOCKED_PREREQUISITE`.

## Packet States

- `READY_FOR_EXECUTION` — prerequisites and authority are sufficient to begin.
- `BLOCKED_PREREQUISITE` — a named earlier evidence or authority condition is incomplete.
- `DECISION_REQUIRED` — safe authority cannot be derived automatically.
- `IN_PROGRESS` — one current branch/PR board owns the work.
- `COMPLETE` — merged, verified, independently reviewed, and documented.

Historical compatibility labels retained for handoff checks only: Packet PR207-REPAIR-1; Packet PE2-RUNTIME-PRODUCER-1; Packet PE4-EVIDENCE-ENTRY-1; Packet TOOL-DISCOVERY-BENCH-1. They are not active routing.

## Open PR Coordination

- PR #299 is merged and accepted; it supersedes #297/#298, which should close without merge.
- PR #300 is the current earliest eligible provider-free surface and cannot establish a live baseline.
- PR #301 follows as observation-only CC Switch adaptation and must be mechanically restacked without importing authority.
- PR #225 is presentation-only and remains last.

The immediate engineering order is to independently accept #300, then mechanically restack and accept #301, then complete the bounded context-capsule automation packet before any separately authorized live Golden Path task. Do not begin live RWE or later stages.

## Evidence Required for Every Engineering Board

Each coherent board must return a bounded `implementation_cost_receipt` in its final report. This is review evidence, not a new runtime store or budget authority.

Record when available:

```text
agent_sessions
review_cycles
repair_iterations
ci_runs
ci_compute_minutes
files_changed
schema_migrations
compatibility_adapters_added
authority_boundaries_touched
external_dependencies_added
rollback_complexity
known_maintenance_surface
expected_reuse_count
cost_or_measurement_unavailable_fields
```

The receipt may begin as a report/document contract. Persisting or automating it requires a later reviewed design and must reuse existing evidence owners.

A Level-2 GO decision requires more than runtime token improvement. It must consider comparable quality/safety, provider/token/latency/cost evidence, implementation and review cost, migration/rollback risk, maintenance surface, authority growth, failure recovery, expected reuse, and realistic implementation feasibility.

A change that reduces tokens but increases total lifecycle cost or weakens reliability is not an efficiency improvement.

## Common Execution Protocol

- Refresh actual `main`, open PR heads, CI, reviews, active documents, and overlapping ownership before work.
- Generate a fresh context capsule from the confirmed accepted baseline; treat it as stale when `main`, PR head, CI, review, or canonical documents change.
- Use one Agent session per coherent board when practical, with internal commit boundaries rather than repeated approval interruptions.
- Do not combine unrelated authority surfaces into one unreviewable commit.
- A new head invalidates earlier CI and review conclusions.
- Reuse the existing scheduler, executor, worktree, verification, artifact, approval, output, replay, scorecard, audit, and `LocalProductStore` owners.
- Bind authority from persisted current owners, never caller assertions.
- Preserve SQLite/PostgreSQL parity, atomicity, restart, concurrency, idempotency, cancellation, lease ownership, late-write refusal, and rollback.
- Keep provider execution off in CI; keep target `main` unchanged; keep auto-merge disabled.
- No Agent may self-approve risk, spend, merge, release, deployment, or production adoption.
- Finish focused/full checks, exact-head CI, complete-diff review, handoff validation, and rollback review before merge.

## Golden Path Acceptance Gate

A live managed task may start only when all of these are current and exact:

- accepted decision and residual-risk hashes;
- authenticated non-fixture operator principal and required scopes;
- separate one-use spend authorization;
- parent-only credential that never enters the child;
- exact executable path/version/SHA;
- exact provider kind/host/base URL/admitted paths/model;
- exact ProductTask/workflow/node/attempt identity;
- exact target repository and target-main SHA;
- request/retry/token/time/cost contract;
- Draft-PR-only output, no auto-merge, no release/deploy;
- gateway/session usage reconciliation;
- cancellation, cleanup, rollback, approval, output-confirmation, and terminal-evidence owners;
- a fresh context capsule bound to the current accepted-main SHA, active PR exact head, workflow evidence, review observation time, and next permitted action.

Codex remains `mediation_hardened_partial`. Retry identity, product-enforced loopback-only network confinement, and host namespace limitations remain explicit residual risks unless separately proved.

## Hard Stops

Stop before any of the following:

- secret, credential, raw prompt/output/transcript, private path, or repository-content exposure;
- second runtime, scheduler, store, evaluator, budget, approval, output, audit, rollback, or context-authority owner;
- caller-asserted authority, stale or conflicting identity, duplicate effect, late write, missing lease, or outcome-unknown treated as success;
- provider call in CI;
- target-default-branch write, auto-merge, merge, release, deployment, installation, or production adoption;
- unreviewed schema migration or SQLite/PostgreSQL semantic divergence;
- performance, cost, or learning claim without comparable evidence.

## Packet PE7-RWE-AUTHORITY-RESTACK-1 — provider-free PR #300 reconciliation

**State:** `IN_PROGRESS`

**Owned PR:** #300

PR #299 is the accepted authority foundation. Mechanically restack PR #300 onto accepted `main`, reconcile it against the final v33 store/spend/lease semantics, remove stale duplicated assumptions, run complete exact-head CI and independent review, and stop. This packet is provider-free and cannot establish a live RWE baseline or authorize a provider call.

## Packet PE7-OBSERVATION-RESTACK-1 — observation-only PR #301 reconciliation

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-AUTHORITY-RESTACK-1

**Owned PR:** #301

After PR #300 is accepted, mechanically restack PR #301 as an observation-only layer. It may normalize usage and pricing observations but must not import authority, credentials, proxy ownership, budget ownership, or live execution.

## Packet PE7-CONTEXT-CAPSULE-AUTOMATION-1 — exact-head publication and session injection

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-OBSERVATION-RESTACK-1

Phase 1 is already accepted through PR #302: `START_HERE.md` owns navigation and `scripts/project_context.py` generates an on-demand fail-closed Markdown or JSON transport view. This packet adds automation only; it must not create a new status database, current-state owner, authorization owner, or committed dynamic `latest context` file.

Required result:

- generate once per terminal exact-head workflow, not once per job;
- bind the capsule to accepted-main SHA, active packet, owned PR exact head, workflow run, complete required-check matrix, exact-head review/objection observation, and observation time;
- publish only a short-lived workflow artifact and/or job summary;
- inject or fetch a fresh capsule at the start of repository-controlled implementation, CI-repair, and review sessions;
- mark evidence unavailable rather than guessing and invalidate the view whenever `main`, head, CI, review, or canonical documents change;
- preserve secret, raw prompt/output/transcript, private-path, and repository-content redaction;
- reuse `START_HERE.md`, `scripts/project_context.py`, its tests, and the handoff checker as the sole navigation/transport owners.

This packet proves context freshness and routing only. It cannot authorize provider spend, live execution, output, merge, release, deployment, RWE acceptance, or a later packet.

## Packet PE7-PRODUCT-GOLDEN-PATH-1 — accepted authority and live residual seal

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-CONTEXT-CAPSULE-AUTOMATION-1

The provider-free authority foundation from PR #299 is merged and accepted. This live packet becomes eligible only after PR #300, PR #301, and context-capsule automation are independently accepted. Completion requires one separately authorized bounded live managed coding task that reaches verification, artifact, current approval, separate output confirmation, `acp/*` Draft PR, unchanged target `main`, reconciled usage, cleanup, and exact terminal evidence.

The live task requires a separate current-session spend authorization and parent-only credential. No repository prompt, fixture result, merged authority code, capsule, or prior test run grants that live authority.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-1 — first bounded baseline

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-PRODUCT-GOLDEN-PATH-1

PR #300 may prepare provider-free corpus, authorization, runner, and evidence contracts, but live RWE requires accepted Golden Path terminal evidence plus a separately persisted one-use RWE spend envelope.

The corpus must be real, versioned, hash-bound, replayable, and frozen before Architecture Convergence. Each task binds exact source repository/commit or fixture tree, task definition/reference, allowed mutable surface, verification, expected class, output bounds, timeout/cancel behavior, executor identity, budget, and cleanup.

The baseline records quality/failure class, request/retry/token/latency/cost-source semantics, timeout/cancel/pause/kill/restart/outcome-unknown, SQLite/PostgreSQL parity, approval/output/target-main/Draft-PR/terminal evidence, and the implementation-cost receipt.

## Packet PE7-ARCHITECTURE-CONVERGENCE-1 — compatibility convergence

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-REAL-WORKLOAD-EVIDENCE-1

Implement incrementally:

1. AC1 unified process supervision.
2. AC2 typed execution boundary.
3. AC3 Golden Path responsibility split.
4. AC4 transaction-scoped domain views.
5. AC5 runtime composition.
6. AC6 Rust-authoritative API/SDK/Dashboard schema convergence.
7. AC7 obsolete-abstraction cleanup after all callers and evidence migrate.

Each packet changes one coherent ownership boundary, preserves compatibility and rollback, and records implementation cost. It must not create a second scheduler, store, budget, approval, output, evidence, or rollback owner.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1 — post-convergence comparison

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-ARCHITECTURE-CONVERGENCE-1

Replay the identical frozen corpus. Compare quality/failure classifications, request/retry/token/latency/cost evidence, restart/recovery, approval/output/terminal behavior, implementation cost, maintenance surface, and rollback burden. Do not tune the corpus from convergence results.

## Packet PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1 — bounded multi-generation decision

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1

First record an evidence-backed GO/NO-GO. On GO only, implement a default-off bounded laboratory controller with small fixed generation/candidate/evaluation limits, deterministic global budgets, one selected laboratory parent per generation, restart/lease/concurrency/exactly-once evidence, sealed-evaluator separation, and SQLite/PostgreSQL parity.

It may not modify `main`, merge, deploy, change the active production Harness, rewrite its evaluator, expand its own permissions, or continue across runs without explicit authority.

## Packet PE7-META-IMPROVER-EXPERIMENT-1 — separate unseen-task experiment

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1

Require pre-registered unseen tasks, immutable evaluator/labels, contamination controls, baselines, statistical/effect/error thresholds, seeds, budgets, stop/rollback rules, and immutable active-Harness identity. A NO-GO result is valid completion.
