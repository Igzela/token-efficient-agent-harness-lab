# Next Decision

Last updated: 2026-08-14.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1` is accepted on main with its source-bound measurement ledger. The human value owner for `PE7-RWE-MR-CORPUS-SAMPLING-1` is Igzela, the repository owner; this session's implementation agent is explicitly delegated to record and execute the remaining T2 value judgments for this route. The current window is `PE7-RWE-MR-CORPUS-SAMPLING-1` `READY_FOR_EXECUTION` for a provider-free corpus and sampling contract only.

## Authoritative Forward Order

```text
[window: PE7-RWE-MR-CORPUS-SAMPLING-1 — READY_FOR_EXECUTION, delegated corpus and sampling freeze]

→ `PE7-RWE-MR-OPERATIONS-EVIDENCE-1` after this packet is merged and closed
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-MR-CORPUS-SAMPLING-1` — `READY_FOR_EXECUTION`

## Completed (PE7-RWE-V2-VIABILITY-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** Disposition `CONTROLLED_FAILURE`. Run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; cells `cell-rwe-minimum-t1-fix_flow_linkage-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t1-fix_flow_linkage-r2-bp-standard-s2026080602`, `cell-rwe-minimum-t2-draft_contract_tests-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t2-draft_contract_tests-r2-bp-standard-s2026080602`. Restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`. No seal; no target-default-branch write. Promotion PR #442 exact head `50e18540f40a8d47c384f2cac74683618f93c273`; merge `8c5c2f85bc5d66c08d730b7d0c69d914af19540c`; canonical workflow `31710478692`.

## Packet PE7-RWE-MR-CORPUS-SAMPLING-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-RWE-MR-ESTIMANDS-1 — COMPLETE on accepted main 4a0048fcb6785adfb3614769298519c95a01de2f; PR #444 exact head c3b61d1ecd898abfab910f0c2f5c33fa6692acef; merge 4a0048fcb6785adfb3614769298519c95a01de2f; exact-head PASS; canonical workflow 31765676789.

**Class:** `CONTRACT`

**Outcome:** Freeze the finite RWE task population, coverage strata, contamination screen, nested repetition rule, precision method, maximum experiment envelope, and no-replacement rule before any new outcome is observed.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md` only. No runtime, schema, migration, fixture, Provider, credential, target, evaluator, scheduler, store, or external-effect change.

**Exit:** An independently reviewed rwe_corpus_sampling.v1 manifest bound to the accepted v2 task/protocol/schedule hashes, with finite population, coverage, precision/sensitivity statement, maximum envelope, and replacement rules fixed; then promote exactly PE7-RWE-MR-OPERATIONS-EVIDENCE-1 only after this packet is merged and closed.

**Stop:** Any task identity/hash drift, missing required task evidence, contamination uncertainty, unavailable task coverage, budget/envelope conflict, or precision claim that cannot be defended from the frozen finite population.

### Twelve-field contract

1. **Outcome and non-goals.** The registered population is the accepted v2 minimum-first corpus: exactly rwe-minimum-t1-fix_flow_linkage (focused_bug_repair) and rwe-minimum-t2-draft_contract_tests (small_test_addition) from https://github.com/Igzela/alters-lab. This packet freezes selection; it does not add tasks, execute tasks, change the evaluator, or claim that this small census has broad statistical power.
2. **Prerequisites and evidence.** Accepted estimands are measurement_estimands.v1 on main 4a0048fcb6785adfb3614769298519c95a01de2f. Corpus rwe-minimum-first-corpus-v2 is bound to its declared artifact hashes: corpus sha256 044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20, protocol sha256 bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db, and schedule sha256 6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38. These are the canonical bindings declared by the accepted RWE artifacts, not raw JSON-file byte hashes. The source commit is 6240768506320a324d68787b9eaa86971c8c930c and source tree hash is 137e912f416a3a8d5be307e91bb2580154fc8fc34c6de52c2441ef3e3f93a064. Task-definition hashes are fcd13b6f7a970c048fd09e1f723a315b8e03d221cad1555bf694ca95115438f8 and f49e374d8b818d9e2cf4566d6fb3323c472a3dd449ebc71413eab96891124e7d in task order.
3. **Owners and paths.** Existing owners remain authoritative: engine/src/rwe/operator_corpus.rs, engine/src/rwe/corpus.rs, engine/src/rwe/economic_protocol.rs, engine/src/rwe/execution_schedule.rs, the versioned engine/rwe/corpora/ artifacts, and LocalProductStore evidence owners. This packet edits only the four allowed documentation paths and creates no parallel corpus, evaluator, budget, or persistence owner.
4. **Frozen invariants.** The inferential unit is task; each task has minimum_repetitions_per_task=2; the registered population is a finite census of two tasks and four scheduled cells. Task identity, source commit/tree, task-definition hash, protocol/corpus/schedule hashes, seeds 2026080601 and 2026080602, and allowed paths are immutable after this contract is accepted.
5. **Only semantic delta.** Coverage is one repository (alters-lab), one implementation language family (Python, as bound by the task verification commands), and two task-family strata: focused bug repair and small test addition. Both strata must be present; no post-outcome task addition or favorable-task substitution is allowed.
6. **Forbidden changes.** Do not use a task whose source or definition hash drifts, whose required verification is unavailable, whose objective or source was exposed to candidate-generated output, or whose raw prompt/output/transcript would need to be retained. Do not treat fixtures, controlled-failure direction, or a worker-recomputable hash as managed acceptance or authenticity.
7. **Ordered implementation slices.** Bind the accepted estimand and v2 artifacts; record rwe_corpus_sampling.v1; verify both task identities, hashes, strata, repetitions, and schedule envelope; run provider-free handoff checks; close this packet in its governed Draft PR; then promote the operations/evidence successor.
8. **Failure, recovery, and stop taxonomy.** Missing, unknown, contaminated, or drifted task evidence is unavailable and fails closed; it is not imputed, dropped, retried, or replaced after outcomes begin. Preserve restricted evidence and existing store receipts; rollback is a documentation revert with evidence retained. A coverage or precision failure remains a bounded DECISION_REQUIRED/insufficient disposition rather than an invented success.
9. **Verification.** Run PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context.CheckpointTests.test_current_repository_packet_binds_safe_live_capsule, uv run --no-project python scripts/check_agent_handoff.py, and git diff --check; also validate the declared artifact hash fields, task-definition hashes, source commit/tree identity, four schedule cells, and budget envelope against accepted main.
10. **Compatibility, rollback, and retention.** No executable behavior changes. Revert the single documentation commit to restore the prior current window; retain all v2 controlled-failure, estimand, corpus, and schedule evidence and never delete or rewrite it.
11. **Exit artifact.** rwe_corpus_sampling.v1 binds N_task=2, N_repetition=2, N_cell=4, exact task/hash identity, paired bootstrap 95% inherited from paired-bootstrap-95, and no model-based power claim. Precision is reported by the task-level paired bootstrap interval; leave-one-task-out sensitivity is descriptive only, and any unresolved interval is reported as insufficient rather than repaired by post-outcome expansion. The maximum registered envelope is four cells, at most 12 provider requests, 80,768 total tokens, 3,600,000 ms wall time, and the accepted operator ceiling USD 0.80.
12. **Next action.** Keep the PR Draft while changing; obtain stable-head two-axis PASS, mark Ready once, wait for exact-head canonical CI, manually squash-merge, record the closeout receipt, refresh main, and only then promote PE7-RWE-MR-OPERATIONS-EVIDENCE-1.

### Corpus-selection record — rwe_corpus_sampling.v1

| Stratum | Registered task | Selection rule | Replacement rule |
|---|---|---|---|
| focused_bug_repair | rwe-minimum-t1-fix_flow_linkage | Include only with source commit/tree and task-definition hash above; required pytest command and bounded mutable paths must remain available | No replacement is permitted; if unavailable, retain unavailable and stop |
| small_test_addition | rwe-minimum-t2-draft_contract_tests | Include only with source commit/tree and task-definition hash above; required pytest command and bounded mutable paths must remain available | No replacement is permitted; if unavailable, retain unavailable and stop |

Contamination screening is pre-outcome and hash-bound: the source commit/tree and task definitions must match the manifest, task selection must precede candidate outcomes, candidate output cannot amend the task/protocol/schedule, and raw prompts/outputs/transcripts are not retained. Any failed screen makes the task unavailable; it does not authorize a substitute before or after unblinding.

The finite-census precision statement is deliberately modest. The two registered tasks are the complete population for this packet, with two nested repetitions each. The primary analysis uses the already frozen task-level paired bootstrap 95% interval and the estimand margins; the pre-registered leave-one-task-out sensitivity is descriptive and cannot turn an unavailable or unresolved result into a pass. No additional minimum meaningful effect, post-outcome expansion, or favorable-task replacement is introduced.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-RWE-MR-CORPUS-SAMPLING-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"opencode_local_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Freeze the finite v2 RWE corpus and sampling manifest without executing a task or effect.","rollback":"Revert the single documentation commit to restore the prior current window while retaining all v2 and estimand evidence.","allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md"],"allowed_outputs":["The versioned rwe_corpus_sampling.v1 manifest and its bounded closeout receipt in docs/NEXT_DECISION.md."],"prerequisites":["PE7-RWE-MR-ESTIMANDS-1"],"prerequisite_receipts":["PR #444 exact head `c3b61d1ecd898abfab910f0c2f5c33fa6692acef`; merge `4a0048fcb6785adfb3614769298519c95a01de2f`; exact-head `PASS`; canonical workflow `31765676789`; exact-head check `31765676776`"],"forbidden_changes":["Any runtime, schema, migration, fixture, Provider, credential, target, evaluator, scheduler, store, or external-effect change.","Do not add tasks, alter hashes, tune selection after outcomes, or claim power not supported by the finite census."],"forbidden_next_actions":["Do not call a Provider, read credentials, execute a task, or rerun run-live-20260813-v2c.","Do not write a target default branch, execute EFFECT/T3, release, deploy, or merge automatically.","Do not start PE7-RWE-MR-OPERATIONS-EVIDENCE-1 before this packet is merged and closed."],"ordered_steps":["Read the accepted estimand, corpus, protocol, schedule, and current owner documents.","Record and hash-bind rwe_corpus_sampling.v1.","Run provider-free handoff checks and prepare the governed Draft PR."],"read_paths":["docs/NEXT_DECISION.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","engine/src/rwe/operator_corpus.rs","engine/src/rwe/corpus.rs","engine/src/rwe/economic_protocol.rs","engine/src/rwe/execution_schedule.rs","engine/rwe/corpora/rwe-minimum-first-corpus/v2/protocol/rwe_economic_protocol.v1.json","engine/rwe/corpora/rwe-minimum-first-corpus/v2/schedule/execution_schedule.v1.json","engine/rwe/corpora/rwe-minimum-first-corpus/v2/tasks/rwe-minimum-t1-fix_flow_linkage.json","engine/rwe/corpora/rwe-minimum-first-corpus/v2/tasks/rwe-minimum-t2-draft_contract_tests.json"],"verification":["PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context.CheckpointTests.test_current_repository_packet_binds_safe_live_capsule","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"expected_artifacts":["Versioned rwe_corpus_sampling.v1 manifest in docs/NEXT_DECISION.md.","Exact task, protocol, corpus, schedule, and closeout bindings."],"pause_gates":["Stop before any Provider call, credential access, task execution, target write, EFFECT, T3 action, release, deployment, or automatic merge.","Stop if any task/hash/coverage/contamination/precision fact cannot be re-proved from accepted main."]}
-->

## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Existing route boundary (quoted for compatibility, not new packet authority): The sole exception is the current packet's dispatch-capsule-authorized, one-per-claim local OpenCode weak-worker Provider invocation; it cannot make the controller read, pass, persist, or report a credential. This packet's external-effect limit is zero and does not use that exception.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- Treat ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures as bounded recovery transitions through their existing owners.
- Preserve exact receipt bindings and failed/unknown evidence; never convert absence, stale evidence, or an unproven external outcome to success.
- Emergency-stop: revert the current window and retain detailed lifecycle evidence. authority, evaluator, recovery, and schema remain unchanged.

## Hard Stops

- no Provider call; no credential read, target write, release, deployment, automatic merge, EFFECT execution, or T3 action without its separate exact authority;
- no second controller, ledger, queue, lease, workflow owner, store, scheduler, evaluator, authority, or persistence owner;
- no future-route path/prose, model output, local checkpoint, or candidate PR accepted as current-main authority;
- no stale/ambiguous owner, caller, path, verification, rollback, cleanup, retention, evidence, schema, evaluator, authority, or recovery fact treated as proved;
- no retry of an outcome-unknown effect and no deletion or concealment of failure, pause, repair, or recovery evidence.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is the sole routing-only index. Promotion removes exactly one eligible packet, re-derives every `REFRESH_AT_PROMOTION` field from accepted main, validates the resulting candidate, and independently reviews the routing change. No future sketch, static path, or profile alone authorizes code or an effect.
