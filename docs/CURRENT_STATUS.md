# Current Status

Last updated: 2026-08-08.

This document separates three states that must not be conflated:

1. **Merged and accepted truth** — code and governing documents on remote `main` that passed their required checks.
2. **Open or local candidate surfaces** — proposed work that is not authoritative until its final exact head is accepted and merged.
3. **Blocked or deferred work** — future work that remains ineligible because a named evidence, planning, or authority gate is incomplete.

Historical packet detail remains in Git history and merged PRs; do not append stale chronology here.

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

## Reported Calibration and Local Candidate — Not Yet Accepted

The latest operator/agent handoff reports:

- the armed compatibility calibration selected 8,192 as the first envelope returning parseable implementation content;
- redacted evidence exists under `/tmp/opencode/rwe-calibration-evidence/`;
- a local v2 refreeze changes 4,000 → 8,192 output tokens, 16,000 → 20,192 per-cell total tokens, and 64,000 → 80,768 four-cell total tokens, with dependent version/hash/binding changes only;
- the same candidate repairs a test-only environment race: four duplicate locks in `engine/tests/test_http_server.rs` delegate to the canonical statics in `engine/tests/http_server/common.rs`, while the two affected auth tests hold the same provider lock;
- `engine/tests/http_server/tick.rs` was confirmed unwired and was not used as a production fix;
- reported verification is 206 `test_http_server` tests ×4 consecutive runs, full engine 1692 passed/1 ignored, fmt and clippy clean.

This is handoff evidence, not accepted repository truth. At the current observed frontier there is no remote branch or PR containing that candidate, no exact candidate head, no canonical CI, and no exact-head independent review. The temporary calibration bundle is not currently represented by a durable GitHub-bound redacted receipt.

## Active Frontier

`PE7-RWE-V2-REFREEZE-1` is `READY_FOR_EXECUTION` as a provider-free packet.

The accepted forward route is decomposed into 98 structural micro-packets: the current refreeze plus 97 explicitly blocked successors. The successor packets separate contract freeze, bounded implementation, external effect, and evidence/decision closeout so an implementation agent never has to invent an architecture, statistical, evaluator, spend, retention, or adoption decision mid-packet. This decomposition does not make any successor eligible.

The first permitted action is to reconcile the calibration receipt and actual local candidate against accepted `main`. If the candidate is unavailable, reconstruct it from accepted main under the exact allowed delta in `docs/NEXT_DECISION.md`. If 8,192 parseability cannot be independently supported, stop `DECISION_REQUIRED`; do not rerun a paid calibration or choose 16,384 without new authority.

No new live run is authorized. A merged refreeze only makes the separately authorized v2 viability packet eligible.

## Open Review Surfaces

| PR | Exact head | Current evidence | Rule |
|---|---|---|---|
| #365 | `fd60c17b511da9e0a61683d874cc26e03b05ba39` | Draft; fast/exact-head checks successful; no independent review observed | Separate documentation clarification; not the active RWE owner |
| #225 | `899fc24dec671fd092a3311a240529aa119324f6` | Open; earlier canonical workflow `29554170902` successful; no independent review observed | Presentation-only and last |

`Igzela/alters-lab#5` remains an external Draft output from an earlier accepted bounded live Golden Path observation. It grants no merge authority and is not the active Harness-repository frontier.

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
| EC1–EC5 experiment control | `BLOCKED_PREREQUISITE` | 15 packets; each control family freezes its contract before implementation |
| Level-1 core without memory/skill | `BLOCKED_PREREQUISITE` | Preflight, one authorized generation, independent closeout |
| Level-1 transfer pilot | `BLOCKED_PREREQUISITE` | Sealed protocol, authorized run, analysis |
| Optional memory/skill factor experiment | `BLOCKED_PREREQUISITE` | 5-packet side branch; not a Level-2 prerequisite |
| Level-2 GO/NO-GO | `BLOCKED_PREREQUISITE` | Rule audit, independent evidence analysis, explicit human receipt |
| Bounded Level-2 controller | `BLOCKED_PREREQUISITE` | 8 packets from frozen contract through simulation, one pilot, and closeout |
| Final sealed transfer | `BLOCKED_PREREQUISITE` | Protocol, authorized run, analysis |
| Human adoption branch | `BLOCKED_PREREQUISITE` | Readiness dossier then separate human decision |
| Meta Improver branch | `BLOCKED_PREREQUISITE` | 11 packets from claim GO/NO-GO through O0/O1 comparison, replication, and claim decision |
| Dashboard #225 / successor | `DEFERRED` | Disposition, presentation refresh, closeout; always last |

## Recursive-Improvement Classification

The repository currently demonstrates neither:

- automatic multi-generation Harness evolution;
- improvement of the improvement operator itself;
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
6. No accepted lineage/mutation, evaluator/holdout, lifecycle-budget, diversity/exploration, or Pareto/stop/recovery contract or implementation packet exists.
7. No Level-2 rule audit, controller contract, provider-free conformance, live pilot, final transfer, adoption decision, or Meta operator-comparison result exists.

## Open Work Coordination

The authoritative next packet is `PE7-RWE-V2-REFREEZE-1`. It may reconcile or reconstruct the reported provider-free candidate, verify v1/v2 hashes and the lock-race root cause, and publish one focused Draft PR. It may not make a Provider call, execute the four-cell schedule, change target content, alter experiment fields beyond the compatibility delta, broaden an authority owner, or begin AC.

After that packet is accepted, `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` may produce a current provider-free preflight and authorization request package. A separate current operator authorization is required only for `PE7-RWE-V2-VIABILITY-RUN-1`; its independently closed result advances to measurement design, not directly to AC.

The full ordered route, 98 structural micro-packets, four packet classes, consolidation limits, progressive execution-readiness rule, acceptance gates, and stop triggers are owned by `docs/NEXT_DECISION.md`. Packet count is not a target: a negative decision may terminate or rewrite a branch, while consolidation is allowed only under the strict same-owner/same-path/same-rollback rule defined there.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, memory projection, novelty score, or scalar VDE index may become production-adoption authority.
