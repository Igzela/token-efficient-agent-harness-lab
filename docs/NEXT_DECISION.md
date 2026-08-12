# Next Decision

Last updated: 2026-08-12.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, a Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

## Authoritative Forward Order

```text
[window: Route automation bootstrap reconciliation — READY_FOR_EXECUTION, provider-free]
→ [route-autopilot adversarial soak — provider-free]
→ [PREFLIGHT B1/B2/provenance contract → bounded repair → provider-free PREFLIGHT]
→ [viability RUN — typed T3 pause]
→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-ROUTE-AUTOMATION-1` — `READY_FOR_EXECUTION`. Sole current packet: reconcile the accepted merge-backed route receipt into one new attempt from the repaired accepted main, then let the existing route own provider-free implementation, verification, PR, closeout, and successor promotion.
2. The first canonical provider-free soak and every later packet remain routing-only until the existing promotion planner proves and accepts one exact current-main successor contract.
3. Provider, credential, target, release, deployment, automatic merge, EFFECT execution, and T3 action remain forbidden.

<!-- route-bootstrap-reconcile:v1 packet_id=PE7-ROUTE-AUTOMATION-1 -->

## Blocked Successors

The fresh human planning decision on 2026-08-12 authorizes one newly bound provider-free bootstrap from decision baseline `72c196aa03a5632bfbd47ba5f19cbc51a154889c`. The route must refresh the accepted post-merge main before every transition and may continue only through independently accepted provider-free successor contracts. The old route10 attempt remains non-resumable evidence from an obsolete main, not a checkpoint. Any external-effect packet still pauses for a separate exact finite T3 receipt.

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

## Packet PE7-ROUTE-AUTOMATION-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1` and `PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1` are COMPLETE, and the status owner proves the accepted merge-backed route receipt used only by the bootstrap bridge. The fresh human GO is bound to accepted decision baseline `72c196aa03a5632bfbd47ba5f19cbc51a154889c`; the controller must derive the post-merge accepted main live before dispatch. Binding-repair evidence remains authority PRs #406/#407/#409, implementation PR #408 merge `57a86c78c3f9611ce48c5bce249721af23db5532`, canonical workflow `31593460813`, and #405 correction workflow `31594277043`. SQLite race-repair evidence is PR #413 exact head `fc8c005981d2fa12f0f494a131b839d65a46a8ba`, canonical workflow `31611860646`, and merge `9cc118fa72d9d13a24cdf968cc5fc20dbe80b28f`. Route10 remains non-resumable evidence only.

**Class:** `CLOSEOUT`

**Outcome:** Reconcile the accepted merge-backed route receipt into one fresh route attempt and prove that weak agents can stably implement accepted provider-free packets end to end: modify code and documents within candidate-bound scope, test, publish a Draft PR, repair ordinary failures, obtain stable exact-head review and canonical CI, wait for governed manual squash merge, close out, synchronize canonical documents, and promote the next independently validated contract without manual successor authoring.

**Allowed delta:** Bootstrap itself may mutate only the existing Plan Execution Ledger and route-owned bounded evidence through the accepted controller. Each successor may change only its independently validated candidate-bound `allowed_paths`. The accepted planning transition that created this window was limited to `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and hard-coded canonical packet/state assertions in `tests/test_session_context.py`; it changed no runtime behavior.

**Exit:** A new bootstrap attempt is bound to the live accepted post-merge main; the canonical adversarial soak proves clean multi-packet traversal and ordinary failure recovery; and at least one routed provider-free implementation packet demonstrates code/document changes, tests, Draft PR, exact-head review/CI, manual merge readback, closeout, and successor promotion without a planning agent manually authoring the successor.

**Stop:** Any stale or conflicting main/receipt/head/scope binding; a second owner; an unproved architecture/schema/evaluator/authority/recovery choice; failure to prove stable weak-agent traversal; route exhaustion; `DECISION_REQUIRED`; `T3_REQUIRED`; `OUTCOME_UNKNOWN`; unrecoverable infrastructure failure; PR #225 activity; or any Provider/credential/target/release/deployment/auto-merge/EFFECT/T3 action.

### Twelve-field contract

1. **Outcome and non-goals.** Start one fresh provider-free route and continue until weak-agent end-to-end implementation and document synchronization are stable under the canonical soak and a real implementation packet. Non-goals: Provider calls, credentials, targets, releases, deployments, automatic merge, EFFECT execution, T3 action, product-runtime authority, or completing future packets whose own accepted prerequisites are absent.
2. **Prerequisites and evidence.** Begin from accepted decision baseline `72c196aa03a5632bfbd47ba5f19cbc51a154889c`, refresh the post-merge accepted main live, prove the accepted route, control-binding repair, and SQLite race-repair receipts above, read back the #405 correction, and retain route10 only as non-resumable evidence.
3. **Owners and paths.** `docs/NEXT_DECISION.md` remains the sole current-window owner; `docs/CURRENT_STATUS.md` owns accepted receipts; GitHub and Issue #383 remain existing durable owners. No code or data owner changes.
4. **Frozen invariants.** Route10 cannot be resumed; exact-head review, canonical CI, artifact scope, single-owner, manual merge, accepted-predecessor ordering, and fail-closed controls remain mandatory. A model result or future-route sketch never creates authority.
5. **Only semantic delta.** Replace the planning-only stop with one accepted bootstrap marker. The already accepted route implementation and lifecycle owners are unchanged.
6. **Forbidden changes.** No second owner, unreviewed successor, caller-selected packet bypass, Provider/T3/EFFECT, credentials, target write, PR #225 activity, release, deployment, auto-merge, schema, evaluator, product-runtime authority, or unproved route reordering.
7. **Ordered implementation slices.** (a) refresh the live accepted main and accepted control receipts; (b) remove emergency stop and enable only the orchestrator while auto-merge remains disabled; (c) start a new `route-run` from accepted main; (d) bootstrap the merge-backed route receipt; (e) promote and execute the provider-free adversarial soak; (f) continue through independently accepted provider-free contracts until the soak and one real implementation packet prove the full weak-agent lifecycle; (g) stop at the first typed terminal or authority boundary.
8. **Failure, restart, idempotency, concurrency, and stop taxonomy.** Ordinary worker, test, CI, review, checkpoint, duplicate, restart, and main-drift failures use existing bounded recovery owners. Missing, stale, conflicting, or ambiguous bindings yield `DECISION_REQUIRED`; a possibly executed external effect yields `OUTCOME_UNKNOWN` and is never retried.
9. **Verification.** Prove this window is read from the live accepted main before bootstrap. The route must then produce the soak's focused evidence plus the focused/full checks, independent exact-head review, canonical CI, manual-merge readback, and closeout evidence required by every promoted packet.
10. **Compatibility, rollback, cleanup, and retention.** Emergency-stop the route and disable orchestration to halt new transitions; retain ledger, review, CI, failure, and route10 evidence. Revert this planning authority if necessary. Do not delete or reinterpret accepted history.
11. **Exit artifact.** Controller-owned bootstrap, promotion, implementation, verification, merge-readback, closeout, and successor receipts; an independently accepted adversarial-soak report; and one real provider-free implementation packet completed by the weak-agent chain with synchronized canonical documents.
12. **Next permitted action and forbidden next actions.** Refresh the accepted main, verify the current control receipts, and start one fresh provider-free route; supervise it until the stated stability evidence exists or a typed hard stop occurs. Do not resume route10, touch PR #225, auto-merge, or perform Provider/T3/EFFECT/credential/target/release/deploy actions.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["Controller-owned bootstrap and route receipts","Provider-free implementation, verification, PR, review, CI, manual-merge readback, closeout, and successor evidence","One accepted adversarial-soak report and one completed real implementation packet"],"allowed_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","tests/test_session_context.py"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["One new bootstrap attempt bound to the live post-merge accepted main","Stable weak-agent full-lifecycle evidence through the canonical soak and one real provider-free implementation packet"],"external_effect_limit":0,"forbidden_changes":["Do not resume route10 or use it as a checkpoint.","Do not call a Provider, read credentials, write a target, release, deploy, auto-merge, execute an EFFECT/T3 action, or touch PR #225.","Do not create a second lifecycle owner or let unreviewed model output create authority."],"forbidden_next_actions":["Do not execute or skip an EFFECT/T3 packet.","Do not treat stale, conflicting, missing, failed, or outcome-unknown evidence as success.","Do not start a successor before its exact candidate is independently accepted."],"goal":"Prove weak agents can stably execute accepted provider-free plans end to end, including implementation and canonical document synchronization.","known_store_mutations":["Existing Plan Execution Ledger Issue #383 route and lifecycle receipts only"],"ordered_steps":["Refresh and bind the live post-merge accepted main.","Reconcile the accepted PE7-ROUTE-AUTOMATION-1 receipt into one new bootstrap attempt.","Promote and execute the canonical provider-free adversarial soak through existing owners.","Continue through independently accepted provider-free contracts until one real implementation packet completes the full lifecycle.","Stop at the first typed terminal or authority boundary."],"packet_id":"PE7-ROUTE-AUTOMATION-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop on missing, stale, conflicting, ambiguous, out-of-scope, or unproved bindings.","Stop before Provider, credential, target, release, deployment, automatic merge, EFFECT, or T3 action.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting."],"plan_lane_state":"plan_lane_active","private_paths_allowed":false,"prerequisite_receipts":["PE7-ROUTE-AUTOMATION-1 COMPLETE: PR #390 exact head 24618e52c969adc93e7bc092c51dde6b2d0ffea9; merge 5481053c736e7db8481cabd9316741f2a5cd6c7a; canonical workflow 31467821768","PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1 COMPLETE: authority PRs #406/#407/#409; implementation PR #408 merge 57a86c78c3f9611ce48c5bce249721af23db5532; canonical workflow 31593460813; correction workflow 31594277043","PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1 COMPLETE: PR #413 exact head fc8c005981d2fa12f0f494a131b839d65a46a8ba; canonical workflow 31611860646; merge 9cc118fa72d9d13a24cdf968cc5fc20dbe80b28f"],"prerequisites":["PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1","PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1"],"read_paths":["START_HERE.md","AGENTS.md","docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/ARCHITECTURE_BOOK.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","scripts/agent-control/"],"rollback":"Emergency-stop and disable orchestration; revert this accepted bootstrap authority if necessary; retain all accepted, failed, pause, correction, and route10 evidence.","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context","uv run --no-project python scripts/check_agent_handoff.py","uv run --no-project python tools/check_security_baseline.py","git diff --check"]}
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
