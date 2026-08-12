# Next Decision

Last updated: 2026-08-12.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, a Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

## Authoritative Forward Order

```text
[window: Controller dispatch input-limit repair — READY_FOR_EXECUTION, provider-free]
→ [route automation bootstrap reconciliation — BLOCKED_PREREQUISITE, provider-free]
→ [route-autopilot adversarial soak — provider-free]
→ [PREFLIGHT B1/B2/provenance contract → bounded repair → provider-free PREFLIGHT]
→ [viability RUN — typed T3 pause]
→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-CONTROLLER-DISPATCH-INPUT-LIMIT-REPAIR-1` — `READY_FOR_EXECUTION`. Sole current packet: restore the existing `agent-controller.yml` dispatch surface below GitHub's 25-input hard limit while preserving one controller owner and fail-closed T3 receipt validation.
2. The prior route bootstrap window is blocked until the repair is accepted, live `command=status` dispatch succeeds, and route starts from the refreshed accepted main. Failed attempt `ea64fd6d-fb8e-5c54-b86c-ae8f96c17550` is retained as non-authorizing failure evidence and is not a checkpoint.
3. The first canonical provider-free soak and every later packet remain routing-only until the existing promotion planner proves and accepts one exact current-main successor contract.
4. Provider, credential, target, release, deployment, automatic merge, EFFECT execution, and T3 action remain forbidden.

<!-- controller-dispatch-input-limit-repair:v1 packet_id=PE7-CONTROLLER-DISPATCH-INPUT-LIMIT-REPAIR-1 -->

## Blocked Successors

The fresh bootstrap reached the sole controller dispatch boundary but GitHub rejected it before creating a run: HTTP 422 reported 28 declared `workflow_dispatch` inputs against a maximum of 25. The route then stopped with `route_controller_unavailable_timeout`, with no claim, PR, Provider call, target write, or effect. Route automation remains blocked until the bounded repair below is accepted and a read-only live status dispatch proves the workflow can start. The old route10 attempt remains non-resumable evidence from an obsolete main, not a checkpoint. Any external-effect packet still pauses for a separate exact finite T3 receipt.

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

## Packet PE7-CONTROLLER-DISPATCH-INPUT-LIMIT-REPAIR-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** Accepted main `aa83ac1f5eada74199e0ce28ecb91d37a48769d6` contains the accepted route, control-binding, and SQLite race-repair receipts. The failed fresh route attempt `ea64fd6d-fb8e-5c54-b86c-ae8f96c17550` and the exact manual reproduction both prove HTTP 422 before workflow creation: `agent-controller.yml` declares 28 `workflow_dispatch` inputs while GitHub permits at most 25. No route claim, PR, Provider call, target write, or effect occurred.

**Class:** `IMPLEMENT`

**Outcome:** Restore the sole existing controller workflow as a valid GitHub dispatch surface by consolidating only the dormant route T3/owner receipt fields into one bounded, versioned payload while preserving the existing dispatcher, ledger, actor derivation, validation, and single-owner boundaries. Prove the route's provider-free `promote-plan` dispatch can start again; do not execute a T3 or external effect.

**Allowed delta:** `.github/workflows/agent-controller.yml`, `scripts/agent-control/dispatcher.py`, focused existing workflow/route transport tests in `tests/test_agent_orchestrator_repairs.py` and `tests/test_agent_plan_promotion.py`, plus the smallest closeout synchronization in `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `tests/test_session_context.py` after implementation acceptance.

**Exit:** The workflow declares at most 25 inputs with enforced headroom; ordinary controller commands preserve their exact fields; both route receipt commands accept only one size-bounded versioned exact-key payload and reject malformed, missing, extra, mistyped, or conflicting data before any ledger mutation; focused/full checks, stable-head review, canonical CI, manual merge, and read-only live `command=status` dispatch all pass; route automation is restored as the sole next packet but is not started by the repair PR.

**Stop:** Any second workflow/controller/ledger owner; removal or weakening of existing receipt validation; a payload that is shell-evaluated, accepts unknown keys, contains credentials/raw prompts/outputs, or can mutate before validation; workflow input count still above 25; schema/evaluator/product-runtime change; PR #225 activity; Provider/credential/target/release/deployment/auto-merge/EFFECT/T3 action; or a choice that cannot preserve backward-safe route receipt semantics.

### Twelve-field contract

1. **Outcome and non-goals.** Repair only the invalid GitHub workflow-dispatch transport so the accepted route can reach its existing controller. Non-goals: route execution, T3 authorization/receipt recording, effect execution, a generic new transport, Provider calls, credentials, targets, releases, deployments, auto-merge, or product-runtime changes.
2. **Prerequisites and evidence.** Bind implementation to the accepted main containing this authority. Preserve the failed route attempt, the HTTP 422 reproduction, #405 correction/readback, route10 evidence, and all prior receipts as append-only evidence.
3. **Owners and paths.** `agent-controller.yml` remains the sole workflow owner; `dispatcher.py` remains the sole command/ledger mutation owner. Allowed implementation paths are `.github/workflows/agent-controller.yml`, `scripts/agent-control/dispatcher.py`, `tests/test_agent_orchestrator_repairs.py`, and `tests/test_agent_plan_promotion.py`. Closeout may update only `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `tests/test_session_context.py`.
4. **Frozen invariants.** Existing command names, controller actor derivation, exact binding validation, ledger owner, emergency stop, manual merge, exact-head review/CI, and no-effect default remain unchanged. Route10 and the failed fresh attempt are evidence, not checkpoints.
5. **Only semantic delta.** Replace the 13 route-receipt-only workflow inputs with one bounded versioned `route_payload` input used only by `record-route-t3-receipt` and `record-route-owner-outcome`; decode it without shell evaluation and delegate to the existing validated functions. Other commands and fields retain their current contract.
6. **Forbidden changes.** No second controller/workflow/ledger, no generic arbitrary command payload, no caller-supplied operator identity, no validation bypass, no logging raw payloads, no schema/evaluator/authority widening, no route start inside implementation, and no forbidden external action.
7. **Ordered implementation slices.** (a) add tests proving the current 28-input defect and required <=25 contract; (b) add exact versioned payload parsing with length/key/type constraints before mutation; (c) rewire only the two route receipt cases and remove their individual workflow inputs; (d) prove all other command inputs unchanged; (e) run focused/full checks; (f) review, canonical CI, manual merge, closeout; (g) live-dispatch only `command=status`, then restore the route packet.
8. **Failure, restart, idempotency, concurrency, and stop taxonomy.** Invalid or unavailable payloads fail before dispatcher mutation. Duplicate valid receipt semantics remain owned by the existing idempotent ledger functions. Failed workflow dispatch remains visible and non-authorizing; do not infer success from absence of a run.
9. **Verification.** Run the focused workflow-contract and T3 transport suites, all Python tests, applicable control-plane/route/handoff/security checks, workflow YAML parsing, and `git diff --check`; then exact-head independent review and canonical CI. After merge, a read-only `status` workflow dispatch must create and complete one exact-main run before route restoration.
10. **Compatibility, rollback, cleanup, and retention.** Revert the implementation merge to restore the prior interface if needed, while keeping route stopped. Retain the 422, timeout, PR/review/CI, smoke-run, and route10 evidence. Remove temporary worktrees only after accepted-main refresh.
11. **Exit artifact.** One accepted implementation PR with exact base/head/review/CI/merge evidence, tests proving input-limit and pre-mutation rejection, one successful exact-main read-only status dispatch, and a closeout restoring `PE7-ROUTE-AUTOMATION-1` as the sole current packet.
12. **Next permitted action and forbidden next actions.** Implement this repair from the accepted authority main in an isolated worktree. Do not restart route, record a T3 receipt, touch PR #225, or perform Provider/credential/target/release/deploy/auto-merge/EFFECT/T3 actions until implementation and closeout are accepted and the status smoke passes.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["One focused controller dispatch input-limit repair PR","Exact-head review, canonical CI, manual merge, closeout, and read-only status-dispatch evidence"],"allowed_paths":[".github/workflows/agent-controller.yml","scripts/agent-control/dispatcher.py","tests/test_agent_orchestrator_repairs.py","tests/test_agent_plan_promotion.py"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["A workflow_dispatch contract with at most 25 inputs and enforced headroom","A bounded versioned exact-key route_payload parser that fails before ledger mutation","A successful post-merge read-only command=status workflow run"],"external_effect_limit":0,"forbidden_changes":["Do not create a second controller, workflow, ledger, or generic command transport.","Do not weaken route receipt validation, derive operator identity from payload, or log raw payload content.","Do not call a Provider, read credentials, write a target, release, deploy, auto-merge, execute an EFFECT/T3 action, start route, or touch PR #225."],"forbidden_next_actions":["Do not restart route before accepted implementation, closeout, and status-dispatch proof.","Do not record a T3 or owner-outcome receipt during validation.","Do not treat a missing workflow run as success."],"goal":"Restore the sole controller workflow below GitHub's workflow_dispatch input limit without widening authority or changing validated receipt semantics.","known_store_mutations":["None during implementation and tests","Existing controller Issue #208 readback only during post-merge status smoke"],"ordered_steps":["Start from the accepted authority main in an isolated worktree.","Add failing workflow input-limit and pre-mutation payload validation tests.","Implement the single-owner versioned route_payload transport.","Run focused/full checks and publish one Draft PR.","Obtain exact-head review and canonical CI, manually merge, close out, and prove read-only status dispatch."],"packet_id":"PE7-CONTROLLER-DISPATCH-INPUT-LIMIT-REPAIR-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop on any validation, owner, actor, scope, or compatibility ambiguity.","Stop before route start or any Provider, credential, target, release, deployment, auto-merge, EFFECT, or T3 action.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting."],"plan_lane_state":"plan_lane_active","private_paths_allowed":false,"prerequisite_receipts":["PE7-ROUTE-AUTOMATION-1 COMPLETE: PR #390 exact head 24618e52c969adc93e7bc092c51dde6b2d0ffea9; merge 5481053c736e7db8481cabd9316741f2a5cd6c7a; canonical workflow 31467821768","PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1 COMPLETE: implementation PR #408 merge 57a86c78c3f9611ce48c5bce249721af23db5532; canonical workflow 31593460813; correction workflow 31594277043","PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1 COMPLETE: PR #413 exact head fc8c005981d2fa12f0f494a131b839d65a46a8ba; canonical workflow 31611860646; merge 9cc118fa72d9d13a24cdf968cc5fc20dbe80b28f","Route bootstrap failure: attempt ea64fd6d-fb8e-5c54-b86c-ae8f96c17550; accepted main aa83ac1f5eada74199e0ce28ecb91d37a48769d6; HTTP 422 for 28 inputs over GitHub maximum 25; no workflow run or downstream mutation"],"prerequisites":["PE7-ROUTE-AUTOMATION-1","PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1","PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1"],"read_paths":["START_HERE.md","AGENTS.md","docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/MODULE_MAP.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md",".github/workflows/agent-controller.yml","scripts/agent-control/dispatcher.py","tests/test_agent_orchestrator_repairs.py","tests/test_agent_plan_promotion.py"],"rollback":"Revert the implementation merge, keep route stopped, and retain all 422, timeout, PR, review, CI, status-smoke, and route10 evidence.","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["PYTHONPATH=src uv run --no-project python -m unittest tests.test_agent_orchestrator_repairs tests.test_agent_plan_promotion","PYTHONPATH=src uv run --no-project python -m unittest discover -s tests","uv run --no-project python scripts/check_agent_handoff.py","uv run --no-project python tools/check_security_baseline.py","git diff --check"]}
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
