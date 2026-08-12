# Next Decision

Last updated: 2026-08-12.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, a Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

## Authoritative Forward Order

```text
[window: Route bootstrap resumption decision — DECISION_REQUIRED, planning-only and no execution authority]
→ [route bootstrap reconciliation — paused and blocked]
→ [route-autopilot adversarial soak — provider-free, blocked]
→ [PREFLIGHT B1/B2/provenance contract → bounded repair → provider-free PREFLIGHT]
→ [viability RUN — typed T3 pause]
→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-ROUTE-BOOTSTRAP-DECISION-1` — `DECISION_REQUIRED`. Sole current packet: a human planning/bootstrap decision must establish whether and how a fresh route attempt may start from the repaired accepted main. It grants no execution authority.
2. No route, weak-worker, soak, successor, Provider, target, EFFECT, or T3 action is active in this window.

## Blocked Successors

`PE7-ROUTE-AUTOMATION-1` bootstrap reconciliation remains paused and blocked until a fresh planning/bootstrap decision explicitly authorizes a new attempt from the repaired accepted main. The old route10 attempt is non-resumable evidence from an obsolete main, not a checkpoint. The first canonical provider-free soak and every later packet remain routing-only in `docs/FUTURE_ROUTE.md`; any later external-effect packet still pauses for an exact finite T3 receipt.

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

## Completed PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1

**Historical state:** `COMPLETE`.

**Historical evidence:** Authority PRs #406/#407/#409; implementation PR #408 exact head `4a2dcf42728ae53f7daaec73e15310e8b0d67b59`; merge `57a86c78c3f9611ce48c5bce249721af23db5532`; both review axes exact `PASS`; canonical workflow `31593460813`; correction workflow `31594277043`. The original #405 invalid records remain non-authorizing, production readback binds the actual head, and route remains paused.

## Packet PE7-ROUTE-BOOTSTRAP-DECISION-1

**State:** `DECISION_REQUIRED`

**Prerequisite:** `PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1` is COMPLETE through authority PRs #406/#407/#409, implementation PR #408 merge `57a86c78c3f9611ce48c5bce249721af23db5532`, canonical workflow `31593460813`, and #405 correction workflow `31594277043`. Route execution remains paused, and the route10 attempt remains non-resumable evidence from an obsolete main.

**Class:** `PLAN`

**Outcome:** Obtain a fresh human planning/bootstrap decision that either authorizes one newly bound provider-free route-bootstrap attempt from the then-current accepted main, revises its bounded contract, or keeps it stopped. This packet performs no route action itself.

**Allowed delta:** No implementation delta. A future explicit planning decision may replace this packet in `docs/NEXT_DECISION.md`, synchronize accepted truth in `docs/CURRENT_STATUS.md` if needed, and update only hard-coded canonical packet identity/state assertions in `tests/test_session_context.py`; no session-context behavior change is authorized.

**Exit:** A fresh human decision records an exact accepted-main binding, the finite provider-free bootstrap scope, ownership, verification, rollback, evidence destination, and stop conditions, or explicitly keeps route stopped.

**Stop:** Any attempt to infer route authority from this placeholder, resume route10, start a worker/soak/successor, touch PR #225, or perform Provider/credential/target/release/deployment/auto-merge/EFFECT/T3 action before a new accepted decision.

### Twelve-field contract

1. **Outcome and non-goals.** Decide only whether and how to establish a new provider-free route-bootstrap attempt. Non-goals: executing route/weak-worker work, starting soak/successors, or performing any external effect.
2. **Prerequisites and evidence.** Begin from freshly verified accepted main after the repair closeout, the accepted repair receipts above, the production #405 correction readback, emergency-stop state, and retained route10 evidence classified as non-resumable.
3. **Owners and paths.** `docs/NEXT_DECISION.md` remains the sole current-window owner; `docs/CURRENT_STATUS.md` owns accepted receipts; GitHub and Issue #383 remain existing durable owners. No code or data owner changes.
4. **Frozen invariants.** Route and weak workers stay stopped; route10 cannot be resumed; exact-head review, canonical CI, artifact scope, single-owner, manual-merge, and fail-closed controls remain mandatory.
5. **Only semantic delta.** None. This packet records a missing human planning decision and carries zero execution authority.
6. **Forbidden changes.** No implementation, route dispatch, worker start, soak/successor start, Provider/T3/EFFECT, credentials, target write, PR #225 activity, release, deployment, auto-merge, schema, evaluator, or authority change.
7. **Ordered implementation slices.** (a) refresh accepted main and retained evidence; (b) present bounded bootstrap options and consequences; (c) obtain an explicit human decision; (d) only then replace this packet with a separately accepted execution contract or continued stop.
8. **Failure, restart, idempotency, concurrency, and stop taxonomy.** Missing, stale, conflicting, or ambiguous bindings keep `DECISION_REQUIRED`; no checkpoint or local artifact can advance state; no external action is retried or inferred.
9. **Verification.** A future decision packet must pass handoff/security/diff checks, independent exact-head review, canonical exact-head CI, and any focused tests required by its actual scope before implementation.
10. **Compatibility, rollback, cleanup, and retention.** Revert only a future planning-document replacement if necessary; retain accepted repair/history/correction and route10 evidence. No destructive cleanup or migration is authorized.
11. **Exit artifact.** One explicit human planning decision bound to the then-current accepted main, with complete scope, owners, checks, rollback, evidence destination, and hard stops—or an explicit continued stop.
12. **Next permitted action and forbidden next actions.** The next permitted action is only that fresh planning/bootstrap decision. Do not resume route, reuse route10 as a checkpoint, start soak/successors, touch PR #225, or perform Provider/T3/EFFECT/credential/target/release/deploy/auto-merge actions.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["One explicit human planning/bootstrap decision or continued-stop decision"],"allowed_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","tests/test_session_context.py"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["One accepted planning decision bound to the then-current accepted main"],"external_effect_limit":0,"forbidden_changes":["Do not resume route or weak-worker execution.","Do not start soak or successors.","Do not call a Provider, read credentials, write a target, release, deploy, auto-merge, or touch PR #225.","Do not change session-context behavior from this planning-only packet."],"forbidden_next_actions":["Do not resume route bootstrap reconciliation or soak without a fresh accepted planning decision.","Do not treat route10 artifacts as a resumable checkpoint.","Do not infer execution authority from this DECISION_REQUIRED packet."],"goal":"Obtain a fresh human planning/bootstrap decision while route and weak-worker execution remain stopped.","known_store_mutations":[],"ordered_steps":["Refresh accepted main and retained repair/route10 evidence.","Present bounded route-bootstrap choices and consequences.","Obtain an explicit human decision before replacing this packet or starting any route action."],"packet_id":"PE7-ROUTE-BOOTSTRAP-DECISION-1","packet_state":"DECISION_REQUIRED","pause_gates":["Remain DECISION_REQUIRED on missing, stale, conflicting, or ambiguous bindings.","Stop before any route, Provider, credential, target, release, deployment, auto-merge, EFFECT, or T3 action."],"plan_lane_state":"plan_lane_active","private_paths_allowed":false,"prerequisite_receipts":["PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1 COMPLETE: authority PRs #406/#407/#409; implementation PR #408 merge 57a86c78c3f9611ce48c5bce249721af23db5532; canonical workflow 31593460813; correction workflow 31594277043"],"prerequisites":["PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1"],"read_paths":["START_HERE.md","AGENTS.md","docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md"],"rollback":"Revert only a future accepted planning-document replacement; retain accepted repair/history/correction and route10 evidence; keep route stopped.","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context","uv run --no-project python scripts/check_agent_handoff.py","uv run --no-project python tools/check_security_baseline.py","git diff --check"]}
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
