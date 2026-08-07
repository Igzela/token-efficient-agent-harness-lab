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

PR #363 is merged and accepted as `995e57e50defd85632b782e3da87416e62cf6d92`. It lands the frozen live-baseline **composition seam and store cell fence** as accepted main capability:

- role-separated delegated attempt admit/activate, one-use v2 spend authority, per-cell dispatch reservation (single winner, replay-safe), terminalization ordering, restart recovery, SQLite/PostgreSQL parity;
- unbypassable provider transport provenance: the `HttpTransport` trait has no self-declaration surface; `External` is minted only for the canonical `ReqwestTransport` concrete type, the fake seam is always wrapped in `InjectedTransportBoundary`, and injected/provider-free execution can never seal a live baseline;
- strict one-way allowed-path containment (parents, `..`, absolute, and pseudo-children of file entries fail closed);
- fixture honesty: the operator-gated armed integration fixture (`ACP_RWE_ARMED_LIVE_RUN=1`) runs the genuine delegated lifecycle through the injected transport, records cells as `fixture_success` (never `success`), and never seals; the CLI live path additionally requires `ACP_RWE_OPERATOR_LIVE_RUN=1`;
- fixture completion is not fixture success: `integration_fixture_succeeded` requires every cell `fixture_success`; merely terminalized failure classes never report `succeeded`;
- the frozen corpus tree-hash inconsistency was repaired deterministically (task bodies unchanged; `137e912f…` independently recomputed as the supervised-patch content hash of the frozen commit).

#363 authorizes no live baseline by itself: a separate one-use live-run authorization against the accepted exact head is still required before any real provider POST or target effect.

The frozen corpus, protocol, schedule, and Board B production wiring are accepted prerequisites. They do **not** authorize a live RWE baseline by themselves.

## First Live Baseline Attempts — Not Accepted

Two separately authorized one-use live runs of the frozen Board A schedule were executed against the accepted exact head on 2026-08-07 (authorizations `auth-live-003`/`auth-live-004`, operator principal `op-live-001`, executor/confirmer cell keys with role-separated scopes, `fixture_only=false`, draft-PR-only, auto-merge disabled):

- run-live-20260807-c and run-live-20260807-d each executed all 4 frozen cells through the genuine delegated lifecycle (store owners, real provider calls, real `ReqwestTransport` external provenance).
- Every planning node (deepseek-v4-pro) completed a real provider request (8 requests, USD 0.002042876 client-recorded spend, well under the USD 0.80 ceiling). Every implementation node (deepseek-v4-flash) failed deterministically after ~20.2 s with `provider_response: DeepSeek response transport was malformed`.
- Root cause (verified): the engine transport enforces a hard 20-second read timeout (`HTTP_READ_TIMEOUT`, `engine/src/provider/transport.rs`); deepseek-v4-flash is a reasoning model whose implementation-stage completion takes ~38.7 s server-side (verified by direct operator probe: HTTP 200, `finish_reason=length`, all 4000 output tokens spent on `reasoning_content`, `content=""`). Secondary finding: with `max_output_tokens` 4000 the frozen model pairing cannot emit content, so the implementation stage would also fail after a timeout repair.
- Outcome: no seal, no Draft PRs, no target-repo writes, no budget breach; 8/8 cells `controlled_failure`; schedule unchanged (no retune, no refreeze); no engine change was made mid-experiment.
- Raw evidence frozen under `/tmp/opencode/rwe-live-baseline-evidence/` (run JSONs, store table exports, diagnostic probe, receipt); independent evidence review `rev-live-20260807` returned overall PASS with five secondary items now corrected in the receipt.

The next live attempt requires a planning decision: (1) a bounded repair packet for the transport read timeout (engine change, new head) and (2) a decision on the frozen schedule's model/output-token pairing (refreeze or bound adjustment). Neither may proceed silently from this document.

## Active Frontier

The first live frozen RWE baseline is `DECISION_REQUIRED`. Two authorized one-use live runs (auth-live-003/004, 2026-08-07) executed the frozen schedule through the genuine delegated lifecycle and failed deterministically at the implementation stage; root cause is the engine transport's hidden 20-second read timeout (`HTTP_READ_TIMEOUT`) against deepseek-v4-flash reasoning generation (~38.7 s server-side), with a secondary model/output-token pairing finding (all 4000 output tokens consumed by `reasoning_content`, `content=""`). Details and frozen evidence are recorded above under "First Live Baseline Attempts — Not Accepted".

The next permitted work is the approved repair chain, not another live run:

1. bounded transport timeout-ownership repair (persisted `ManagedCallLimits.timeout_ms` remains the provider-call timeout authority; no hidden stricter transport ceiling; correct timeout/connection/parse classification) — engine PR;
2. after the repair is accepted, one provider-compatibility calibration for deepseek-v4-flash implementation content (pre-registered candidate envelopes, max 2 provider requests, first viable bound wins, no evaluator/task-success signal) — not a scoring run;
3. after calibration, a versioned refreeze that keeps every experiment field except the compatibility-necessary output-token envelope and dependent bindings — new corpus/protocol/schedule hashes, old failed attempts remain valid failure evidence;
4. only after all three are accepted may a new one-use, finite, new-freeze-bound, accepted-main-bound live-run authorization be requested for a new 4-cell First Live Frozen RWE Baseline.

AC1–AC7 remain `BLOCKED_PREREQUISITE` until an accepted pre-convergence RWE baseline exists. No branch-local head authorizes live baseline, provider calls, or target effects. The accepted composition seam (merged #363) runs the genuine delegated lifecycle through the injected transport only when the operator gate `ACP_RWE_ARMED_LIVE_RUN=1` is set with populated token references; the CLI live path additionally requires `ACP_RWE_OPERATOR_LIVE_RUN=1`. Without the gates the test SKIPs, so CI never creates external effects, and the fixture never seals a live baseline.

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
| Live baseline composition seam + store cell fence | `COMPLETE` | PR #363 accepted as `995e57e…`; authorizes no live run by itself |
| First live frozen RWE baseline | `DECISION_REQUIRED` | Attempted 2026-08-07 (auth-live-003/004), not accepted; deterministic transport read-timeout failure root-caused; repair packet + schedule pairing decision required before next attempt |
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

1. No accepted live frozen RWE baseline exists; the first baseline is `DECISION_REQUIRED` pending the approved transport repair, compatibility calibration, and versioned refreeze chain.
2. Architecture Convergence has not started because its baseline prerequisite is absent.
3. No diversity, contamination, evaluator-gaming, or mutation-family control packet has been implemented for generational Harness experiments.
4. No accepted memory/skill projection experiment exists; raw evidence remains authoritative and summaries remain non-authoritative projections.
5. No Level-2, sealed transfer, or Meta Improver result exists.

## Open Work Coordination

The first live frozen RWE baseline is `DECISION_REQUIRED`. The approved chain, executed in order on accepted `main`, is: transport timeout-ownership repair → provider-compatibility calibration (after repair accepted) → versioned refreeze (after calibration) → new one-use live-run authorization. The authoritative step-by-step chain and each step's acceptance conditions are in `docs/NEXT_DECISION.md` (`PE7-REAL-WORKLOAD-EVIDENCE-1` packet); this file records only the status facts: the repair is in flight as an engine PR, calibration and refreeze have not started, and no step may begin before the previous one is accepted.

AC1–AC7 remain `BLOCKED_PREREQUISITE`; an accepted pre-convergence baseline moves `PE7-ARCHITECTURE-CONVERGENCE-1` to the next active frontier, and AC implementation stays out of this round.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, memory projection, novelty score, or scalar VDE index may become production-adoption authority.
