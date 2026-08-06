# Current Status

Last updated: 2026-08-06.

This document separates three states that must not be conflated:

1. **Merged and accepted truth** — code and governing documents on `main` that passed their required checks.
2. **Open review surfaces** — proposed work that is not authoritative until its final exact head is accepted and merged.
3. **Blocked or deferred work** — future work that remains ineligible because a named evidence or authority gate is incomplete.

Historical packet detail remains in Git history and merged PRs; do not append stale chronology here.

## Verified Repository State

- Repository: `Igzela/token-efficient-agent-harness-lab`.
- Accepted runtime/code baseline: PR #361 at `9aea39b75a7999ae7db22176b77e06bcf7a6890f`; later strictly documentation-only governance commits do not change runtime behavior.
- Resolve the current remote `main` identity from Git/GitHub or a fresh context capsule rather than hard-coding the moving documentation head here.
- PR #361 merged from exact head `aeff4544e2fca6621e716f4514f9246e96826d94` after an exact-head complete-diff review receipt with outcome `APPROVED` and successful canonical exact-head workflows.
- Post-merge runtime-baseline workflow run `31067282847` completed successfully, including all source jobs and the terminal `context-capsule` job.
- A new head, CI result, review, or canonical-document change invalidates any older context capsule or status summary.

## Accepted Product and Control-Plane State

Accepted `main` contains:

- the Rust-owned workflow runtime, scheduler, ProductTask and `LocalProductStore` authority boundaries;
- SQLite default storage with PostgreSQL parity and restart/recovery evidence;
- managed-coding runtime profiles and provider-free DeepSeek protocol/runtime wiring;
- delegated Golden Path authority with separate risk, spend, attempt, artifact approval, output confirmation, and terminal-evidence owners;
- one independently accepted bounded live Golden Path observation, still classified `INSUFFICIENT_REPETITIONS` rather than RWE or economic improvement;
- exact-head CI, review transport, context-capsule transport, and the outbound local engineering loop;
- provider-free RWE/VDE contracts and artifact validation;
- Harness Evolution Level-1 as a default-off one-generation fixture laboratory with immutable active-Harness identity and no production-adoption authority.

No runtime path may merge, release, deploy, write target default branches, or adopt a candidate as the production Harness.

## Minimum First RWE State

PR #360 is merged and accepted as `3c6cd00f68f4db2a9eef99598deebc42f95ab62b`. It makes live-eligible RWE admission fail closed before authorization consumption when the current gate is not ready. Repeated rejected calls create no run or task-attempt row and do not consume the one-use authority.

PR #361 is merged and accepted as `9aea39b75a7999ae7db22176b77e06bcf7a6890f`. It completed Board A of `PE7-REAL-WORKLOAD-EVIDENCE-1`:

- froze two real `Igzela/alters-lab` tasks under one exact target-main identity;
- froze one `rwe_economic_protocol.v1` instance with two repetitions, one budget point, pre-registered seeds, reviewer policy, non-inferiority margins, cost-completeness fields, and stop rules;
- froze one deterministic four-cell `execution_schedule.v1` with exact task/repetition/seed/budget bindings and a run-level local-estimate ceiling;
- added strict `rwe_run_authorization.v2` bindings for accepted main, corpus/protocol/schedule hashes, target, task budgets, principal, finite expiry, provider route, and in-process executor identity;
- preserved strict v1 fixture versus v2 production-contract separation;
- added no provider call, target effect, schema migration, or new authority owner.

The frozen corpus, protocol, and schedule are accepted prerequisites. They do **not** authorize live execution by themselves.

## Active Frontier

`PE7-REAL-WORKLOAD-EVIDENCE-1` is `READY_FOR_EXECUTION` at Board B.

The next eligible work is provider-free production wiring for `rwe_run_authorization.v2` and the separately persisted one-use RWE spend envelope. Board B must reuse the existing RWE, ProductTask, managed-provider, scheduler, store, audit, approval, output, cleanup, and terminal-evidence owners.

No Board B implementation PR was open at the time of this update.

The first live RWE run remains blocked until all Board B authority, persistence, issue/admit, exact accepted-main, budget, restart, parity, and fail-closed gates are merged and a separate current live-run authorization is issued.

## Open Review Surfaces

| PR | Purpose | Status |
|---|---|---|
| #225 | Presentation-only Dashboard theme | OPEN; independent and last; cannot substitute for runtime or evidence work |
| `Igzela/alters-lab#5` | Draft-PR output from the accepted bounded live Golden Path observation | OPEN, Draft, unmerged; no merge authority |

Merged or closed PRs are not open review surfaces and should not be retained in this table.

## Capability Status

| Stage | State | Entry requirement |
|---|---|---|
| Minimum First RWE Board A: frozen corpus/protocol/schedule and authorization v2 contract | `COMPLETE` | PR #361 accepted |
| Minimum First RWE Board B: production issue/admit/spend wiring | `READY_FOR_EXECUTION` | Start from accepted `main`; provider-free implementation and tests first |
| First live frozen RWE baseline | `BLOCKED_PREREQUISITE` | Board B accepted plus separate one-use live-run authority |
| Architecture Convergence AC1–AC7 | `BLOCKED_PREREQUISITE` | Accepted pre-convergence RWE baseline |
| Identical-corpus/protocol replay | `BLOCKED_PREREQUISITE` | Architecture Convergence complete |
| Harness-Evolution experiment-control hardening | `BLOCKED_PREREQUISITE` | Comparable replay evidence and accepted design packet |
| Memory/skill projection experiment | `BLOCKED_PREREQUISITE` | Immutable raw-evidence boundary and separate accepted experiment contract |
| Strengthened Level-1 calibration | `BLOCKED_PREREQUISITE` | Experiment-control and memory/skill boundaries accepted |
| Level-2 GO/NO-GO | `BLOCKED_PREREQUISITE` | Comparable quality, reliability, lifecycle-cost, diversity, contamination, and Pareto evidence |
| Bounded Level-2 generational controller | `BLOCKED_PREREQUISITE` | Explicit evidence-backed GO |
| Sealed transfer evaluation | `BLOCKED_PREREQUISITE` | Accepted bounded Level-2 result |
| Meta Improver experiment | `BLOCKED_PREREQUISITE` | Accepted transfer result and separate second-order experiment authority |
| Dashboard #225 | `DEFERRED` | Handle last |

## Recursive-Improvement Classification

The accepted direction is **bounded recursive Harness optimization**, not open-ended evolution or general recursive self-improvement.

The repository currently demonstrates neither:

- automatic multi-generation Harness evolution;
- improvement of the improvement operator itself;
- stable cross-task or cross-model transfer;
- expanding problem-space exploration;
- continuous learning;
- production self-update.

Future experiments must distinguish:

```text
Harness improvement
!= improvement-operator improvement
!= production adoption
!= general recursive self-improvement
```

A NO-GO, saturation result, diversity collapse, transfer failure, or inability to beat the frozen baseline under equal total lifecycle budget is valid completion and must be preserved as evidence.

## Confirmed Integration Gaps

1. Board B production wiring and one-use RWE spend persistence are not yet accepted.
2. No repeated live frozen RWE baseline exists.
3. Architecture Convergence has not started because its baseline prerequisite is absent.
4. No diversity, contamination, evaluator-gaming, or mutation-family control packet has been implemented for generational Harness experiments.
5. No accepted memory/skill projection experiment exists; raw evidence remains authoritative and summaries remain non-authoritative projections.
6. No Level-2, sealed transfer, or Meta Improver result exists.

## Open Work Coordination

The next permitted action is to implement Board B of `PE7-REAL-WORKLOAD-EVIDENCE-1` as one focused provider-free branch/PR:

- wire authorization v2 through the existing production issue/admit path;
- persist and atomically consume the separate one-use RWE spend authority;
- derive accepted-main and all authority bindings from current owners, never caller assertions;
- preserve SQLite/PostgreSQL parity, idempotency, restart, concurrency, cancellation, late-write refusal, cleanup, and rollback;
- make every missing, stale, conflicting, expired, duplicate, outcome-unknown, or over-budget state fail closed before an external effect;
- add no provider call in CI and no target effect;
- finish focused checks, canonical exact-head CI, complete-diff independent review, and documentation closeout before any live RWE authorization.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, memory projection, novelty score, or scalar VDE index may become production-adoption authority.
