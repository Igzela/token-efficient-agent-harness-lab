# Next Decision

Last updated: 2026-08-06.

This document is the single owner of forward routing, packet prerequisites, entry/exit gates, and the immediate next permitted action. Accepted historical facts belong in `docs/CURRENT_STATUS.md`; durable architecture and authority invariants belong in `docs/ARCHITECTURE_BOOK.md`.

## Current Direction

The repository optimizes one outcome:

> Under non-negotiable quality, safety, traceability, compatibility, recovery, and rollback constraints, increase verifiable and reusable task delivery per unit of total lifecycle cost.

Quality, authority, evidence integrity, compatibility, recovery, and rollback are hard gates. Token use, monetary cost, latency, accepted delivery, engineering effort, maintenance surface, and reuse are optimization evidence only after those gates pass.

Do not substitute feature count, Agent count, candidate count, PR creation, fixture success, novelty prompts, reflection, debate, or one scalar score for product capability, scientific exploration, learning, or recursive self-improvement.

Charlie accepted the following direction on 2026-08-06:

- retain the existing evidence-first sequence;
- classify the planned Level-1/Level-2 work as **bounded recursive Harness optimization**, not general recursive self-improvement;
- add experiment-control, exploration-diversity, memory/skill projection, strengthened single-generation calibration, and sealed transfer gates before any Meta Improver claim;
- keep candidate generation, experimental-parent selection, and production adoption as separate authorities;
- accept NO-GO, saturation, diversity collapse, transfer failure, or inability to beat the frozen baseline as valid experimental completion.

This decision changes future routing and acceptance gates. It does not activate a provider call, live experiment, Level-2 controller, Meta Improver, production adoption, merge, release, or deployment.

## Authoritative Forward Order

```text
Minimum First RWE Board B: production authorization/spend wiring
→ first frozen live RWE baseline
→ Architecture Convergence AC1–AC7
→ identical-corpus and identical-protocol replay
→ Harness-Evolution experiment-control hardening
→ memory/skill projection experiment
→ strengthened one-generation Level-1 calibration
→ VDE/Pareto and Level-2 GO/NO-GO
→ bounded Level-2 controller only on GO
→ sealed transfer evaluation
→ separately authorized Meta Improver experiment
→ human adoption decision, if any
→ Dashboard #225 last
```

No downstream packet starts automatically. Every packet must satisfy its named prerequisites on accepted `main`.

## Active Routing

1. `PE7-REAL-WORKLOAD-EVIDENCE-1` — `READY_FOR_EXECUTION` at Board B.
2. `PE7-ARCHITECTURE-CONVERGENCE-1` — `BLOCKED_PREREQUISITE`.
3. `PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1` — `BLOCKED_PREREQUISITE`.
4. `PE7-HARNESS-EVOLUTION-EXPERIMENT-CONTROL-HARDENING-1` — `BLOCKED_PREREQUISITE`.
5. `PE7-MEMORY-SKILL-PROJECTION-EXPERIMENT-1` — `BLOCKED_PREREQUISITE`.
6. `PE7-HARNESS-EVOLUTION-LEVEL1-CALIBRATION-1` — `BLOCKED_PREREQUISITE`.
7. `PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1` — `BLOCKED_PREREQUISITE`.
8. `PE7-HARNESS-EVOLUTION-TRANSFER-EVALUATION-1` — `BLOCKED_PREREQUISITE`.
9. `PE7-META-IMPROVER-EXPERIMENT-1` — `BLOCKED_PREREQUISITE`.
10. Dashboard PR #225 — `DEFERRED_LAST`.

## Packet States

- `READY_FOR_EXECUTION` — prerequisites and repository authority are sufficient to begin provider-free implementation.
- `BLOCKED_PREREQUISITE` — a named earlier evidence, implementation, or authority condition is incomplete.
- `DECISION_REQUIRED` — safe direction or authority cannot be derived from accepted owners.
- `IN_PROGRESS` — one current branch/PR owns the packet.
- `COMPLETE` — merged, verified, independently reviewed, and documented.

## Common Evidence and Cost Contract

Every engineering or experimental board returns a bounded `implementation_cost_receipt` with available realized evidence:

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
human_preparation_minutes
review_minutes
material_rework_minutes
recovery_minutes
observed_reuse_count
cost_or_measurement_unavailable_fields
```

Keep realized facts separate from forecasts:

```text
realized_lifecycle_cost
forecast_lifecycle_cost
observed_reuse_count
expected_reuse_scenario
```

Failed, rejected, cancelled, timed-out, killed, recovered, and outcome-unknown attempts retain their consumed cost. Successful-run-only costing is prohibited.

## Comparison and Claim Discipline

The following claims require different evidence and must not be collapsed:

```text
candidate passed one task
!= Harness improved on a frozen comparison
!= improvement transfers to unseen tasks
!= improvement operator became better
!= open-ended evolution
!= general recursive self-improvement
```

A Harness improvement claim requires frozen tasks, evaluator, reviewer policy, budgets, seeds, stop rules, and comparable lifecycle cost.

An improvement-operator claim requires a frozen baseline operator `O0` and candidate operator `O1`, equal total lifecycle budget, unseen task families, immutable evaluator/labels, repeated evidence, and a demonstrated improvement in the distribution or cost of valid Harness improvements.

No current or planned packet alone establishes open-ended evolution or general recursive self-improvement.

## Common Execution Protocol

- Refresh remote `main`, open PR exact heads, CI, reviews, current documents, and overlapping ownership before work.
- Generate a fresh context capsule and treat it as stale when `main`, a PR head, CI, review, or a canonical document changes.
- Reuse the existing scheduler, executor, ProductTask, worktree, verification, artifact, approval, output, replay, scorecard, audit, cleanup, terminal-evidence, and `LocalProductStore` owners.
- Bind authority from persisted current owners, never caller assertions, model text, branch-local summaries, or memory projections.
- Preserve SQLite/PostgreSQL parity, atomicity, restart, concurrency, idempotency, cancellation, lease ownership, late-write refusal, compensation, and rollback.
- Keep provider execution off in CI, target `main` unchanged, Draft-PR-only output, and auto-merge disabled.
- Do not combine unrelated authority surfaces into one unreviewable board.
- Finish focused checks, canonical exact-head CI, complete-diff independent review, handoff validation, and rollback review before merge.

## Hard Stops

Stop before any of the following:

- secret, credential, raw prompt/output/transcript, private path, or unredacted repository-content exposure;
- a second runtime, scheduler, store, evaluator, budget, approval, output, audit, rollback, VDE, memory-authority, or context-authority owner;
- caller-asserted authority, stale identity, duplicate effect, missing lease, late write, or outcome-unknown treated as success;
- provider call in CI;
- target-default-branch write, auto-merge, merge, release, deployment, installation, or automatic production adoption;
- candidate modification of evaluator rules, scanner scope, ignore/baseline, sealed holdout, budget accounting, statistical method, reviewer rubric, or immutable safety policy;
- reporting the best candidate while hiding rejected candidates, diversity collapse, contamination, evaluator gaming, or full consumed cost;
- treating memory, skills, summaries, novelty scores, forecasts, or scalar VDE indices as authority;
- changing corpus, reviewer policy, budget, verifier, seeds, stop rules, non-inferiority margins, or statistical method after observing comparison results;
- claiming learning, open-ended evolution, or recursive self-improvement without the separately required evidence.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-1 — Minimum First RWE

**State:** `READY_FOR_EXECUTION`

### Board A — Complete

PR #361 froze and accepted:

- two real tasks from one exact `Igzela/alters-lab` target-main identity;
- one `rwe_economic_protocol.v1` instance;
- one deterministic `execution_schedule.v1`;
- one strict `rwe_run_authorization.v2` contract and validator;
- provider-free mutation tests, hash locks, v1/v2 separation, and fail-closed bindings.

The frozen artifacts do not authorize a live run.

### Board B — Next Permitted Work

Implement the smallest provider-free production wiring that:

1. issues and validates `rwe_run_authorization.v2` through the existing authenticated RWE/store owner;
2. derives accepted-main, corpus, protocol, schedule, target, principal, provider, executor, expiry, and budget bindings from current owners rather than checkout text or caller assertions;
3. persists one separately authorized, one-use RWE spend envelope under the existing spend/budget authority model;
4. atomically admits or rejects before any run, task-attempt, provider, workspace, or target effect;
5. prevents rejected, stale, duplicate, expired, revoked, conflicting, or not-ready requests from consuming authority;
6. preserves restart, idempotency, concurrency, late-write refusal, cancellation, cleanup, terminal evidence, and SQLite/PostgreSQL parity;
7. keeps provider calls and target effects absent from CI;
8. adds no second scheduler, store, budget, evaluator, approval, output, audit, or rollback owner.

Board B completion is provider-free. A live baseline still requires a new separately authorized run against the accepted exact head.

### First Live Baseline Exit Gate

The first baseline must execute the frozen schedule without changing the corpus, protocol, reviewer policy, budgets, seeds, verifier, or thresholds. It records layered success, failure class, provider/request/token/latency/cost evidence, approval/output/terminal bindings, recovery and cleanup, full failed-attempt cost, and evidence sufficiency.

No claim stronger than the observed evidence-sufficiency state is allowed.

## Packet PE7-ARCHITECTURE-CONVERGENCE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-REAL-WORKLOAD-EVIDENCE-1

An accepted pre-convergence frozen RWE baseline is also required before implementation begins.

Implement incrementally:

1. AC1 unified process supervision.
2. AC2 typed execution boundary.
3. AC3 Golden Path responsibility split.
4. AC4 transaction-scoped domain views.
5. AC5 explicit runtime composition root.
6. AC6 Rust-authoritative API/SDK/Dashboard schema convergence.
7. AC7 obsolete-abstraction cleanup after all callers and evidence migrate.

Each board changes one coherent ownership boundary, preserves compatibility and rollback, and records implementation cost.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-ARCHITECTURE-CONVERGENCE-1

Replay the identical frozen corpus, source identities, verifier, reviewer policy, value basis, budget grid, seeds, stop rules, and statistical method.

Compare layered success and failure classifications, reliability, token/request/latency/cost evidence, restart/recovery, approval/output/terminal behavior, review/rework burden, implementation cost, maintenance surface, rollback burden, LCAP, and the lifecycle-cost Pareto frontier.

Do not tune the comparison from post-convergence results.

## Packet PE7-HARNESS-EVOLUTION-EXPERIMENT-CONTROL-HARDENING-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1

An evidence-backed planning decision to prepare future Harness experiments is also required. This packet establishes experiment contracts only. It must not start Level-2 or modify the active Harness.

Required contracts:

- immutable active-Harness identity;
- versioned candidate lineage and parent identity;
- pre-registered mutation-operator families such as context management, memory policy, tool routing, Agent information flow, verification strategy, task decomposition, and control flow;
- candidate generation budget and generator identity;
- equal total lifecycle budget rather than token-only equality;
- immutable primary evaluator, regression checks, safety/policy sentinel, contamination sentinel, evaluator-gaming sentinel, diversity sentinel, cost projection, and independent review;
- hard-gate-first eligibility followed by Pareto comparison;
- no candidate access to sealed holdout labels or final judge answers.

The evaluator constellation reuses current verification, replay, scorecard, evidence, audit, and review owners. It does not create a second evaluator authority.

Diversity evidence should include, where measurable:

```text
pairwise candidate diversity
parent distance
seed-strategy distance
mutation-family coverage
near-duplicate rate
problem-reframing rate
method-recombination rate
independent novelty-judge disagreement
```

These fields are evidence and sentinels, not production authority.

## Packet PE7-MEMORY-SKILL-PROJECTION-EXPERIMENT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HARNESS-EVOLUTION-EXPERIMENT-CONTROL-HARDENING-1

Test memory and skill policies separately from Architecture Convergence and from the active production Harness.

The immutable raw event, transcript, artifact, commit, CI, review, and decision evidence remains authoritative. Memory and skills are derived projections that must be deletable, rebuildable, invalidatable, and non-authoritative.

Each projection binds, when applicable:

```text
source references
created_at
builder identity and version
derivation reason
confidence
scope
supersedes
expiry or invalidation condition
```

Exact numbers, dates, current commit identities, CI, authority, and accepted decisions must be rechecked against original owners when they matter.

The memory builder and evaluator must not access sealed final answers or hidden labels. No Agent may directly rewrite raw evidence or turn a memory/skill file into permission, spend, evaluator, adoption, or routing authority.

A negative or no-benefit result is valid completion.

## Packet PE7-HARNESS-EVOLUTION-LEVEL1-CALIBRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-MEMORY-SKILL-PROJECTION-EXPERIMENT-1

Experiment-control hardening must also remain accepted and unchanged.

Run one strengthened generation only:

```text
frozen active Harness
→ candidates from multiple pre-registered mutation families
→ diversity admission and duplicate rejection
→ equal total lifecycle budget
→ hard-gate evaluator constellation
→ Pareto selection
→ sealed unseen-task holdout
→ PR_READY only
```

This board validates lineage, evaluator immutability, diversity sentinels, contamination controls, gaming resistance, cost completeness, and attribution. It cannot activate Level-2 or replace the active Harness.

## Packet PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HARNESS-EVOLUTION-LEVEL1-CALIBRATION-1

An explicit evidence-backed GO is also required.

GO requires:

- every correctness, safety, authority, compatibility, evidence, recovery, and rollback hard gate passes;
- pre-registered quality and reliability non-inferiority;
- comparable value semantics and `COMPARISON_ELIGIBLE` evidence;
- uncertainty-aware lifecycle-cost Pareto improvement;
- no unacceptable review, rework, recovery, maintenance, authority, or rollback regression;
- no material diversity collapse, contamination, or evaluator gaming;
- credible implementation feasibility under a deterministic total budget.

On GO only, implement a default-off bounded laboratory controller with small fixed generation, candidate, and evaluation limits; one selected laboratory parent per generation; deterministic global budgets; exact candidate lineage; restart/lease/concurrency/exactly-once evidence; sealed-evaluator separation; and SQLite/PostgreSQL parity.

Mandatory stop rules include:

- authority, safety, contamination, or evaluator-integrity failure;
- total budget exhaustion;
- repeated sealed-holdout regression;
- diversity below the pre-registered threshold;
- repeated exploitation of the same evaluator weakness;
- no reproducible Pareto improvement for the pre-registered saturation window;
- improvement-per-total-lifecycle-cost no longer improves;
- maintenance, review, rework, or recovery burden exceeds its limit.

Level-2 may not modify `main`, merge, deploy, rewrite its evaluator, expand its permissions, adopt a production Harness, or continue across runs without explicit authority.

## Packet PE7-HARNESS-EVOLUTION-TRANSFER-EVALUATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1

Evaluate the selected experimental Harness on pre-registered sealed unseen tasks and, where practical, unseen task families, models, or execution environments.

Freeze evaluator, labels, budgets, seeds, stop rules, baselines, and contamination controls before execution. Compare transfer quality, reliability, lifecycle cost, diversity, review/rework burden, and failure classifications.

A development-set improvement with transfer regression is not an accepted Harness improvement.

## Packet PE7-META-IMPROVER-EXPERIMENT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HARNESS-EVOLUTION-TRANSFER-EVALUATION-1

Separate experiment authority is also required.

The experiment compares a frozen baseline improvement operator `O0` with a candidate operator `O1`. Both receive equal total lifecycle budget and operate on unseen task families under immutable evaluator/labels, contamination controls, baselines, statistical/effect/error thresholds, seeds, stop rules, rollback, and immutable active-Harness identity.

An operator improvement requires repeatable evidence that `O1`, relative to `O0`, does at least one of the following without unacceptable regression:

- produces a better distribution of hard-gate-eligible Harness candidates;
- reaches comparable accepted delivery at lower total lifecycle cost;
- finds more valid Pareto improvements;
- transfers more reliably across unseen task families or models.

One improved descendant Harness is insufficient. The improvement mechanism itself must be shown to improve.

The strongest permitted claim after success is bounded second-order improvement in the tested domain. It is not evidence of open-ended evolution or general recursive self-improvement.

A NO-GO result is valid completion.

## Adoption Boundary

Candidate generation authority, experimental-parent selection, and production adoption are separate:

```text
candidate generation
!= experimental parent selection
!= active-Harness adoption
```

Even after every experiment passes, adoption requires a separate human decision, exact candidate artifact, complete diff, independent review, canonical CI, rollback plan, and no unresolved objection. No evolution packet grants automatic adoption.

## Dashboard Boundary

Dashboard PR #225 remains last and presentation-only. It may project accepted schemas and evidence but cannot become a workflow, evaluator, spend, approval, adoption, output, merge, release, or deployment owner.
