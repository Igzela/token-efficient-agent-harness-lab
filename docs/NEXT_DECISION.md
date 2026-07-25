# Next Decision

Last updated: 2026-07-25.

## First-Order Objective

The repository optimizes one outcome:

> Under non-negotiable quality, safety, traceability, compatibility, and rollback constraints, increase verifiable and reusable task delivery per unit of total lifecycle cost.

Quality, safety, integrity, authority, and rollback are hard gates. Token use, monetary cost, latency, engineering effort, maintenance surface, and expected reuse are optimization evidence only after those gates pass.

Do not substitute feature count, model/provider count, Dashboard completeness, PR creation, or fixture success for product capability or learning.

## Authoritative Roadmap

Execute in this order:

1. **Close the provider-free Codex/Golden Path review board.**
   - Repair and independently accept cumulative PR #299.
   - Repair and independently accept provider-free RWE preparation PR #300.
   - Repair and independently accept observation-only CC Switch adaptation PR #301.
   - #297/#298 are superseded by #299 and must not merge separately.
2. **Run one live managed Golden Path acceptance task.**
   - Requires accepted partial-mediation authority, authenticated non-fixture principal, parent-only credential, explicit one-use spend authorization, exact target SHA, Draft-PR-only output, and terminal evidence.
3. **Freeze and run the first bounded RWE baseline.**
   - Requires a separate RWE spend envelope; Golden Path authorization is insufficient.
4. **Run Architecture Convergence AC1–AC7.**
   - AC1 unified process supervision.
   - AC2 typed execution boundary.
   - AC3 Golden Path responsibility split.
   - AC4 transaction-scoped domain views.
   - AC5 runtime composition.
   - AC6 API/SDK/Dashboard schema convergence.
   - AC7 obsolete-abstraction cleanup.
5. **Replay the identical frozen RWE corpus.**
6. **Make an evidence-backed Level-2 GO/NO-GO decision.**
7. **Only on GO, implement the bounded Level-2 generational controller.**
8. **Only after accepted Level-2 evidence, consider the separately authorized Meta Improver experiment.**
9. **Handle Dashboard PR #225 last.**

Do not skip RWE and begin Architecture Convergence or Level-2 early. Do not treat provider-free fixture completion as live acceptance.

## Current Routing

| Work | State | Exit evidence |
|---|---|---|
| PR #299 authority board | `IN_PROGRESS` | Final unchanged head, complete CI, independent complete-diff review, no unresolved authority objection |
| PR #300 provider-free RWE preparation | `BLOCKED_ON_#299` | Accepted #299 base, genuine frozen corpus, store-owned one-use RWE authorization, provider-free runner/evidence owner, complete CI |
| PR #301 observation adaptation | `IN_PROGRESS` | Correct canonical token buckets and provider/request identities, no second authority, MIT attribution, complete CI |
| Live Golden Path | `BLOCKED_PREREQUISITE` | One bounded live task reaches verified Draft PR and exact terminal evidence under accepted authority |
| First RWE baseline | `BLOCKED_PREREQUISITE` | Frozen corpus executed under separate authorization; baseline sealed and independently accepted |
| Architecture Convergence | `BLOCKED_PREREQUISITE` | First RWE baseline accepted |
| Same-corpus replay | `BLOCKED_PREREQUISITE` | AC1–AC7 complete |
| Level-2 decision | `BLOCKED_PREREQUISITE` | Comparable pre/post evidence plus lifecycle-cost evidence |
| Level-2 implementation | `BLOCKED_PREREQUISITE` | Explicit GO |
| Meta Improver | `BLOCKED_PREREQUISITE` | Accepted Level-2 and pre-registered unseen-task experiment |

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

Level-2 GO requires more than runtime token improvement. The decision must consider:

- comparable quality and safety;
- provider/token/latency/cost evidence;
- implementation and review cost;
- migration and rollback risk;
- maintenance surface and authority growth;
- failure recovery burden;
- expected reuse and realistic probability of successful adoption.

A change that reduces tokens but increases total lifecycle cost or weakens reliability is not an efficiency improvement.

## Common Execution Protocol

- Refresh actual `main`, open PR heads, CI, reviews, active documents, and overlapping ownership before work.
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
- cancellation, cleanup, rollback, approval, output-confirmation, and terminal-evidence owners.

Codex remains `mediation_hardened_partial`. Retry identity, loopback-only network confinement, and host namespace limitations remain explicit residual risks unless separately proved.

## RWE Contract

The first RWE corpus must be real, versioned, hash-bound, replayable, and frozen before Architecture Convergence. Each task binds exact source repository/commit or fixture tree, task definition/reference, allowed mutable surface, verification, expected class, output bounds, timeout/cancel behavior, executor identity, and budget.

The baseline records at least:

- quality/pass/failure classification;
- request, retry, token, latency, and cost-source semantics;
- timeout, cancellation, pause/kill, restart, outcome-unknown, and cleanup;
- SQLite/PostgreSQL parity;
- approval, output, Draft PR, target-main, and terminal evidence;
- implementation-cost receipt for the board that produced the baseline.

The post-convergence run must use the identical corpus. Do not tune the corpus using convergence results.

## Level-2 and Meta Boundaries

Level-2 remains a bounded laboratory controller, not production recursive self-update. Initial hard limits remain small and explicit: bounded generations, candidates, total evaluations, global budgets, concurrency, time, workspaces, and artifacts. It may select a laboratory parent but may not modify `main`, merge PRs, deploy, change the active production Harness, rewrite its evaluator, or expand its own authority.

Meta Improver is a separate research decision. It requires unseen tasks, immutable evaluation labels, contamination controls, baselines, statistical thresholds, seeds, budgets, stop/rollback rules, and an immutable active Harness. A NO-GO result is valid completion.

## Hard Stops

Stop before any of the following:

- secret, credential, raw prompt/output/transcript, private path, or repository-content exposure;
- second runtime, scheduler, store, evaluator, budget, approval, output, audit, or rollback owner;
- caller-asserted authority, stale or conflicting identity, duplicate effect, late write, missing lease, or outcome-unknown treated as success;
- provider call in CI;
- target-default-branch write, auto-merge, merge, release, deployment, installation, or production adoption;
- unreviewed schema migration or SQLite/PostgreSQL semantic divergence;
- performance, cost, or learning claim without comparable evidence.

## Immediate Next Decision

The immediate engineering task is not a new provider or learning subsystem. It is to finish the final heads of #299/#300/#301 without overwriting this main documentation convergence, obtain independent review and complete CI, then merge only the accepted cumulative surfaces in dependency order. After that, the next manual gate is one bounded live Golden Path task; RWE and Architecture Convergence remain blocked until their stated evidence exists.
