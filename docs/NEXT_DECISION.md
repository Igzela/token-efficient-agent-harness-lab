# Next Decision

Last updated: 2026-08-15.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The primary-route scope decision is owned by `docs/CURRENT_STATUS.md`. The minimal AC0 data/trace freeze and provider-free AC2 typed-execution contract are accepted. PR #472 supplied the additive boundary core but its exact-head audit exposed a fail-closed repair; the current window is the provider-free boundary repair. Caller migration remains behind that repair. Deferred runtime-inventory and shared-`ProcessSupervisor` hardening remains optional and is not an implementation frontier.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1`, `PE7-RWE-MR-CORPUS-SAMPLING-1`, `PE7-RWE-MR-OPERATIONS-EVIDENCE-1`, and `PE7-RWE-MR-PROTOCOL-FREEZE-1` are accepted on main with their source-bound measurement, corpus, operations, and protocol contracts. The reconstructable snapshot replacement and provider-free DB preflight are accepted. The later DB RUN is retained as a non-baseline controlled failure and removed from the forward AC prerequisite chain; this planning decision does not claim an EFFECT receipt, T3 closeout, or decision-grade baseline.

## Authoritative Forward Order

```text
[window: PE7-AC2-BOUNDARY-REPAIR-1 — READY_FOR_EXECUTION, provider-free]

```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-AC2-BOUNDARY-REPAIR-1` — `READY_FOR_EXECUTION`

## Packet PE7-AC2-BOUNDARY-REPAIR-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC2-CONTRACT-1 — COMPLETE; see its accepted receipt in `docs/CURRENT_STATUS.md`.

**Class:** `IMPLEMENT`

**Outcome:** Repair the AC2 typed boundary so contradictory mappings cannot authorize success and owner-proven pre-spawn refusals retain their explicit `NotStarted + KnownFailure` classification.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `engine/src/node_executor.rs`, and `engine/tests/test_product_golden_path_g2.rs`; one canonical mapping owner only; no caller-wide migration in this packet.

**Exit:** `is_known_success()` is fail-closed on effect state, owner-proven pre-spawn refusals map to `NotStarted + KnownFailure`, ambiguous unavailable outcomes remain `Unknown`, and focused negative/exhaustive tests pass without changing `process_outcome.v1` serialization.

**Stop:** The owner cannot distinguish explicit pre-spawn refusal from ambiguous unavailability, any public/wire compatibility changes become necessary, or the repair would create another execution/authority owner.

### Bounded execution contract

1. **Goal and non-goals.** Repair the existing AC2 mapping invariant and its pre-spawn classification; do not migrate callers, add AC1 shared supervision, change wire/schema contracts, or create a second owner.
2. **Accepted binding.** Promotion is bound to accepted main `f53e8ce48b232be705b61efd3e59babed94735bb`; route manifest SHA is `99493c3e9aa115cd4a9841dce65e97bc6c94422ae1b57fcf32be49a071311d41`; promotion evidence SHA is `6cc5bc4bbcd51f7fbb22a16e33e7b48df0b398861ec3b0faf00afb1f816b3b25`.
3. **Owners and paths.** Canonical owner: `engine/src/node_executor.rs`; focused tests: `engine/tests/test_product_golden_path_g2.rs`; `docs/CURRENT_STATUS.md` is the evidence destination and `docs/NEXT_DECISION.md` is the packet owner.
4. **Frozen invariants.** Existing `ProcessOutcome` construction and `process_outcome.v1` serialization remain unchanged; the typed mapping remains internal evidence; LocalProductStore and all lifecycle, authority, recovery, and target owners remain unchanged.
5. **Only semantic delta.** `is_known_success()` requires a proven started effect and known success; explicit owner-recorded pre-spawn refusal reasons map to `NotStarted + KnownFailure`; other unavailable/unknown reasons remain `Unknown`.
6. **Forbidden changes.** No caller-wide migration, AC1 `ProcessSupervisor`, public API or wire/schema migration, Provider call, target write, T3/EFFECT action, automatic merge, second executor, scheduler, store, journal, budget, evaluator, or authority owner.
7. **Ordered implementation slice.** Commit the invariant repair separately; add the owner-proven pre-spawn classification; extend G2 with contradictory-mapping, explicit-pre-spawn, ambiguous-unavailable, exhaustive-state, and serialization-compatibility assertions; update status evidence only after checks pass.
8. **Failure and recovery.** Unknown effect or outcome remains non-success and non-retryable; a failed repair PR leaves accepted main unchanged; no rollback may restore the known contradictory-success defect; any post-merge recovery must use a replacement fail-closed repair.
9. **Verification.** Run `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets --all-features -- -D warnings`, `cargo test -p engine --test test_product_golden_path_g2`, `cargo test -p engine`, `bash scripts/check_wire_codegen_drift.sh`, `uv run --no-project python scripts/check_agent_handoff.py`, and `git diff --check`; canonical CI remains required for acceptance.
10. **Compatibility and rollback.** No serialized field or public API changes are permitted. Keep the repair commit separate from later caller migration; if this packet is not accepted, abandon its branch without changing main, and never blindly revert an accepted repair to the known-defective boundary.
11. **Evidence destination.** Record exact repair, focused/full test, review, CI, merge, and rollback evidence in `docs/CURRENT_STATUS.md` under the AC2 boundary repair capability.
12. **Next action.** Keep the repair PR Draft while changing; run final stable-head Standards/Spec review, mark Ready once, run canonical exact-head CI, manually squash merge, refresh main, then promote caller migration in a separate routing closeout.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["A provider-free fail-closed AC2 boundary repair limited to the independently proved current-main allowed paths.","Exact-head verification and review evidence through the existing lifecycle owners."],"allowed_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","engine/src/node_executor.rs","engine/tests/test_product_golden_path_g2.rs"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["Record boundary-repair implementation and verification evidence in the accepted status document. (docs/CURRENT_STATUS.md:AC2)"],"external_effect_limit":0,"forbidden_changes":["Do not add AC1 ProcessSupervisor or a second runtime/executor/store/authority owner.","Do not change public wire/schema contracts or create a migration.","Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."],"forbidden_next_actions":["Do not treat unknown effect or outcome state as success or retryable.","Do not use FUTURE_ROUTE static paths as current-main authority.","Do not create a second controller, ledger, queue, lease, store, workflow owner.","Do not retry a possibly executed external effect whose outcome is unknown."],"goal":"Repair the AC2 typed boundary invariant and preserve owner-proven pre-spawn classification without changing wire/schema or authority ownership.","ordered_steps":["Repair ProcessBoundaryMapping::is_known_success and owner-proven pre-spawn classification in engine/src/node_executor.rs; add focused exhaustive/negative/compatibility coverage in engine/tests/test_product_golden_path_g2.rs; update docs/CURRENT_STATUS.md at closeout."],"packet_id":"PE7-AC2-BOUNDARY-REPAIR-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop when an owner, state, reason, test, path, operation, destination, or decision cannot be re-proved from accepted main.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.","Stop before a Provider, target, automatic merge, authority consumption, or external effect.","Do not retry a possibly executed external effect whose outcome is unknown."],"plan_lane_state":"plan_lane_active","prerequisite_receipts":["PE7-AC2-CONTRACT-1 COMPLETE: PR #469 exact head `142fad048f1d9e8dfb40aa61145108a2fe48f871`; merge `591f8c607804813fe0b809f92f494cb6bcee7820`; exact-head `PASS`; canonical workflow `31871125792`"],"prerequisites":["PE7-AC2-CONTRACT-1"],"private_paths_allowed":false,"promotion_evidence_sha256":"6cc5bc4bbcd51f7fbb22a16e33e7b48df0b398861ec3b0faf00afb1f816b3b25","read_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","engine/src/node_executor.rs","engine/tests/test_product_golden_path_g2.rs"],"risk_class":"none","rollback":"Do not change accepted main on failed repair; never blindly revert an accepted repair to the known-defective boundary.","route_manifest_sha256":"99493c3e9aa115cd4a9841dce65e97bc6c94422ae1b57fcf32be49a071311d41","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["cargo fmt --all -- --check","cargo clippy -p engine --all-targets --all-features -- -D warnings","cargo test -p engine --test test_product_golden_path_g2","cargo test -p engine","bash scripts/check_wire_codegen_drift.sh","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"verification_family":"source_focused_full","worker_tier":"T1"}
-->

## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Existing route boundary (quoted for compatibility, not new packet authority): The sole exception is the current packet's dispatch-capsule-authorized, one-per-claim local OpenCode weak-worker Provider invocation; it cannot make the controller read, pass, persist, or report a credential. This packet's external-effect limit is zero and does not use that exception.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- `READY_FOR_EXECUTION` packets require a valid dispatch capsule; blocked packets carry no executable capsule.
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
