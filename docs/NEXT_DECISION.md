# Next Decision

Last updated: 2026-08-03.

## Current Direction

The repository optimizes one outcome:

> Under non-negotiable quality, safety, traceability, compatibility, and rollback constraints, increase verifiable and reusable task delivery per unit of total lifecycle cost.

Quality, safety, integrity, authority, compatibility, evidence completeness, and rollback are hard gates. Accepted delivery, reliability, token use, monetary cost, latency, engineering effort, maintenance surface, recovery burden, and observed reuse are optimization evidence only after those gates pass.

Do not substitute feature count, model/provider count, Dashboard completeness, PR creation, fixture success, a single successful run, or a scalar efficiency index for product capability or learning.

The authoritative order is complete through the outbound local-loop control plane; accepted history and evidence are owned by `docs/CURRENT_STATUS.md`, and completed packet contracts remain in Git history. The remaining forward stages are:

```text
→ freeze one operator-supplied real RWE corpus under the accepted protocol
→ first frozen Real Workload Evidence baseline
→ Architecture Convergence AC1–AC7
→ identical-corpus and identical-protocol replay
→ VDE/Pareto evidence and Level-2 GO/NO-GO
→ bounded Level-2 controller only on GO
→ separately authorized Meta Improver experiment
→ Dashboard #225 last
```

The clean reseal authorization `GOLDEN-PATH-RECOVERY-AND-CLEAN-RESEAL-20260801` (issued `2026-08-01T14:29:00+09:00`) allowed at most three separately consumed attempts; attempt-1 is consumed, terminalized, and independently accepted, and the unused attempt-2/attempt-3 allowances confer no authority for further live-seal execution without a new planning-layer decision. No forward stage starts automatically: RWE requires a frozen operator-supplied real corpus/protocol and a separately persisted one-use RWE spend envelope; later stages require their named prerequisites and GO decisions. Fixture completion is not live acceptance; context-capsule automation is transport, not authority; VDE is a read-only evidence projection, not a new execution or adoption authority.

## Active Routing

1. `PE7-REAL-WORKLOAD-EVIDENCE-1` — `BLOCKED_PREREQUISITE` on a frozen real corpus/protocol and a separately persisted one-use RWE spend envelope; the review-transport repair and the local-loop control-plane prerequisites are now `COMPLETE`.
2. `PE7-ARCHITECTURE-CONVERGENCE-1` — `BLOCKED_PREREQUISITE`.
3. `PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1` — `BLOCKED_PREREQUISITE`.
4. `PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1` — `BLOCKED_PREREQUISITE`.
5. `PE7-META-IMPROVER-EXPERIMENT-1` — `BLOCKED_PREREQUISITE`.
The delegated autonomous Golden Path packet remains complete through merged PR #323 and is no longer routed. The live-seal packet is accepted `COMPLETE`. The outbound local-loop packet is accepted `COMPLETE` through closeout PR #358 and is no longer routed. This closeout does not start RWE: RWE remains gated by the frozen corpus/protocol and one-use spend envelope prerequisites above.

## Packet States

- `READY_FOR_EXECUTION` — prerequisites and authority are sufficient to begin.
- `BLOCKED_PREREQUISITE` — a named earlier evidence or authority condition is incomplete.
- `DECISION_REQUIRED` — safe authority cannot be derived automatically.
- `IN_PROGRESS` — one current branch/PR board owns the work.
- `COMPLETE` — merged, verified, independently reviewed, and documented.

Historical compatibility labels retained for handoff checks only: Packet PR207-REPAIR-1; Packet PE2-RUNTIME-PRODUCER-1; Packet PE4-EVIDENCE-ENTRY-1; Packet TOOL-DISCOVERY-BENCH-1. They are not active routing.

## Open PR Coordination

Merged-PR acceptance facts, open review surfaces, and open-work coordination are owned by `docs/CURRENT_STATUS.md` (`## Verified Repository State`, `## Open Review Surfaces`, `## Open Work Coordination`). This document links rather than duplicates them.

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
observed_reuse_count
expected_reuse_count
cost_or_measurement_unavailable_fields
```

The receipt may begin as a report/document contract. Persisting or automating it requires a later reviewed design and must reuse existing evidence/artifact owners.

Separate realized facts from forecasts:

```text
realized_lifecycle_cost
forecast_lifecycle_cost
observed_reuse_count
expected_reuse_scenario
```

Expected reuse, future maintenance, and amortization are scenario inputs until observed. Failed, cancelled, timed-out, killed, recovered, and outcome-unknown attempts retain their consumed cost; successful-run-only costing is prohibited.

A Level-2 GO decision requires more than runtime token improvement. It must consider comparable layered success, reliability, provider/token/latency/cost evidence, implementation and review cost, migration/rollback risk, maintenance surface, authority growth, failure recovery, observed reuse, uncertainty, and realistic implementation feasibility.

A change that reduces tokens but increases total lifecycle cost, weakens reliability, increases material rework, or broadens authority without accepted benefit is not an efficiency improvement.

## VDE Routing Contract

`docs/ARCHITECTURE_BOOK.md` is the sole full owner of VDE semantics: layered success, typed value bases, realized/forecast separation, LCAP, evidence-sufficiency states, reviewer measurement, artifact-first persistence, Pareto precedence, and non-authority boundaries.

This document owns only execution routing:

- the first live Golden Path sample may prove evidence wiring and realized-cost capture only; it remains `INSUFFICIENT_REPETITIONS`;
- before RWE, freeze the exact real corpus, source/verifier, primary value basis, reviewer policy, repetitions, budget grid, stop rules, non-inferiority margins, cost completeness, seeds, and statistical method;
- replay the identical corpus and protocol after Architecture Convergence;
- require `COMPARISON_ELIGIBLE` evidence and hard-gate non-inferiority before Level-2 GO;
- do not extend Level-1 `MetricVector`, add a VDE table, automate adoption, or create a second evidence authority in this packet.

This provider-free contract does not move the active frontier, authorize a live task, establish an RWE baseline, or create a VDE result.

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
- No Agent may self-approve risk, spend, merge, release, deployment, production adoption, value basis, reviewer acceptance, or economic improvement.
- Finish focused/full checks, exact-head CI, complete-diff review, handoff validation, and rollback review before merge.

## Golden Path Acceptance Gate

A live managed task may start only when all of these are current and exact:

- accepted decision and residual-risk hashes;
- authenticated non-fixture operator principal and required scopes;
- separate one-use spend authorization;
- parent-only credential that never enters the child;
- versioned managed-coding runtime profile and exact observed executable path/version/SHA/capabilities when a binary exists;
- exact provider/protocol/host/base URL/admitted paths/requested model/resolved model;
- exact ProductTask/workflow/node/attempt identity;
- exact target repository and target-main SHA;
- request/retry/token/time/cost contract;
- Draft-PR-only output, no auto-merge, no release/deploy;
- gateway/session usage reconciliation;
- cancellation, cleanup, rollback, approval, output-confirmation, and terminal-evidence owners;
- a fresh context capsule bound to the current accepted-main SHA, active PR exact head, workflow evidence, review observation time, and next permitted action.

Codex remains `mediation_hardened_partial`. Retry identity, product-enforced loopback-only network confinement, and host namespace limitations remain explicit residual risks unless separately proved.

The first bounded live Golden Path task also records one complete realized workflow sample when available: provider/request/token/latency/cost-source evidence, human preparation, review and material rework time, repair iterations, CI effort, recovery, approval/output, cleanup, and terminal evidence. That sample must not be reported as ROI, stable VDE, success probability, or an RWE baseline.

## Hard Stops

Stop before any of the following:

- secret, credential, raw prompt/output/transcript, private path, or repository-content exposure;
- second runtime, scheduler, store, evaluator, budget, approval, output, audit, rollback, VDE authority, or context-authority owner;
- caller-asserted authority, stale or conflicting identity, duplicate effect, late write, missing lease, or outcome-unknown treated as success;
- provider call in CI;
- target-default-branch write, auto-merge, merge, release, deployment, installation, or production adoption;
- unreviewed schema migration or SQLite/PostgreSQL semantic divergence;
- performance, cost, value, reliability, VDE, ROI, or learning claim without comparable frozen evidence;
- implicit aggregation across incompatible value bases;
- treating forecast cost/reuse as realized evidence;
- changing corpus, reviewer policy, budget, verifier, or thresholds after observing comparison results.

## Packet PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1

**State:** `COMPLETE`

**Owned PRs:** clean reseal attempt-1 accepted (terminal receipt `5158092741`); packet repairs through PRs #339/#340, #342, and #346.

**Authority record:** authorization `GOLDEN-PATH-RECOVERY-AND-CLEAN-RESEAL-20260801` allowed at most three separately consumed attempts, one ProductTask per attempt, at most three provider requests per attempt, zero retries, at most one new `acp/*` branch and one Draft PR per attempt, and a combined provider spend cap of `$1.00`, with route `deepseek-v4-pro` planning, `deepseek-v4-flash` implementation, deterministic non-provider verification, and `deepseek-v4-pro` review. Attempt-1 was consumed, terminalized, and independently accepted; attempt-2 and attempt-3 remain unused ceilings and confer no authority without a new planning-layer decision. The outcome is exactly one `INSUFFICIENT_REPETITIONS` sample; details are owned by `CURRENT_STATUS`.

## Packet TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1 — non-authoritative independent-review transport hardening

**State:** `COMPLETE`

**Owned PRs:** #350 merged at `0bd9501235767dc680a40196080c271c5049f91d`.

**Binding:** the transport remains the non-authoritative exact-head review evidence path — receipts bind only their exact head, live in the PR review thread, and grant no merge authority; mechanics are owned by `docs/REAL_WORLD_TESTING_PLAYBOOK.md`.

## Packet TOOL-LOCAL-LOOP-CONTROL-PLANE-1 — outbound local worker and durable engineering loop

**State:** `COMPLETE`

**Owned PRs:** #351 merged at `f37ad7f72c7d49257b8cf28df4ca4388ad2249f4`; repair #353 merged at `4e6ceca804c329c7356dc4254302bf7f83b78cb2`; smoke Issue #355 → `handed_off` → Draft PR #356; acceptance closeout PR #358.

**Binding:** Plan-derived candidate admission remains deferred. A later planning-authorized packet may re-enable Plan admission only after plan-aware CI/review/terminal binding, exact-spec prompt blob binding, and post-claim revalidation are implemented and independently reviewed. The legacy public self-hosted execution path stays retired: `agent-intake` remains `disabled_manually` and Issue #208 keeps `agent-emergency-stop`; no parallel execution path may be re-enabled without planning authority.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-1 — first bounded baseline

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1`, `TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1`, and `TOOL-LOCAL-LOOP-CONTROL-PLANE-1`

PR #300 may prepare provider-free corpus, authorization, runner, and evidence contracts, but live RWE requires the accepted Golden Path terminal evidence, accepted review-transport repair, a frozen operator-supplied real corpus/protocol, and a separately persisted one-use RWE spend envelope.

Before execution, freeze a real, versioned, hash-bound, replayable `rwe_economic_corpus.v1`-class contract. Each task binds exact source repository/commit, task definition/reference, allowed mutable surface, verification, expected class, output bounds, timeout/cancel behavior, executor identity, budget, cleanup, primary value basis/source/confidence, layered acceptance rubric, reviewer policy, minimum repetitions, budget points, stop rules, non-inferiority margins, cost-completeness requirements, seeds, and statistical method.

Fixture authority corpora remain separate and cannot establish task value or economic performance. Different value bases remain separate unless a pre-registered versioned conversion contract exists.

The baseline records layered success, failure class, request/retry/token/latency/cost-source semantics, timeout/cancel/pause/kill/restart/outcome-unknown, SQLite/PostgreSQL parity, approval/output/target-main/Draft-PR/terminal evidence, realized lifecycle cost, review/rework/recovery evidence, evidence-sufficiency state, and the implementation-cost receipt.

The baseline may report raw observations and uncertainty. It must not claim `COMPARISON_ELIGIBLE` until minimum repetitions and cost completeness are satisfied.

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

Each packet changes one coherent ownership boundary, preserves compatibility and rollback, and records implementation cost. It must not create a second scheduler, store, budget, approval, output, evidence, VDE, or rollback owner.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1 — post-convergence comparison

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-ARCHITECTURE-CONVERGENCE-1

Replay the identical frozen corpus, source identities, verifier, reviewer policy, value basis, budget grid, seed set, stop rules, and statistical method. Compare layered success/failure classifications, reliability, request/retry/token/latency/cost evidence, restart/recovery, approval/output/terminal behavior, realized lifecycle cost, review/rework burden, implementation cost, maintenance surface, rollback burden, LCAP, human-relative saving when comparable, and the lifecycle-cost Pareto frontier. Do not tune the corpus, thresholds, or reviewer policy from convergence results.

## Packet PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1 — bounded multi-generation decision

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1

First record an evidence-backed GO/NO-GO. GO requires all hard gates, pre-registered quality and reliability non-inferiority, comparable value semantics, `COMPARISON_ELIGIBLE` evidence, uncertainty-aware VDE/Pareto improvement, and no unacceptable review/rework/recovery/maintenance/authority/rollback regression. A scalar index cannot independently satisfy GO.

On GO only, implement a default-off bounded laboratory controller with small fixed generation/candidate/evaluation limits, deterministic global budgets, one selected laboratory parent per generation, restart/lease/concurrency/exactly-once evidence, sealed-evaluator separation, and SQLite/PostgreSQL parity.

It may not modify `main`, merge, deploy, change the active production Harness, rewrite its evaluator, expand its own permissions, or continue across runs without explicit authority.

## Packet PE7-META-IMPROVER-EXPERIMENT-1 — separate unseen-task experiment

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1

Require pre-registered unseen tasks, immutable evaluator/labels, contamination controls, baselines, statistical/effect/error thresholds, seeds, budgets, stop/rollback rules, and immutable active-Harness identity. A NO-GO result is valid completion.
