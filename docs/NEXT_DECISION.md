# Next Decision

Last updated: 2026-08-14.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1`, `PE7-RWE-MR-CORPUS-SAMPLING-1`, and `PE7-RWE-MR-OPERATIONS-EVIDENCE-1` are accepted on main with their source-bound measurement, corpus, and operations contracts. The human value owner for the measurement-readiness route is Igzela, the repository owner; this session's implementation agent is explicitly delegated to record and execute the remaining T2 value judgments for this route. The current window is `PE7-RWE-MR-PROTOCOL-FREEZE-1` `READY_FOR_EXECUTION` for provider-free protocol canonicalization and validation only.

## Authoritative Forward Order

```text
[window: PE7-RWE-MR-PROTOCOL-FREEZE-1 — READY_FOR_EXECUTION, delegated protocol freeze]

→ `PE7-RWE-DB-SNAPSHOT-CORPUS-1` after this packet is merged and closed
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-MR-PROTOCOL-FREEZE-1` — `READY_FOR_EXECUTION`

## Historical V2 Closeout

**State:** `COMPLETE`

**Evidence:** Disposition `CONTROLLED_FAILURE`; run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; four frozen cells; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`; no seal and no target-default-branch write. Do not rerun this effect.

## Packet PE7-RWE-MR-PROTOCOL-FREEZE-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-RWE-MR-OPERATIONS-EVIDENCE-1 — COMPLETE on accepted main e34d1ae3c3ecf5e6c919c71a3d26d6690a66444; PR #446 exact head 34c68d94c1737769c60fb7ea1722b464a5d764aa; independent review receipt comment 5289427966; canonical tests workflow 31769511015; exact-head check 31769511065.

**Class:** CLOSEOUT

**Outcome:** Assemble and independently verify the complete decision-baseline plus contemporary-replay protocol.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md` only; mechanical canonicalization, hashing, validation, and review. No new threshold, evaluator, runtime, schema, owner, spend, Provider, target, or external-effect authority.

**Exit:** One versioned hash-bound rwe_decision_baseline_protocol.v1 manifest, corpus-selection rule, authorization envelope template, analysis plan, and reconstructability manifest with zero unresolved decision field. Explicit fail-closed stop states count as resolved dispositions; they do not authorize a run.

**Stop:** Cross-document contradiction, post-outcome tunable field, missing owner, excessive unaffordable envelope, or incomplete rollback/retention contract.

### Protocol freeze manifest — rwe_decision_baseline_protocol.v1

1. **Scope and authority.** This is provider-free closeout work. It grants no spend, execution, output, merge, release, deployment, adoption, EFFECT, or T3 authority. The accepted v2 four-cell result remains CONTROLLED_FAILURE and is not rerun or used to tune a decision rule.
2. **Frozen inputs.** Accepted main is e34d1ae3c3ecf5e6c919c71a3d26d6690a66444. The source repository is https://github.com/Igzela/alters-lab at commit 6240768506320a324d68787b9eaa86971c8c930c, source tree 137e912f416a3a8d5be307e91bb2580154fc8fc34c6de52c2441ef3e3f93a064. The corpus, protocol, and schedule bindings are 044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20, bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db, and 6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38. Task-definition hashes are fcd13b6f7a970c048fd09e1f723a315b8e03d221cad1555bf694ca95115438f8 and f49e374d8b818d9e2cf4566d6fb3323c472a3dd449ebc71413eab96891124e7d.
3. **Field owners and dispositions.**

| Required field(s) | Existing owner or frozen output | Missing or ambiguous rule |
|---|---|---|
| accepted main, source commit/tree, corpus/protocol/schedule, task definitions | accepted-main receipt; engine/src/rwe/operator_corpus.rs; engine/src/rwe/corpus.rs; engine/src/rwe/economic_protocol.rs; engine/src/rwe/execution_schedule.rs; versioned engine/rwe/corpora/ artifacts | any mismatch stops protocol validation and no replacement is allowed |
| task/cell/repetition/seed, mutable paths, verifier, executor/model, budget point | versioned v2 protocol and schedule artifacts; engine/src/rwe/live_baseline_coordinator.rs | caller assertions cannot fill a missing or conflicting value; preflight stops |
| primary value, hard gates, non-inferiority margins, repetition aggregation, uncertainty method | accepted measurement-readiness estimand contract (PR #444) and frozen v2 protocol object; current status accepted receipt | no observed result may alter a value or gate; unavailable required evidence is not imputed |
| reviewer policy and acceptance rubric | frozen v2 protocol object reviewer_policy and acceptance_rubric keys; operations-evidence manifest | policy is frozen before outcomes; task/cell reviewer evidence remains unavailable until an existing receipt proves it |
| authorization envelope, expiry, one-use spend | engine/src/storage/local_product_store/rwe_authority.rs and engine/src/rwe/runner.rs, schema rwe_run_authorization.v2 | only store-owned v2 authority with caller-supplied finite expires_at is admissible; absent or expired authority stops |
| protocol cost, request/token/latency, lifecycle cost, failure charging | versioned v2 protocol cost_completeness; engine/src/execution_usage/; LocalProductStore journal; operations-evidence manifest | unknown remains unavailable, failed/outcome-unknown cost is retained, and no second cost ledger is created |
| decision-baseline protocol, corpus-selection rule, analysis plan, reconstruction manifest | this versioned manifest in docs/NEXT_DECISION.md, validated against the existing RWE owners and docs/ARCHITECTURE_BOOK.md | this packet may canonicalize/hash only; any new semantic choice or missing owner stops |
| contemporary old/new replay identity and toolchain/dependency reconstruction | operations-evidence manifest and the successor snapshot packet under existing artifact owners | current disposition is reconstructable=false until exact snapshot receipts exist; no replay run is allowed before that |
| redacted public evidence, restricted raw access, retention, deletion | existing terminal/artifact redaction owners; operations-evidence manifest | raw access/retention/deletion is UNAVAILABLE_NOW; no raw copy or deletion, and no baseline run, is permitted until an accepted owner/receipt exists |
| review, CI, merge, closeout, rollback | GitHub and docs/REAL_WORLD_TESTING_PLAYBOOK.md; accepted PR #446 receipt; existing artifact/store owners | exact-head review and canonical CI are repository evidence only; failed or unknown evidence remains preserved |

4. **Decision rule.** Use the accepted task-level verified_delivery_points value, machine-verification, reviewer-acceptance, and recovery-failure estimands with the frozen v2 margins: -0.10 for the first three lower bounds and +0.05 for the recovery-failure upper bound. Aggregate two registered repetitions within each task; the task is unavailable if either required repetition is unavailable. Use paired-bootstrap-95 at 95% confidence, resampling tasks while retaining candidate/baseline pairing. No post-outcome exclusion, threshold tuning, partial repetition promotion, or unknown-as-zero conversion is permitted.
5. **Corpus selection.** Use only the two source-bound tasks and their two registered repetitions from the frozen v2 corpus. The finite four-cell schedule, seeds, mutable paths, verification commands, executor, model, and budget points are immutable inputs. A changed source tree, task definition, protocol, schedule, or accepted main invalidates the candidate and stops.
6. **Authorization envelope.** Any later EFFECT must use store-owned rwe_run_authorization.v2 with a caller-supplied finite expires_at, exact frozen bindings, one-use spend consumption, and existing cleanup/journal owners. This CONTRACT packet never issues or consumes it.
7. **Contemporary replay.** The protocol requires an exact pre-convergence Harness/configuration/dependency/toolchain/environment snapshot and a separately verified reconstruction manifest before old/new replay. Until the snapshot packet emits those exact receipts, reconstructable=false is the fixed disposition and replay/preflight stops.
8. **Evidence and retention.** Public evidence is bounded redacted/digest-only. Raw prompts, outputs, transcripts, credentials, private paths, and unredacted repository content are never committed or published. Because no accepted current owner/receipt proves restricted raw access, retention, or deletion, this protocol grants no raw copy/delete authority and fixes that field as UNAVAILABLE_NOW.
9. **Rollback and closeout.** Rollback is one documentation revert with all accepted operations, controlled-failure, digest, review, and CI evidence retained. Closeout requires provider-free handoff checks, independent complete-diff review, exact-head canonical CI, manual merge, and refreshed accepted main. No outcome is accepted by this manifest alone.
10. **Next action.** Keep the PR Draft while changing; obtain stable-head two-axis PASS, mark Ready once, wait for exact-head canonical CI, manually squash-merge, record the closeout receipt, refresh main, and only then promote PE7-RWE-DB-SNAPSHOT-CORPUS-1.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-RWE-MR-PROTOCOL-FREEZE-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"opencode_local_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Freeze and validate the hash-bound decision-baseline and contemporary-replay protocol without executing a task or effect.","rollback":"Revert the single documentation commit to restore the prior current window while retaining the accepted operations-evidence and all historical evidence.","allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md"],"allowed_outputs":["The versioned rwe_decision_baseline_protocol.v1 manifest and bounded closeout receipt in docs/NEXT_DECISION.md."],"prerequisites":["PE7-RWE-MR-OPERATIONS-EVIDENCE-1"],"prerequisite_receipts":["PE7-RWE-MR-OPERATIONS-EVIDENCE-1 COMPLETE: PR #446 exact head `34c68d94c1737769c60fb7ea1722b464a5d764aa`; merge `e34d1ae3c3ecf5e6c919c71a3d26d6690a66444`; exact-head review receipt comment `5289427966`; canonical workflow `31769511015`; exact-head check `31769511065`"],"forbidden_changes":["Any runtime, schema, migration, fixture, Provider, credential, target, evaluator, scheduler, store, or external-effect change.","Do not add thresholds, post-outcome exclusions, a new owner, or a second protocol/ledger."],"forbidden_next_actions":["Do not call a Provider, read credentials, execute a task, or rerun run-live-20260813-v2c.","Do not write a target default branch, execute EFFECT/T3, release, deploy, or merge automatically.","Do not start PE7-RWE-DB-SNAPSHOT-CORPUS-1 before this packet is merged and closed."],"ordered_steps":["Bind the accepted operations-evidence receipt and frozen v2 artifacts.","Record and hash-check rwe_decision_baseline_protocol.v1 with explicit stop states.","Run provider-free handoff checks and prepare the governed Draft PR."],"read_paths":["docs/NEXT_DECISION.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/ARCHITECTURE_BOOK.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","engine/src/rwe/operator_corpus.rs","engine/src/rwe/corpus.rs","engine/src/rwe/economic_protocol.rs","engine/src/rwe/execution_schedule.rs","engine/src/rwe/runner.rs","engine/src/rwe/live_baseline_coordinator.rs","engine/src/storage/local_product_store/rwe_authority.rs","engine/rwe/corpora/rwe-minimum-first-corpus/v2/protocol/rwe_economic_protocol.v1.json","engine/rwe/corpora/rwe-minimum-first-corpus/v2/schedule/execution_schedule.v1.json"],"verification":["PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context.CheckpointTests.test_current_repository_packet_binds_safe_live_capsule","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"expected_artifacts":["Versioned rwe_decision_baseline_protocol.v1 manifest in docs/NEXT_DECISION.md.","Explicit frozen inputs, analysis rule, authorization envelope, replay reconstruction stop, and rollback/retention rules."],"pause_gates":["Stop before any Provider call, credential access, task execution, target write, EFFECT, T3 action, release, deployment, or automatic merge.","Stop if any decision field is contradictory, post-outcome tunable, unowned, unaffordable, or lacks a fixed fail-closed disposition."]}
-->

## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Existing route boundary (quoted for compatibility, not new packet authority): The sole exception is the current packet's dispatch-capsule-authorized, one-per-claim local OpenCode weak-worker Provider invocation; it cannot make the controller read, pass, persist, or report a credential. This packet's external-effect limit is zero and does not use that exception.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- Treat ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures as bounded recovery transitions through their existing owners.
- Preserve exact receipt bindings and failed/unknown evidence; never convert absence, stale evidence, or an unproven external outcome to success.
- Emergency-stop: revert the current window and retain detailed lifecycle evidence. Authority, evaluator, recovery, and schema remain unchanged.

## Hard Stops

- no Provider call; no credential read, target write, release, deployment, automatic merge, EFFECT execution, or T3 action without its separate exact authority;
- no second controller, ledger, queue, lease, workflow owner, store, scheduler, evaluator, authority, or persistence owner;
- no future-route path/prose, model output, local checkpoint, or candidate PR accepted as current-main authority;
- no stale/ambiguous owner, caller, path, verification, rollback, cleanup, retention, evidence, schema, evaluator, authority, or recovery fact treated as proved;
- no retry of an outcome-unknown effect and no deletion or concealment of failure, pause, repair, or recovery evidence.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is the sole routing-only index. Promotion removes exactly one eligible packet, re-derives every `REFRESH_AT_PROMOTION` field from accepted main, validates the resulting candidate, and independently reviews the routing change. No future sketch, static path, or profile alone authorizes code or an effect.
