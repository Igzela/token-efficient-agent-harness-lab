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

Board B of `PE7-REAL-WORKLOAD-EVIDENCE-1` is accepted as provider-free production issue/admit/one-use spend wiring for `rwe_run_authorization.v2` under the existing authenticated RWE/`LocalProductStore` owner. Bindings derive from freeze owners and the authenticated principal; rejected, stale, expired, revoked, wrong-tenant, and conflicting requests fail closed without consuming authority. SQLite/PostgreSQL issue and admit audit parity is preserved. No second spend, store, scheduler, evaluator, approval, output, audit, or rollback owner was added. Board B authorizes no Provider call and no target effect by itself.

The frozen corpus, protocol, schedule, and Board B production wiring are accepted prerequisites. They do **not** authorize a live RWE baseline by themselves.

## Active Frontier

`PE7-REAL-WORKLOAD-EVIDENCE-1` is `READY_FOR_EXECUTION` at the first live frozen RWE baseline.

The next permitted work is a separately authorized one-use live run of the frozen Board A schedule against the accepted exact head, after current live-run authority is issued. The 4-cell live coordinator, provider-backed execution, target writes, and evidence closeout remain outside Board B and require that separate authority.

The first live RWE run remains blocked until a separate current live-run authorization is issued against the accepted exact head. Branch-local Draft heads never authorize live baseline, provider calls, or target effects.

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
| Minimum First RWE Board B: production issue/admit/spend wiring | `COMPLETE` | Provider-free production `rwe_run_authorization.v2` issue/admit and one-use spend wiring accepted |
| First live frozen RWE baseline | `BLOCKED_PREREQUISITE` | Separate one-use live-run authority against accepted exact head |
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

1. No repeated live frozen RWE baseline exists; a separate one-use live-run authorization against the accepted exact head is still required.
2. Architecture Convergence has not started because its baseline prerequisite is absent.
3. No diversity, contamination, evaluator-gaming, or mutation-family control packet has been implemented for generational Harness experiments.
4. No accepted memory/skill projection experiment exists; raw evidence remains authoritative and summaries remain non-authoritative projections.
5. No Level-2, sealed transfer, or Meta Improver result exists.

## Open Work Coordination

The next permitted action is the first live frozen RWE baseline of `PE7-REAL-WORKLOAD-EVIDENCE-1`, only after a separate current one-use live-run authorization is issued against the accepted exact head:

- execute the frozen Board A schedule without changing corpus, protocol, reviewer policy, budgets, seeds, verifier, or thresholds;
- reuse the accepted Board B production issue/admit/spend path and existing ProductTask, managed-provider, scheduler, store, audit, approval, output, cleanup, and terminal-evidence owners;
- record layered success, failure class, provider/request/token/latency/cost evidence, approval/output/terminal bindings, recovery and cleanup, full failed-attempt cost, and evidence sufficiency;
- make every missing, stale, conflicting, expired, duplicate, outcome-unknown, or over-budget state fail closed before an external effect;
- claim no result stronger than the observed evidence-sufficiency state.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, memory projection, novelty score, or scalar VDE index may become production-adoption authority.
