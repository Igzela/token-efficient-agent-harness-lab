# Current Status

Last updated: 2026-08-09.

This document owns accepted repository truth and confirmed capability gaps only. It separates two states that must not be conflated:

1. **Merged and accepted truth** — code and governing documents on remote `main` that passed their required checks.
2. **Confirmed gaps** — capabilities or evidence not yet accepted on `main`.

Open PR heads, Draft/Ready state, CI, reviews, mergeability, and the next permitted action are live observations and must come from a fresh context capsule. Current execution routing belongs in `docs/NEXT_DECISION.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`. Historical packet detail remains in Git history and merged PRs; do not append stale chronology here.

## Verified Repository State

- Repository: `Igzela/token-efficient-agent-harness-lab`.
- Active runtime/code baseline: PR #369 merge `ee43eac853644266614da09de764a3bf19f2d281`; later documentation-only commits do not change that code identity. Always refresh remote `main` for the current canonical-document head.
- PR #368 accepted the provider timeout-ownership repair from exact head `17cc5d03…`; its exact-head review receipt reports `PASS` with no open findings, and its canonical exact-head workflow completed successfully before merge.
- PR #369 accepted the operator-gated compatibility-calibration mechanism from exact head `b571c95a75a7c8eacda99a8f586d8f2360868ab7`; canonical exact-head workflow `31172449577` completed successfully and the exact-head review receipt reports `PASS` with no open findings.
- #369's merge commit is the active runtime/code identity above. The merged mechanism does not prove that an armed calibration was run.
- A new `main`, PR head, CI result, review receipt, or canonical-document change invalidates older context capsules and branch-local status prose.

## Accepted Product and Control-Plane State

Accepted `main` contains:

- the Rust-owned workflow runtime, scheduler, ProductTask, and `LocalProductStore` authority boundaries;
- SQLite default storage with PostgreSQL parity and restart/recovery evidence;
- managed-coding runtime profiles and provider-free DeepSeek protocol/runtime wiring;
- delegated Golden Path authority with separate risk, spend, attempt, artifact approval, output confirmation, and terminal-evidence owners;
- exact-head CI, bounded review convergence, context-capsule transport, and the outbound local engineering loop;
- provider-free RWE/VDE contracts, production RWE v2 issue/admit/one-use spend, the first-live-baseline composition seam, store cell fence, and artifact validation;
- a transport whose authorized finite request timeout is no longer silently capped by the former 20-second body-read ceiling;
- an operator-gated, maximum-two-request compatibility calibration that requires parseable implementation content and is skipped by CI;
- Harness Evolution Level-1 as a default-off one-generation fixture laboratory with immutable active-Harness identity and no production-adoption authority.

No runtime path may merge, release, deploy, write target default branches, or adopt a candidate as the production Harness.

## Minimum First RWE Accepted State

The following prerequisites are accepted:

- Board A frozen v1 corpus/protocol/schedule and strict production authorization contract;
- Board B store-owned production issue/admit/one-use spend wiring;
- PR #363 composition seam, role-separated delegated lifecycle, replay-safe store cell fence, unbypassable Provider provenance, allowed-path containment, restart/cleanup behavior, and honest fixture semantics;
- PR #368 timeout ownership and failure classification repair;
- PR #369 bounded compatibility-calibration mechanism.

These capabilities do not authorize a live run by themselves.

## First Live v1 Attempts — Valid Failure Evidence, Not an Accepted Baseline

Two separately authorized one-use runs on 2026-08-07 (`auth-live-003`/`auth-live-004`, `run-live-20260807-c`/`-d`) executed all four v1 cells through the genuine delegated lifecycle:

- 8/8 planning nodes made real `deepseek-v4-pro` requests successfully;
- 8/8 implementation nodes failed after about 20.2 seconds because the then-current transport body-read timeout was shorter than `deepseek-v4-flash` reasoning generation;
- a direct probe also showed all 4,000 output tokens consumed by `reasoning_content`, leaving empty implementation content;
- no seal, Draft PR, target write, budget breach, outcome-unknown retry, or default-branch effect occurred;
- consumed cost and controlled failures remain valid evidence and must not be deleted or rewritten after v2.

These runs established the root cause and fail-closed behavior. They did not establish a viable baseline or architecture/economic improvement.

## Accepted Readiness Boundary

Accepted `main` contains the provider-free compatibility-calibration mechanism, but it contains no accepted v2 refreeze or viable four-cell RWE baseline. The accepted route may prepare a provider-free refreeze; it does not authorize a calibration rerun, live schedule, target effect, or downstream Architecture Convergence work.

Candidate evidence remains non-authoritative until it is bound to one exact PR head, passes the repository review protocol and canonical CI, and is merged. Do not record candidate branches, PR numbers, CI runs, or review claims here; the capsule observes them at handoff time and fails closed when unavailable or conflicting.

## Capability Status

| Capability | State | Entry or exit condition |
|---|---|---|
| RWE Board A freeze, Board B authority, live composition seam | `COMPLETE` | Accepted on main |
| Timeout ownership repair | `COMPLETE` | PR #368 accepted |
| Compatibility calibration mechanism | `COMPLETE` | PR #369 accepted; mechanism only |
| V2 refreeze + bounded test-race repair | `READY_FOR_EXECUTION` | Receipt/candidate reconciliation, focused PR, exact-head CI/review |
| V2 four-cell viability | `BLOCKED_PREREQUISITE` | 3 packets: current preflight, one authorized run, independent closeout |
| Measurement readiness | `BLOCKED_PREREQUISITE` | 4 packets: estimands, corpus/sample, operations/evidence, protocol freeze |
| Decision-grade pre-AC baseline | `BLOCKED_PREREQUISITE` | 4 packets: snapshot/corpus, preflight, run, analysis |
| AC0 inventory/freeze | `BLOCKED_PREREQUISITE` | 3 packets: runtime, data/contracts, trace/order freeze |
| AC1–AC5 | `BLOCKED_PREREQUISITE` | Each stage has contract, additive core, and enumerated migration/closeout |
| AC6 schema convergence | `BLOCKED_PREREQUISITE` | Contract, Rust/codegen, SDK, Dashboard data migration, compatibility closeout |
| AC7 cleanup | `BLOCKED_PREREQUISITE` | Removal manifest, deletion-only implementation, independent closeout |
| Contemporary old/new replay | `BLOCKED_PREREQUISITE` | Reconstruction, protocol/preflight, authorized run, analysis |
| EC1–EC5 experiment control | `BLOCKED_PREREQUISITE` | 17 packets; causal mutation evidence and each control family freeze before implementation |
| Level-1 core without memory/skill | `BLOCKED_PREREQUISITE` | Preflight, one authorized generation, independent closeout |
| Level-1 transfer pilot | `BLOCKED_PREREQUISITE` | Sealed protocol, authorized run, analysis |
| Optional memory/skill factor experiment | `BLOCKED_PREREQUISITE` | 5-packet side branch; not a Level-2 prerequisite |
| Level-2 GO/NO-GO | `BLOCKED_PREREQUISITE` | Rule audit, independent evidence analysis, explicit human receipt |
| Bounded Level-2 controller | `BLOCKED_PREREQUISITE` | 8 packets from frozen contract through simulation, one pilot, and closeout |
| Final sealed transfer | `BLOCKED_PREREQUISITE` | Protocol, authorized run, analysis |
| Human adoption branch | `BLOCKED_PREREQUISITE` | Readiness dossier then separate human decision |
| Meta Improver branch | `BLOCKED_PREREQUISITE` | 11 packets from claim GO/NO-GO through O0/O1 comparison, replication, and claim decision |
| Optional advanced R4–R6 research | `BLOCKED_PREREQUISITE` | Supported Meta result plus separate human GO; bounded metacognitive, weight-adapter, then one outer-policy family |
| Dashboard #225 / successor | `DEFERRED` | Disposition, presentation refresh, closeout; always last |

## Recursive-Improvement Classification

The repository currently demonstrates neither:

- automatic multi-generation Harness evolution;
- improvement of the improvement operator itself;
- self-referential metacognitive-operator improvement;
- Harness and model-weight/adapter co-evolution;
- outer parent/lever/curriculum-policy evolution;
- stable cross-task or cross-model transfer;
- expanding problem-space exploration;
- continuous learning;
- production self-update.

```text
Harness improvement
!= improvement-operator improvement
!= production adoption
!= general recursive self-improvement
```

A NO-GO, saturation result, diversity collapse, transfer failure, or inability to beat the frozen baseline under equal total lifecycle budget is valid completion and must be preserved.

## Confirmed Integration Gaps

1. No accepted v2 refreeze or viable four-cell RWE baseline exists.
2. The reported 8,192 calibration result lacks an observed durable GitHub-bound redacted receipt at the current frontier.
3. The current two-task/four-cell design is lifecycle viability evidence, not a statistically decision-grade architecture baseline.
4. No accepted task-level measurement-readiness contract, larger decision baseline, or reconstructable contemporary old/new comparison exists.
5. AC0–AC7 have not started; each implementation slice remains blocked until its immediately preceding current-main contract packet is accepted.
6. No accepted causal-mutation, lineage/mutation, evaluator/holdout, lifecycle-budget, diversity/exploration, or Pareto/stop/recovery contract or implementation packet exists.
7. No Level-2 rule audit, controller contract, provider-free conformance, live pilot, final transfer, adoption decision, or fixed Meta operator-comparison result exists.
8. No accepted metacognitive-operator, parameter-efficient training adapter, weight/harness factorial, co-evolution, or outer-policy research contract exists; full-weight and model-architecture evolution remain unrouted.

## Maintenance Boundary

After each accepted merge, update this file only when an accepted capability or confirmed gap changed. Update `docs/NEXT_DECISION.md` only when the current executable window changed, and update `docs/FUTURE_ROUTE.md` only when long-horizon order or a routing-only sketch changed. Never copy live PR, CI, or review state into any of those documents.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, memory projection, novelty score, or scalar VDE index may become production-adoption authority.
