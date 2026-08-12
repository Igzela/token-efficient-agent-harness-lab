# Next Decision

Last updated: 2026-08-12.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, a Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

## Authoritative Forward Order

```text
[window: SQLite workspace-preparation receipt race repair — READY_FOR_EXECUTION, provider-free]
→ [route automation bootstrap reconciliation — BLOCKED_PREREQUISITE, provider-free]
→ [route-autopilot adversarial soak — provider-free]
→ [PREFLIGHT B1/B2/provenance contract → bounded repair → provider-free PREFLIGHT]
→ [viability RUN — typed T3 pause]
→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1` — `READY_FOR_EXECUTION`. Sole current packet: remove the SQLite receipt/status time-of-check/time-of-use race that can misclassify a concurrent duplicate intake as legacy reconciliation, without changing schema or weakening genuine crash-left recovery.
2. The accepted route bootstrap window is `BLOCKED_PREREQUISITE`. PR #411 accepted its fresh bootstrap authority, but the route remains emergency-stopped until the race repair is independently accepted and the repaired main is green.
3. The first canonical provider-free soak and every later packet remain routing-only until the route bootstrap closes and the existing promotion planner proves one exact current-main successor contract.
4. Provider, credential, target, release, deployment, automatic merge, EFFECT execution, and T3 action remain forbidden.

<!-- workspace-prep-receipt-race-repair:v1 packet_id=PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1 -->

## Blocked Successors

PR #411 accepted one newly bound provider-free bootstrap from decision baseline `72c196aa03a5632bfbd47ba5f19cbc51a154889c`, but accepted-main validation exposed a reproducible SQLite receipt/status observation race before route start. The route remains emergency-stopped until the bounded repair below is accepted. The old route10 attempt remains non-resumable evidence from an obsolete main, not a checkpoint. Any external-effect packet still pauses for a separate exact finite T3 receipt.

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

## Packet PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PR #411 accepted the fresh route bootstrap authority on main, but no route transition has started. Accepted-main validation reproducibly showed a concurrent duplicate intake observing `workspace_preparing` after an earlier receipt miss. The repair must begin from the live accepted main and keep the controller emergency-stopped.

**Class:** `REPAIR`

**Outcome:** Make SQLite receipt and task-status observation transactionally consistent so a duplicate intake cannot convert a valid concurrent receipt publication into false legacy-reconciliation evidence. Preserve the genuine missing-receipt recovery stop, PostgreSQL behavior, single ProductTask owner, and all physical-worktree compensation boundaries.

**Allowed delta:** Only `engine/src/storage/local_product_store/product_tasks.rs`, `engine/tests/test_product_golden_path_recovery.rs`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and hard-coded canonical packet assertions in `tests/test_session_context.py`. Move the SQLite receipt/status decision under one existing store-owned transaction/connection lock; add focused concurrency regression evidence and minimal status synchronization. No schema or owner change.

**Exit:** Focused concurrent duplicate-intake and locked-recovery tests are stable; applicable Rust, PostgreSQL, security, handoff, and canonical exact-head CI are green; independent complete-diff review is exact `PASS`; the implementation is manually squash-merged; and the canonical main status restores `PE7-ROUTE-AUTOMATION-1` as the sole ready window.

**Stop:** Any need to change schema, durable recovery semantics, ProductTask ownership, worktree compensation, PostgreSQL locking, or external-effect authority; any stale/conflicting head or scope binding; PR #225 activity; or any Provider/credential/target/release/deployment/auto-merge/EFFECT/T3 action.

### Twelve-field contract

1. **Outcome and non-goals.** Repair one SQLite concurrent receipt/status TOCTOU. Non-goals: schema changes, new locks or owners, PostgreSQL semantic changes, worktree recovery redesign, route execution, or any external effect.
2. **Prerequisites and evidence.** Begin from accepted main containing PR #411. The same concurrent duplicate-intake failure must remain preserved as failed evidence; a successful rerun never rewrites it.
3. **Owners and paths.** `LocalProductStore` remains sole persistence owner; `product_tasks.rs` remains the workspace-preparation lifecycle owner. Allowed paths are exactly the five paths listed above.
4. **Frozen invariants.** Receipt plus `workspace_preparing` publication remains atomic; genuine preparing-without-receipt state remains reconciliation-only; physical worktree mutation remains behind the existing receipt and guard.
5. **Only semantic delta.** SQLite determines existing receipt and current task status from one transactionally consistent connection-locked observation instead of two transaction-external reads.
6. **Forbidden changes.** No schema, migration, PostgreSQL lock, retry-budget widening, second store/lock owner, recovery weakening, route start, Provider/T3/EFFECT, credential, target, release, deploy, auto-merge, or PR #225 activity.
7. **Ordered implementation slices.** (a) add focused regression evidence for the interleaving; (b) remove the split SQLite pre-read; (c) prove existing receipt reuse, genuine missing-receipt rejection, concurrent duplicate collapse, and locked compensation recovery; (d) synchronize minimal canonical status; (e) ship through the normal manual-merge path.
8. **Failure, restart, idempotency, concurrency, and stop taxonomy.** Ordinary test/CI/review failures are repairable within scope. A true missing receipt stays typed reconciliation; any wider recovery or schema choice yields `DECISION_REQUIRED`.
9. **Verification.** Run focused recovery tests repeatedly, Rust formatting/clippy, default and `pg-tests` suites, session-context tests, security baseline, handoff, `git diff --check`, independent exact-head review, and canonical CI.
10. **Compatibility, rollback, cleanup, and retention.** Revert the implementation merge to restore prior behavior; retain both failed canonical runs and all route10/ledger evidence. No database migration or cleanup is introduced.
11. **Exit artifact.** One accepted implementation PR proving transactionally consistent SQLite observation and restoring the route bootstrap as the sole ready window.
12. **Next permitted action and forbidden next actions.** Implement only the bounded repair from a fresh accepted-main worktree. Do not enable or start route until the repair and its closeout are accepted.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["One focused SQLite receipt/status race repair","Focused concurrency regression evidence","Exact-head review, CI, manual-merge readback, and canonical status synchronization"],"allowed_paths":["engine/src/storage/local_product_store/product_tasks.rs","engine/tests/test_product_golden_path_recovery.rs","docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","tests/test_session_context.py"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_local","expected_artifacts":["Transactionally consistent SQLite receipt/status observation","Stable concurrent duplicate-intake and recovery evidence","One accepted implementation PR restoring route bootstrap readiness"],"external_effect_limit":0,"forbidden_changes":["Do not change schema, migrations, PostgreSQL locking, ProductTask ownership, or worktree compensation semantics.","Do not enable or start route before this repair is accepted.","Do not call a Provider, read credentials, write a target, release, deploy, auto-merge, execute EFFECT/T3, or touch PR #225."],"forbidden_next_actions":["Do not treat a genuine preparing-without-receipt state as ordinary concurrency.","Do not broaden retry budgets or add a second lock owner.","Do not restore route readiness before exact-head review, canonical CI, merge readback, and closeout."],"goal":"Eliminate the SQLite workspace-preparation receipt/status TOCTOU while preserving fail-closed recovery.","known_store_mutations":["Existing SQLite ProductTask receipt/status transaction only; no schema mutation"],"ordered_steps":["Refresh and bind live accepted main.","Add focused regression evidence for the split-read interleaving.","Move SQLite receipt/status observation under one existing store-owned transaction and connection lock.","Run focused and full verification, exact-head review, canonical CI, and manual squash merge.","Synchronize accepted status and restore PE7-ROUTE-AUTOMATION-1 readiness."],"packet_id":"PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop on any required schema, durable recovery, owner, or PostgreSQL semantic change.","Stop on stale, conflicting, ambiguous, or out-of-scope binding.","Stop before Provider, credential, target, release, deployment, auto-merge, EFFECT, T3, route start, or PR #225 activity."],"plan_lane_state":"plan_lane_active","private_paths_allowed":false,"prerequisite_receipts":["PE7-ROUTE-AUTOMATION-1 bootstrap authority accepted on main through PR #411; route not started"],"prerequisites":["PE7-ROUTE-AUTOMATION-1 bootstrap authority accepted; controller remains emergency-stopped"],"read_paths":["START_HERE.md","AGENTS.md","docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/MODULE_MAP.md","docs/ARCHITECTURE_BOOK.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","engine/src/storage/local_product_store/product_tasks.rs","engine/tests/test_product_golden_path_recovery.rs"],"rollback":"Revert the repair merge; retain failed canonical CI and all route/ledger evidence; keep route emergency-stopped until a replacement repair is accepted.","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["cargo fmt --all -- --check","cargo clippy -p engine --all-targets --all-features -- -D warnings","cargo test -p engine --features pg-tests --test test_product_golden_path_recovery -- --test-threads=1","scripts/ci/run_rust_tests.py","cargo test -p engine --features pg-tests -- --test-threads=1","PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context","uv run --no-project python scripts/check_agent_handoff.py","uv run --no-project python tools/check_security_baseline.py","git diff --check"]}
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
