# Current Status

Last updated: 2026-08-13.

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
- PR #380 accepted the repository-maintenance route contract from exact head `e905cf6ec7a989b54e60f913657ca306f33ebf49`; merge `546cabc1ceb98b49b543d0bd90a62fc228e67338`; exact-head `PASS` receipts on both review axes with no open objections; canonical workflow `31386777810` completed every required job successfully. It established the single route-controller boundary without activating Plan execution.
- PR #408 accepted the bounded control-binding integrity repair from exact head `4a2dcf42728ae53f7daaec73e15310e8b0d67b59`; merge `57a86c78c3f9611ce48c5bce249721af23db5532`; both independent review axes returned exact `PASS`; canonical workflow `31593460813` completed every required job successfully. Authority was accepted separately through PR #406 merge `83d735feb157b1ef60501cdfea1ecf5b7f3d05ef`, PR #407 merge `a4b33f942fcc3515a1a32916daa2044ca9fbf54e`, and the planning-only closeout amendment PR #409 merge `d278563b4694c629770a3a7673b1283c6e58568d`.
- The PR #370 evidence binds the external calibration to accepted checkout `ee43eac…`, first-viable 8,192 output tokens, one of at most two requests, and SHA-256 digests for both the restricted raw bundle and redacted receipt. Those digests are evidence references, not permission to expose or commit the restricted bundle.
- A new `main`, PR head, CI result, review receipt, or canonical-document change invalidates older context capsules and branch-local status prose.

## Accepted Packet Receipts

This table is the durable cross-document prerequisite index. A packet may appear here only after merge, exact-head review, canonical CI, and canonical-document synchronization have all been established; live PR state still comes from a fresh capsule.

| Packet | State | Accepted evidence |
|---|---|---|
| `PE7-RWE-V2-REFREEZE-1` | `COMPLETE` | PR #370 exact head `36c92b93975366c3f85471f247a3afb128e5351c`; merge `3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82`; exact-head `PASS`; canonical workflow `31312135471`; redacted calibration and restricted-bundle digests bound in the PR evidence |
| `PE7-CTRL-ROUTE-CONTRACT-1` | `COMPLETE` | PR #380 exact head `e905cf6ec7a989b54e60f913657ca306f33ebf49`; merge `546cabc1ceb98b49b543d0bd90a62fc228e67338`; exact-head `PASS`; canonical workflow `31386777810`; route-contract receipt bound to the accepted main merge |
| `PE7-PLAN-LANE-ACTIVATION-1` | `COMPLETE` | PR #382 exact head `dde26f884ce8a85b776b5933c84c4e6cfd73cb19`; merge `e55e19f1b7c353b4baa2b40ee7b5b16af8918a6c`; exact-head `PASS`; canonical workflow `31395404498` (native-runtime rerun after a confirmed infra-only OpenSSL linker flake); Plan lane active behind real terminal-owner readiness; Plan Execution Ledger Issue #383 provisioned |
| `PE7-LIFECYCLE-CONTROLLER-1` | `COMPLETE` | PR #385 exact head `5867eb9e35151c8252cda26bb6a956dfe80252b0`; merge `ca7e4585c594a5c9820c8d1267858780c28503ac`; exact-head `PASS`; canonical workflow `31401184171`; plan-packet CI/review/merge/closeout receipts recorded on the ledger as controller-owned transitions with idempotent readback |
| `PE7-SUCCESSOR-PROMOTION-ESCALATION-1` | `COMPLETE` | PR #387 exact head `5fe3d55af19aa7a081115637f8f8a7aa63b581af`; merge `597f90282fb6ca72472b890b825684bf54486709`; exact-head `PASS`; canonical workflow `31403849100`; exactly-one successor-promotion receipts (packet id, accepted-main SHA, capsule digest) and bounded pause escalation are controller-owned on the Plan Execution Ledger |
| `PE7-ROUTE-AUTOMATION-1` | `COMPLETE` | PR #390 exact head `24618e52c969adc93e7bc092c51dde6b2d0ffea9`; merge `5481053c736e7db8481cabd9316741f2a5cd6c7a`; exact-head `PASS`; canonical workflow `31467821768` |
| `PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1` | `COMPLETE` | Authority PRs #406/#407/#409; implementation PR #408 exact head `4a2dcf42728ae53f7daaec73e15310e8b0d67b59`; merge `57a86c78c3f9611ce48c5bce249721af23db5532`; exact-head `PASS` on both review axes; canonical workflow `31593460813`; #405 retrospective correction workflow `31594277043` and production readback bind actual head `e68ec0b3a7b78d3ca241922bf3995c2f3ba4ecfa` while retaining `historical_merge_compliant=false` |
| `PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1` | `COMPLETE` | PR #413 base `59cec5745ddd7f89ce8c099a5de2c7e3c3ec3a1e`; exact head `fc8c005981d2fa12f0f494a131b839d65a46a8ba`; exact-head `PASS` receipt comment `5268787985`; canonical workflow `31611860646`; merge `9cc118fa72d9d13a24cdf968cc5fc20dbe80b28f`; deterministic production-path concurrent-winner receipt reuse and genuine missing-receipt rejection |
| `PE7-ROUTE-AUTONOMY-STABILIZATION-1` | `COMPLETE` | PR #416 exact head `9ce548f620314303b37753a18539c17b5daa6698`; merge `306b500c43270ca83d7cb9defd365140b525187c`; exact-head `PASS`; canonical workflow `31630036965` |
| `PE7-ROUTE-AUTOPILOT-SOAK-1` | `COMPLETE` | Closeout PR #429 exact head `92e9b49c13b51ee9c471a6acc2181c37d8084029`; merge `d40c8ce82101922e7270f30bd6da592d72354ffe`; exact-head `PASS`; canonical workflow `31681024633`; OpenCode worker PR #426 exact head `c54860674fbf5045239469c2a842ec88002bb3df`; merge `f02d58b5d1fb8d74dd1c68349e4075eb7641879e`; ledger #383 trusted CI/review/merge/closeout; canonical workflow `31664342318` |
| `PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1` | `COMPLETE` | PR #432 exact head `f31ba002720424deb003728eec52aa9ceae35e33`; merge `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50`; exact-head `PASS`; canonical workflow `31686429471`; contract digest `c8ea4c802e2554b1fa5d0b2f247879ba758d67e4d5df23ed43f1eddadf8aef74` |
| `PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1` | `COMPLETE` | PR #434 exact head `9fdd1045928f862a5b1c1017bc0e9d73e5d50966`; merge `e311db76bf4d2a3a407213b8129a600bc447fd56`; exact-head `PASS`; canonical workflow `31690000442`; durable B2 rule caller-supplied finite expires_at |

## Accepted Product and Control-Plane State

Accepted `main` contains:

- the Rust-owned workflow runtime, scheduler, ProductTask, and `LocalProductStore` authority boundaries;
- SQLite default storage with PostgreSQL parity and restart/recovery evidence;
- managed-coding runtime profiles and provider-free DeepSeek protocol/runtime wiring;
- delegated Golden Path authority with separate risk, spend, attempt, artifact approval, output confirmation, and terminal-evidence owners;
- exact-head CI, bounded review convergence, context-capsule transport, and the outbound local engineering loop;
- the repository-maintenance route contract with one existing queue/lease/controller boundary;
- an activated Plan lane behind real terminal-owner readiness checks, consuming the accepted weak-agent dispatch capsule and the provisioned Plan Execution Ledger Issue #383;
- controller-owned plan-packet lifecycle transitions (CI/review/merge/closeout receipts) recorded on the Plan Execution Ledger with idempotent readback and recovery;
- controller-owned exactly-one successor-promotion and bounded pause-escalation receipts, with no successor execution, EFFECT, or T3 authority;
- controller-derived full-SHA live PR review binding, append-only authenticated retrospective correction/readback, and deterministic plan-artifact allowed-path enforcement before any worktree mutation or GitHub write;
- a 16-input accepted controller dispatch surface, strict compact route-receipt payload validation, focused-check candidate mutation detection, and lifecycle consumer binding across packet, attempt, ledger, PR, head, CI, review, merge, and canonical closeout evidence;
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

Accepted `main` contains the provider-free compatibility-calibration mechanism, the distinct versioned v2 refreeze, the repository-maintenance route implementation, the accepted control-binding integrity repair, the accepted SQLite workspace-preparation receipt-race repair, route bootstrap reconciliation (PR #418, merge `629eac2116ddfefe7342a59ba875d8270e92d689`), route worker-failure containment (PR #422, merge `05e8f5c304a4107012c2a1d1a702ee93a76699e0`), and the bounded local OpenCode weak-worker transport (PR #423 authorization, merge `85fbf3cb9dc156637eb41d3623decdadb5d083eb`; PR #424 transport replacement, merge `5347a51266f23a2abdbe49c9021c2388f3795a5c`; PR #425 OpenCode `--file` argv repair, merge `d2764e49278fc912bf0439f3dc80063ed7fc717a`). Controller status smoke workflow `31631388199` succeeded on exact accepted main `306b500c43270ca83d7cb9defd365140b525187c`, proving that the former 28-input HTTP 422 dispatch defect is removed while orchestration remains enabled, emergency stop remains clear, and auto-merge remains disabled. It contains an accepted route-autopilot soak closeout, **no accepted v2 preflight receipt**, and **no v2 four-cell viability result**. PR #405's original invalid review records remain visible and non-authorizing; the additive retrospective correction proves only that the actual merged code later received exact-head `PASS`, with `historical_merge_compliant=false`. Accepted main now has store-derived B1 preflight `observed_at` and fail-closed Golden Path/RWE `created_at` provenance (PR #434 merge `e311db76bf4d2a3a407213b8129a600bc447fd56`). The durable B2 rule is caller-supplied finite `expires_at` on store-owned `rwe_run_authorization.v2`; no freeze-duration TTL was invented. The current window is the provider-free v2 viability preflight. Neither the route controller nor a compiled successor may issue or consume external-effect authority, write a target, run an external schedule, or authorize downstream measurement/Architecture Convergence work outside each independently accepted packet.

Candidate evidence remains non-authoritative until it is bound to one exact PR head, passes the repository review protocol and canonical CI, and is merged. Do not record candidate branches, PR numbers, CI runs, or review claims here; the capsule observes them at handoff time and fails closed when unavailable or conflicting.

## Capability Status

| Capability | State | Entry or exit condition |
|---|---|---|
| RWE Board A freeze, Board B authority, live composition seam | `COMPLETE` | Accepted on main |
| Timeout ownership repair | `COMPLETE` | PR #368 accepted |
| Compatibility calibration mechanism | `COMPLETE` | PR #369 accepted; mechanism only |
| V2 refreeze + bounded test-race repair | `COMPLETE` | PR #370 accepted; exact v2 freeze and canonical lock repair are on main |
| Repository-maintenance route contract | `COMPLETE` | PR #380 accepted; its queue/lease/controller boundary is consumed by the accepted Plan lane |
| Plan-lane activation | `COMPLETE` | PR #382 accepted; Plan lane active behind terminal-owner readiness; ledger Issue #383 provisioned |
| Plan-packet lifecycle controller | `COMPLETE` | PR #385 accepted; CI/review/merge/closeout receipts controller-owned on the ledger with idempotent readback |
| Plan-lane successor promotion and escalation | `COMPLETE` | PR #387 accepted; exactly-one successor-promotion receipts and bounded pause escalation are controller-owned on the existing ledger |
| Route automation | `COMPLETE` | PR #416/#418/#422 accepted the control-plane path; PR #426 completed one OpenCode-backed code-and-document packet; PR #429 persisted trusted CI/review/merge/closeout on ledger #383 and closed the soak |
| `PE7-ROUTE-AUTONOMY-STABILIZATION-1` | `COMPLETE` | PR #416 accepted the stabilization implementation and PR #418 completed its one-time bootstrap reconciliation; overall Route automation remains incomplete until the soak, real packet lifecycle, and successor advancement are proved |
| Control binding integrity | `COMPLETE` | PR #408 accepted strict live exact-head review binding, authenticated append-only #405 correction/readback, and deterministic pre-effect plan-artifact scope enforcement |
| `PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1` | `COMPLETE` | PR #413 base `59cec5745ddd7f89ce8c099a5de2c7e3c3ec3a1e`, exact head `fc8c005981d2fa12f0f494a131b839d65a46a8ba`, exact-head review receipt comment `5268787985`, canonical workflow `31611860646`, and squash merge `9cc118fa72d9d13a24cdf968cc5fc20dbe80b28f` accepted transactionally consistent SQLite receipt/status observation with deterministic concurrent-winner reuse and genuine missing-receipt rejection |
| Route-autopilot adversarial soak | `COMPLETE` | PR #426 worker plus PR #429 closeout: one real OpenCode-backed packet through existing PR/CI/review/merge/closeout owners, with trusted ledger receipts after GitHub verify |
| V2 provider-free viability preflight | `READY_FOR_EXECUTION` | Provider-free operator_preflight ready=true without issue/admit/spend; unissued request package sha256 `015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a`; closeout still required |
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

1. No accepted four-cell v2 RWE baseline exists. A provider-free preflight ran ready=true with zero blockers and no issue/admit/Provider/target effect; the unissued request package digest is `015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a`. The durable B2 rule remains caller-supplied finite `expires_at`.
2. The current two-task/four-cell design is lifecycle viability evidence, not a statistically decision-grade architecture baseline.
3. No accepted task-level measurement-readiness contract, larger decision baseline, or reconstructable contemporary old/new comparison exists.
4. AC0–AC7 have not started; each implementation slice remains blocked until its immediately preceding current-main contract packet is accepted.
5. No accepted causal-mutation, lineage/mutation, evaluator/holdout, lifecycle-budget, diversity/exploration, or Pareto/stop/recovery contract or implementation packet exists.
6. No Level-2 rule audit, controller contract, provider-free conformance, live pilot, final transfer, adoption decision, or fixed Meta operator-comparison result exists.
7. No accepted metacognitive-operator, parameter-efficient training adapter, weight/harness factorial, co-evolution, or outer-policy research contract exists; full-weight and model-architecture evolution remain unrouted.
8. The failed bootstrap from accepted main `aa83ac1f5eada74199e0ce28ecb91d37a48769d6` remains valid non-authorizing evidence: it stopped with `route_controller_unavailable_timeout` after GitHub rejected 28 workflow inputs with HTTP 422, before any workflow run, PR, claim, Provider call, target write, or external effect. PR #416 and accepted-main smoke `31631388199` removed that exact dispatch blocker. The route remains stopped until the one-time merge-backed bootstrap starts from current main; route10 remains non-resumable obsolete-main evidence.

## Maintenance Boundary

After each accepted merge, update this file only when an accepted capability or confirmed gap changed. Update `docs/NEXT_DECISION.md` only when the current executable window changed, and update `docs/FUTURE_ROUTE.md` only when long-horizon order or a routing-only sketch changed. Never copy live PR, CI, or review state into any of those documents.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, memory projection, novelty score, or scalar VDE index may become production-adoption authority.
