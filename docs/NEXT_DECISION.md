# Next Decision

Last updated: 2026-08-12.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, a Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

## Authoritative Forward Order

```text
[window: Control binding integrity repair — READY_FOR_EXECUTION, provider-free]
→ [route bootstrap reconciliation — blocked until repair closeout]
→ [route-autopilot adversarial soak — provider-free, blocked]
→ [PREFLIGHT B1/B2/provenance contract → bounded repair → provider-free PREFLIGHT]
→ [viability RUN — typed T3 pause]
→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1` — `READY_FOR_EXECUTION`. Sole current packet: repair exact-head review binding, append-only review correction/readback, and deterministic plan-artifact scope enforcement. It creates no route, Provider, target, EFFECT, or T3 effect.
2. No route, weak-worker, soak, successor, Provider, target, EFFECT, or T3 action is active in this window.

## Blocked Successors

`PE7-ROUTE-AUTOMATION-1` bootstrap reconciliation is blocked until this repair has merged, received canonical closeout, and the old route10 attempt has been retained only as non-resumable evidence from an obsolete main. The first canonical provider-free soak and every later packet remain routing-only in `docs/FUTURE_ROUTE.md`; any later external-effect packet still pauses for an exact finite T3 receipt.

## Packet States

- `READY_FOR_EXECUTION` — an accepted, complete twelve-field contract permits only its stated provider-free work.
- `BLOCKED_PREREQUISITE` — a named receipt, contract, or authority is missing.
- `DECISION_REQUIRED` — current-main evidence cannot prove the required owner, path, decision, or recovery fact.
- `T3_REQUIRED` — the route has completed bounded provider-free preparation and waits for one exact source-authoritative T3 receipt; no effect occurred.
- `OUTCOME_UNKNOWN` — an external effect may have occurred and the route must not retry or infer success.
- `ROUTE_EXHAUSTED` — the canonical inventory has no remaining successor after a proved closeout.
- `IN_PROGRESS` — the existing ledger/PR lifecycle owns a current attempt.
- `COMPLETE` — merged, exact-head verified, independently reviewed, and synchronized into accepted documents.

Exact-head review `PASS`, a PR merge, and packet `COMPLETE` are different facts.

## Promotion and Continuous-Route Invariants

The deterministic promotion compiler owns only stable route facts: packet identity, prerequisite graph, class, tier, risk profile, checked manifest integrity, and the accepted predecessor receipt/disposition. `docs/FUTURE_ROUTE.md` paths and prose are hints only; they are never current-main edit authority.

The `RoutePromotionPlanner` module resolves `REFRESH_AT_PROMOTION` from the actual accepted main before a candidate can be compiled: exact owner and caller/consumer evidence, allowed-path closure, ordered slices, precise allowlisted verification, rollback/cleanup/retention/evidence destinations, and relevant schema/evaluator/authority/recovery decisions. It emits a bounded candidate with hashes for every observation or a typed `DECISION_REQUIRED`; it may not emit a plausible generic contract. Deterministic validators reject a candidate with missing, stale, contradictory, out-of-closure, or unverified facts.

At a T3 boundary, a conclusion from an allowlisted decision source — `human_operator`, `local_sol_5_6_max`, or `gpt_web` — is authoritative only for the exact finite disposition already prepared by the accepted route request. The authenticated controller actor is the receipt transport, not necessarily the decision source. Local Sol 5.6 Max is an explicit escalation for genuinely difficult provider-free design, planning, or documentation questions, not a routine reviewer or automatic `route-run` step; GPT web evidence must likewise be supplied as a hash-bound, finite conclusion. The retention-safe `decision_evidence_digest` commits that conclusion without storing its raw prompt/output; the controller, not its caller, derives `decision_digest` from that digest, source, exact request binding, and disposition. A source conclusion cannot dispatch or execute the EFFECT, read credentials, merge, mint or widen authority, create a route node, or write a ledger receipt itself. Material route reordering or branching, a new architecture/schema/evaluator/authority/recovery decision, a new effect scope, or an unexpected condition remains `DECISION_REQUIRED` for the human/planning process; no model conclusion is authority for that change. Before ledger mutation or resume, the existing authenticated transport must prove the current request binding, the authority-owner digest, an allowlisted source identifier, the source-evidence digest, and the recomputable decision binding. A redacted outcome digest alone is not outcome provenance: the named existing owner must independently prove its evidence in the routed CLOSEOUT packet. Inability to prove a required current binding leaves the route at `T3_REQUIRED` or `DECISION_REQUIRED`, while a possibly executed but unproved effect is `OUTCOME_UNKNOWN`.

`NEXT_DECISION.md` retains only this current packet and, during a transition, one short completed-predecessor binding. Promotion replaces that binding rather than accumulating history. The driver must prove a bounded document size and stable one-current-window shape across the checked inventory manifest and the canonical route-control additions.

## Completed PE7-ROUTE-AUTOMATION-1

**Historical state:** `COMPLETE`.

**Historical evidence:** PR #390 exact head `24618e52c969adc93e7bc092c51dde6b2d0ffea9`; merge `5481053c736e7db8481cabd9316741f2a5cd6c7a`; exact-head `PASS`; canonical workflow `31467821768`. Its route implementation remains accepted, but bootstrap reconciliation and every successor stay paused behind the current repair.

## Packet PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-ROUTE-AUTOMATION-1` is COMPLETE, but route execution is paused. PR #405 merged from actual head `e68ec0b3a7b78d3ca241922bf3995c2f3ba4ecfa` after an invalid review receipt recorded nonexistent head `e68ec0b3e84d5b0d5526cc3a114e2a3d0c0a0de5`; the original record is retained as non-authorizing evidence. A fresh post-merge review of the complete actual range returned exact `PASS`, without retroactively making that merge compliant. The existing Plan Execution Ledger Issue #383 and its lifecycle owners remain the sole durable owner.

**Class:** `IMPLEMENT`

**Outcome:** Fail closed before review dispatch, diff acquisition, ledger mutation, or receipt publication unless a full caller-provided expected head exactly equals the controller-resolved live PR head and both live commit identities exist; add append-only correction/supersession readback for invalid review evidence; and reject plan artifacts whose manifest paths escape the claim/candidate-bound `allowed_paths` before any reset, apply, commit, push, PR, or handoff action.

**Allowed delta:** `.github/workflows/agent-review.yml`; `scripts/agent-control/{artifact_contract,dispatcher,local_run_once,plan_lifecycle,pr_binding,prompt_builder,state_manager,validate_review}.py`; `scripts/agent-control/review_schema.json`; `tests/{test_agent_control_state,test_agent_local_loop,test_agent_orchestrator_artifacts,test_agent_orchestrator_repairs,test_agent_review_finalization,test_session_context}.py`; `tools/{test_check_agent_handoff,test_prompt_builder}.py`; and the smallest `docs/CURRENT_STATUS.md` / `docs/NEXT_DECISION.md` authority and closeout synchronization. `tests/test_session_context.py` is authorized only to replace the stale hard-coded route packet identity with this already-accepted repair packet identity so the current-entry contract verifies the accepted frontier; no session-context behavior change is authorized. No other production or test path is authorized.

**Exit:** The original invalid #405 records remain visible and non-authorizing; an additive supersession record binds the actual head and production readback selects it as the current valid review state; all review producers and consumers reject abbreviated, nonexistent, stale, mismatched, or unavailable live bindings; and plan-scope violations return stable `plan_scope_violation` evidence with zero commit, push, PR, or handoff effects.

**Stop:** A replacement ledger/schema owner, destructive history rewrite, changed review-convergence meaning, new persistence owner, widened route authority, schema migration beyond an additive backward-compatible correction record, unprovable live PR identity, Provider/credential/target action, release/deployment, auto-merge, EFFECT/T3 execution, or any attempt to resume route/weak-worker execution.

### Twelve-field contract

1. **Outcome and non-goals.** Repair only review exact-head/correction binding and plan-artifact allowed-path enforcement. Non-goals: route execution, soak, Provider/T3/EFFECT work, credentials, target writes, product runtime/schema/evaluator changes, new persistence owners, release/deploy, auto-merge, or PR #225 activity.
2. **Prerequisites and evidence.** Bind implementation to the accepted authority merge. Preserve #405 base `87ea67c39ba1ddb6e4cd35b7b86513500a25c325`, actual head `e68ec0b3a7b78d3ca241922bf3995c2f3ba4ecfa`, invalid recorded head `e68ec0b3e84d5b0d5526cc3a114e2a3d0c0a0de5`, original comments, and fresh retrospective exact `PASS` evidence. Preserve route10 logs, captures, temporary artifacts, and termination evidence without treating them as a checkpoint.
3. **Owners and paths.** `state_manager.py` and the existing Issue #383 comments remain the sole review-state persistence/readback owner; `pr_binding.py` may provide the shared live PR identity primitive; `validate_review.py`, `dispatcher.py`, `prompt_builder.py`, and `agent-review.yml` may consume only that binding; `artifact_contract.py` owns canonical path normalization/containment; `local_run_once.py` invokes it for the plan chain. Changes are limited to the Allowed delta above.
4. **Frozen invariants.** A worker claim is never authority. Only controller-derived full live base/head identities may bind review state. Existing invalid history is append-only and non-authorizing. Artifact validation precedes scope validation; scope validation precedes every worktree mutation or GitHub write. Existing route, ledger, workflow, store, merge, CI, and handoff owners remain singular.
5. **Only semantic delta.** Require a full 40-hex expected head to equal the live `headRefOid`, verify referenced commit objects, optionally bind the live base where the path needs it, store controller-derived identities rather than worker echo, add backward-compatible correction/supersession state and production readback, and apply the canonical artifact scope helper to plan manifests with typed `plan_scope_violation` failure.
6. **Forbidden changes.** No prefix/short-SHA authorization, caller metadata fallback, overwrite/delete of old review comments, ordinary R2 fabricated as correction, second path algorithm, prompt-only scope control, route resume, external effect, changed review verdict meaning, new ledger/store/schema owner, or consumer bypass.
7. **Ordered implementation slices.** (a) centralize strict live PR/commit binding and enforce it before review diff/prompt/dispatch/write paths; (b) distrust `reviewed_head_sha`, record controller-derived bindings, and guard every production consumer; (c) add append-only correction/supersession schema, writer, and readback selection in the existing owner; (d) add the reusable artifact scope helper and plan-chain pre-mutation gate; (e) add focused positive and negative tests; (f) write and read back the #405 Issue correction after accepted implementation.
8. **Failure, restart, idempotency, concurrency, and stop taxonomy.** Exact mismatch, abbreviated/nonexistent/stale head, unavailable metadata, conflicting base, invalid supersession target, or plan scope escape fail closed before effects with stable bounded diagnostics. Correction writes are idempotent and append-only. A replacement implementation head invalidates review. Any owner/schema/semantic expansion beyond this contract is `DECISION_REQUIRED`; independent safe work continues while route remains stopped.
9. **Verification.** Tests cover exact match, mismatch, nonexistent, abbreviated, stale, metadata unavailable, untrusted reviewer echo, consumer guards, correction/supersession readback, legal file, directory allowed path, repository-supported outside-scope rejection, and proof that rejection performs no reset/apply/commit/push/PR/handoff. Run focused control-state/review/artifact/prompt/local-loop tests, applicable route/handoff tests, security baseline, handoff check, and `git diff --check`; then complete-diff independent exact-head `PASS` and canonical exact-head CI.
10. **Compatibility, rollback, cleanup, and retention.** Existing valid review comments continue to read unchanged; old invalid comments remain visible but cannot authorize after a valid supersession. Revert the implementation to restore prior code while retaining all correction/history and route10 evidence. No migration, destructive cleanup, credential retention, raw prompt/output retention, or private-path publication is allowed.
11. **Exit artifact.** Separate accepted authority, implementation, and closeout PRs; exact base/head/review/canonical-CI/merge receipts; #405 PR correction URL; Issue #383 correction URL and production readback; complete changed-path and command evidence; zero-effect plan-scope negative proof; preserved route10 evidence location and explicit stopped state.
12. **Next permitted action and forbidden next actions.** From the authority merge, implement and close out only this repair. After closeout, the next permitted action is a fresh planning/bootstrap decision from the new accepted main. Do not resume route, reuse route10 as a checkpoint, start soak/successors, touch #225, or perform Provider/T3/EFFECT/credential/target/release/deploy/auto-merge actions.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["Exact-head review binding and correction/readback evidence","Typed plan_scope_violation diagnostics with zero downstream effects","Authority, implementation, and closeout receipts"],"allowed_paths":[".github/workflows/agent-review.yml","docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","scripts/agent-control/artifact_contract.py","scripts/agent-control/dispatcher.py","scripts/agent-control/local_run_once.py","scripts/agent-control/plan_lifecycle.py","scripts/agent-control/pr_binding.py","scripts/agent-control/prompt_builder.py","scripts/agent-control/review_schema.json","scripts/agent-control/state_manager.py","scripts/agent-control/validate_review.py","tests/test_agent_control_state.py","tests/test_agent_local_loop.py","tests/test_agent_orchestrator_artifacts.py","tests/test_agent_orchestrator_repairs.py","tests/test_agent_review_finalization.py","tests/test_session_context.py","tools/test_check_agent_handoff.py","tools/test_prompt_builder.py"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["Accepted authority PR","Accepted focused implementation PR","Append-only #405 correction with production readback","Accepted closeout PR"],"external_effect_limit":0,"forbidden_changes":["Do not resume route or weak-worker execution.","Do not create a second ledger, store, schema owner, controller, queue, scheduler, evaluator, or path algorithm.","Do not overwrite historical review evidence or change review-convergence meaning.","Do not call a Provider, read credentials, write a target, release, deploy, auto-merge, or touch PR #225."],"forbidden_next_actions":["Do not resume route bootstrap reconciliation or soak after closeout without a fresh planning decision.","Do not treat route10 artifacts as a resumable checkpoint.","Do not authorize from short, stale, nonexistent, mismatched, worker-claimed, or unavailable PR metadata."],"goal":"Repair review exact-head and correction binding plus deterministic plan-artifact scope enforcement before any route resume.","known_store_mutations":["Append-only review correction comment on the existing Plan Execution Ledger Issue #383"],"ordered_steps":["Accept this planning authority independently.","Implement strict controller-derived live PR binding and additive correction/readback.","Implement canonical plan-artifact scope enforcement before mutations or GitHub writes.","Run focused/full verification and independent exact-head review.","Run canonical CI and manually squash merge.","Write/read back #405 correction and accept a minimal closeout while route remains stopped."],"packet_id":"PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop on a required new persistence/schema owner or changed review semantics.","Stop on unprovable live PR metadata or unresolved independent-review finding.","Stop before any route, Provider, credential, target, release, deployment, auto-merge, EFFECT, or T3 action."],"plan_lane_state":"plan_lane_active","private_paths_allowed":false,"prerequisite_receipts":["PE7-ROUTE-AUTOMATION-1 COMPLETE: PR #390 exact head 24618e52c969adc93e7bc092c51dde6b2d0ffea9; merge 5481053c736e7db8481cabd9316741f2a5cd6c7a; canonical workflow 31467821768","PR #405 retrospective review PASS bound to actual head e68ec0b3a7b78d3ca241922bf3995c2f3ba4ecfa; historical invalid receipt remains non-authorizing"],"prerequisites":["PE7-ROUTE-AUTOMATION-1"],"read_paths":["START_HERE.md","AGENTS.md","docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/MODULE_MAP.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","scripts/agent-control/"],"rollback":"Revert the repair code and canonical synchronization while retaining append-only historical and correction evidence; keep route stopped.","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["PYTHONPATH=src uv run --no-project python -m unittest tests.test_agent_control_state tests.test_agent_orchestrator_artifacts tests.test_agent_orchestrator_repairs tests.test_agent_review_finalization tests.test_agent_local_loop tools.test_prompt_builder tools.test_check_agent_handoff","PYTHONPATH=src uv run --no-project python -m unittest tests.test_agent_route_driver tests.test_agent_plan_lifecycle tests.test_agent_plan_lane tests.test_agent_plan_promotion","uv run --no-project python scripts/check_agent_handoff.py","uv run --no-project python tools/check_security_baseline.py","git diff --check"]}
-->

## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- Treat ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures as bounded recovery transitions through their existing owners.
- Preserve exact receipt bindings and failed/unknown evidence; never convert absence, stale evidence, or an unproven external outcome to success.

## Hard Stops

- no Provider call, credential read, target write, release, deployment, automatic merge, EFFECT execution, or T3 action without its separate exact authority;
- no second controller, ledger, queue, lease, workflow owner, store, scheduler, evaluator, authority, or persistence owner;
- no future-route path/prose, model output, local checkpoint, or candidate PR accepted as current-main authority;
- no stale/ambiguous owner, caller, path, verification, rollback, cleanup, retention, evidence, schema, evaluator, authority, or recovery fact treated as proved;
- no retry of an outcome-unknown effect and no deletion or concealment of failure, pause, repair, or recovery evidence.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is the sole routing-only index. Promotion removes exactly one eligible packet, re-derives every `REFRESH_AT_PROMOTION` field from accepted main, validates the resulting candidate, and independently reviews the routing change. No future sketch, static path, or profile alone authorizes code or an effect.
