# Current Status

Last updated: 2026-08-10.

This document owns accepted repository truth and confirmed capability gaps only. It separates two states that must not be conflated:

1. **Merged and accepted truth** — code and governing documents on remote `main` that passed their required checks.
2. **Confirmed gaps** — capabilities or evidence not yet accepted on `main`.

Open PR heads, Draft/Ready state, CI, reviews, mergeability, and the next permitted action are live observations and must come from a fresh context capsule. Current execution routing belongs in `docs/NEXT_DECISION.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`. Historical packet detail remains in Git history and merged PRs; do not append stale chronology here.

## Verified Repository State

- Repository: `Igzela/token-efficient-agent-harness-lab`.
- Active runtime/code baseline: PR #370 merge `3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82`; later PR #373 changed only canonical documentation and PR #375 changed only the engine PostgreSQL integration test file, and neither changes that code identity. Always refresh remote `main` for the current canonical-document head.
- PR #368 accepted the provider timeout-ownership repair from exact head `17cc5d03…`; its exact-head review receipt reports `PASS` with no open findings, and its canonical exact-head workflow completed successfully before merge.
- PR #369 accepted the operator-gated compatibility-calibration mechanism from exact head `b571c95a75a7c8eacda99a8f586d8f2360868ab7`; canonical exact-head workflow `31172449577` completed successfully and the exact-head review receipt reports `PASS` with no open findings.
- PR #370 accepted the versioned v2 RWE refreeze from exact head `36c92b93975366c3f85471f247a3afb128e5351c`; exact-head review reports `PASS` with no open objections, canonical workflow `31312135471` completed every required job successfully, and the merged v2 corpus/protocol/schedule hashes are `044fcd7b…`, `bc68bfb3…`, and `6a729f12…`.
- PR #375 accepted the weak-agent executable session routing and fail-closed recovery repair (superseding rejected #374) from exact head `2aea374e2ff26b798a104884658e3af5c6a378e4`; merge `63fbc4e264d2a1f2250299e25dcf168d71376aef`; exact-head `PASS` receipts on both review axes with no open objections; canonical exact-head workflow completed every required job including `pg-integration-tests` and the terminal context capsule. The same PR carried a bounded repair de-rotting 16 hard-coded `2026-07` timestamps in `engine/tests/test_pg_integration.rs` that crossed the strict 30-day operator-decision freshness bound on 2026-08-10 (pre-existing `main` calendar rot, reproduced on a pristine `origin/main` worktree before the PR was touched); test-data only, zero production code.
- The PR #370 evidence binds the external calibration to accepted checkout `ee43eac…`, first-viable 8,192 output tokens, one of at most two requests, and SHA-256 digests for both the restricted raw bundle and redacted receipt. Those digests are evidence references, not permission to expose or commit the restricted bundle.
- A new `main`, PR head, CI result, review receipt, or canonical-document change invalidates older context capsules and branch-local status prose.

## Accepted Packet Receipts

This table is the durable cross-document prerequisite index. A packet may appear here only after merge, exact-head review, canonical CI, and canonical-document synchronization have all been established; live PR state still comes from a fresh capsule.

| Packet | State | Accepted evidence |
|---|---|---|
| `PE7-RWE-V2-REFREEZE-1` | `COMPLETE` | PR #370 exact head `36c92b93975366c3f85471f247a3afb128e5351c`; merge `3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82`; exact-head `PASS`; canonical workflow `31312135471`; redacted calibration and restricted-bundle digests bound in the PR evidence |

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

- Board A frozen v1 corpus/protocol/schedule, accepted distinct v2 refreeze, and strict production authorization contract;
- Board B store-owned production issue/admit/one-use spend wiring;
- PR #363 composition seam, role-separated delegated lifecycle, replay-safe store cell fence, unbypassable Provider provenance, allowed-path containment, restart/cleanup behavior, and honest fixture semantics;
- PR #368 timeout ownership and failure classification repair;
- PR #369 bounded compatibility-calibration mechanism.
- PR #370 v1-byte-preserving v2 corpus/protocol/schedule refreeze, semantic whitelist, freeze/hash locks, and bounded HTTP-test lock repair.

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

Accepted `main` contains the provider-free compatibility-calibration mechanism and the distinct, versioned v2 refreeze. It contains **no accepted v2 preflight receipt** and **no v2 four-cell viability result**. `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` is no longer a future-route sketch: it is the current window in `docs/NEXT_DECISION.md`, parked `DECISION_REQUIRED` because two contract values (preflight-receipt maximum age; one-use authorization request-package expiry policy) have no accepted owner on current `main`. Current execution state and routing belong to `docs/NEXT_DECISION.md`. The packet's provider-free preflight and operator-readable authorization request package must not issue or consume authority, call a Provider, write a target, run the schedule, or authorize downstream measurement/Architecture Convergence work.

Candidate evidence remains non-authoritative until it is bound to one exact PR head, passes the repository review protocol and canonical CI, and is merged. Do not record candidate branches, PR numbers, CI runs, or review claims here; the capsule observes them at handoff time and fails closed when unavailable or conflicting.

## Capability Status

| Capability | State | Entry or exit condition |
|---|---|---|
| RWE Board A freeze, Board B authority, live composition seam | `COMPLETE` | Accepted on main |
| Timeout ownership repair | `COMPLETE` | PR #368 accepted |
| Compatibility calibration mechanism | `COMPLETE` | PR #369 accepted; mechanism only |
| V2 refreeze + bounded test-race repair | `COMPLETE` | PR #370 accepted; exact v2 freeze and canonical lock repair are on main |
| V2 provider-free viability preflight | `DECISION_REQUIRED` | Prerequisite accepted (#370); packet block expanded in `docs/NEXT_DECISION.md` against current `main` with two unresolvable-from-`main` contract values (preflight-receipt maximum age B1; one-use authorization request-package expiry policy B2); the planning owner or operator must accept those values before any coding entry |
| V2 four-cell run and closeout | `BLOCKED_PREREQUISITE` | Preflight acceptance, then one separately authorized run and independent closeout |
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

1. No accepted provider-free v2 preflight receipt or viable four-cell v2 RWE baseline exists.
2. The current two-task/four-cell design is lifecycle viability evidence, not a statistically decision-grade architecture baseline.
3. No accepted task-level measurement-readiness contract, larger decision baseline, or reconstructable contemporary old/new comparison exists.
4. AC0–AC7 have not started; each implementation slice remains blocked until its immediately preceding current-main contract packet is accepted.
5. No accepted causal-mutation, lineage/mutation, evaluator/holdout, lifecycle-budget, diversity/exploration, or Pareto/stop/recovery contract or implementation packet exists.
6. No Level-2 rule audit, controller contract, provider-free conformance, live pilot, final transfer, adoption decision, or fixed Meta operator-comparison result exists.
7. No accepted metacognitive-operator, parameter-efficient training adapter, weight/harness factorial, co-evolution, or outer-policy research contract exists; full-weight and model-architecture evolution remain unrouted.

## Maintenance Boundary

After each accepted merge, update this file only when an accepted capability or confirmed gap changed. Update `docs/NEXT_DECISION.md` only when the current executable window changed, and update `docs/FUTURE_ROUTE.md` only when long-horizon order or a routing-only sketch changed. Never copy live PR, CI, or review state into any of those documents.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, memory projection, novelty score, or scalar VDE index may become production-adoption authority.
