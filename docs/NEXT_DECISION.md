# Next Decision

Last updated: 2026-08-15.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The primary-route scope decision is owned by `docs/CURRENT_STATUS.md`. The minimal AC0 data/trace freeze, provider-free AC2 typed-execution contract, and additive AC2 typed boundary core are accepted. The AC2 caller-migration packet is now the bounded provider-free implementation window. Deferred runtime-inventory and shared-`ProcessSupervisor` hardening remains optional and is not an implementation frontier.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1`, `PE7-RWE-MR-CORPUS-SAMPLING-1`, `PE7-RWE-MR-OPERATIONS-EVIDENCE-1`, and `PE7-RWE-MR-PROTOCOL-FREEZE-1` are accepted on main with their source-bound measurement, corpus, operations, and protocol contracts. The reconstructable snapshot replacement and provider-free DB preflight are accepted. The later DB RUN is retained as a non-baseline controlled failure and removed from the forward AC prerequisite chain; this planning decision does not claim an EFFECT receipt, T3 closeout, or decision-grade baseline.

## Authoritative Forward Order

```text
[window: PE7-AC2-CALLER-MIGRATION-1 — READY_FOR_EXECUTION, provider-free]

```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-AC2-CALLER-MIGRATION-1` — `READY_FOR_EXECUTION`

## Packet PE7-AC2-CALLER-MIGRATION-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC2-BOUNDARY-CORE-1 — COMPLETE; see its accepted receipt in `docs/CURRENT_STATUS.md`.

**Class:** `IMPLEMENT`

**Outcome:** Migrate enumerated executors/callers and remove only superseded internal result plumbing approved by the contract.

**Allowed delta:** Mechanical caller migration and local compatibility cleanup only.

**Exit:** All production execution paths emit the typed boundary, outcome unknown stays non-success/non-retry, and AC3 receives refreshed golden traces.

**Stop:** A caller has unclassified semantics, public compatibility breaks, or removal reaches beyond the approved internal surface.

### Bounded execution contract

1. **Goal and non-goals.** Migrate existing process-outcome decision callers to the typed AC2 boundary; do not add AC1 shared supervision, change wire/schema contracts, or create a second execution/store/authority owner.
2. **Accepted binding.** Promotion is bound to accepted main `f53e8ce48b232be705b61efd3e59babed94735bb`; route manifest SHA is `99493c3e9aa115cd4a9841dce65e97bc6c94422ae1b57fcf32be49a071311d41`; promotion evidence SHA is `c2d92cb7197f7881e7cccc63170c0f1e0ecce040af49636c5010e01adfada107`.
3. **Owners and paths.** Canonical mapping owner: `engine/src/node_executor.rs`; production callers: `engine/src/cli/cli_node_executor.rs`, `engine/src/storage/local_product_store/product_tasks.rs`, and `engine/src/storage/local_product_store/managed_acceptance.rs`; focused tests: `engine/tests/test_product_golden_path_g2.rs`, `engine/tests/test_product_golden_path_evidence.rs`, and `engine/tests/test_product_golden_path_recovery.rs`; `docs/CURRENT_STATUS.md` is the evidence destination.
4. **Frozen invariants.** Existing `ProcessOutcome` evidence serialization remains unchanged; LocalProductStore remains the lifecycle, lease, spend, verification, approval, output, audit, recovery, and target authority; provider credentials and raw output remain outside receipts.
5. **Only semantic delta.** Replace direct success/exit interpretation in the enumerated callers with `ProcessBoundaryMapping`; preserve existing wire evidence and lifecycle status names, including owner-proven pre-spawn refusals as `NotStarted + KnownFailure` while leaving ambiguous unavailable outcomes `Unknown`.
6. **Forbidden changes.** No AC1 `ProcessSupervisor`, public wire/schema or migration change, Provider call, target write, T3/EFFECT action, automatic merge, second executor, scheduler, store, journal, budget, evaluator, or authority owner.
7. **Ordered implementation slice.** First tighten the canonical mapping/caller interfaces in `node_executor.rs`; then migrate CLI terminal classification, ProductTask verification/terminal evidence, and managed acceptance checks; add only focused negative/recovery assertions and update the accepted status evidence at closeout.
8. **Failure and recovery.** `ProcessEffectState::Unknown` or `ProcessOutcomeState::Unknown` remains non-success and non-retryable; a contradictory mapping can never authorize success; preserve existing outcome-unknown, lease, rollback, and reconciliation owners; revert the caller-only head to return to the accepted boundary core.
9. **Verification.** Run `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets --all-features -- -D warnings`, focused G2/evidence/recovery tests, `cargo test -p engine`, `bash scripts/check_wire_codegen_drift.sh`, `uv run --no-project python scripts/check_agent_handoff.py`, and `git diff --check`; canonical CI remains required for acceptance.
10. **Compatibility and rollback.** No serialized field or public API changes are permitted; rollback is a revert of the caller-migration commit(s), retaining the accepted AC2 contract and boundary core.
11. **Evidence destination.** Record exact implementation, focused-test, full-test, review, CI, merge, and rollback evidence in `docs/CURRENT_STATUS.md` under the AC2 caller-migration capability.
12. **Next action.** Keep the implementation PR Draft while changing; run final stable-head Standards/Spec review, mark Ready once, run canonical exact-head CI, manually squash merge, then refresh main and close out the packet.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["A provider-free caller migration limited to the independently proved current-main allowed paths.","Exact-head verification and review evidence through the existing lifecycle owners."],"allowed_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","engine/src/node_executor.rs","engine/src/cli/cli_node_executor.rs","engine/src/storage/local_product_store/product_tasks.rs","engine/src/storage/local_product_store/managed_acceptance.rs","engine/tests/test_product_golden_path_g2.rs","engine/tests/test_product_golden_path_evidence.rs","engine/tests/test_product_golden_path_recovery.rs"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["Record caller-migration implementation and verification evidence in the accepted status document. (docs/CURRENT_STATUS.md:AC2)"],"external_effect_limit":0,"forbidden_changes":["Do not add AC1 ProcessSupervisor or a second runtime/executor/store/authority owner.","Do not change public wire/schema contracts or create a migration.","Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."],"forbidden_next_actions":["Do not treat unknown effect or outcome state as success or retryable.","Do not use FUTURE_ROUTE static paths as current-main authority.","Do not create a second controller, ledger, queue, lease, store, workflow owner.","Do not retry a possibly executed external effect whose outcome is unknown."],"goal":"Migrate existing process-outcome decision callers to ProcessBoundaryMapping without changing wire/schema or authority ownership; repair contradictory and owner-proven pre-spawn mappings fail-closed.","ordered_steps":["Repair ProcessBoundaryMapping invariants and owner-proven pre-spawn classification in engine/src/node_executor.rs; migrate engine/src/cli/cli_node_executor.rs, engine/src/storage/local_product_store/product_tasks.rs, and engine/src/storage/local_product_store/managed_acceptance.rs; add focused G2/evidence/recovery coverage; update docs/CURRENT_STATUS.md at closeout."],"packet_id":"PE7-AC2-CALLER-MIGRATION-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.","Stop before a Provider, target, automatic merge, authority consumption, or external effect.","Do not retry a possibly executed external effect whose outcome is unknown."],"plan_lane_state":"plan_lane_active","prerequisite_receipts":["Accepted main f53e8ce48b232be705b61efd3e59babed94735bb; PE7-AC2-BOUNDARY-CORE-1 accepted receipt in docs/CURRENT_STATUS.md"],"prerequisites":["PE7-AC2-BOUNDARY-CORE-1"],"private_paths_allowed":false,"promotion_evidence_sha256":"c2d92cb7197f7881e7cccc63170c0f1e0ecce040af49636c5010e01adfada107","read_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","engine/src/node_executor.rs","engine/src/cli/cli_node_executor.rs","engine/src/storage/local_product_store/product_tasks.rs","engine/src/storage/local_product_store/managed_acceptance.rs","engine/tests/test_product_golden_path_g2.rs","engine/tests/test_product_golden_path_evidence.rs","engine/tests/test_product_golden_path_recovery.rs"],"risk_class":"none","rollback":"Revert the caller-migration head while retaining the accepted AC2 boundary core.","route_manifest_sha256":"99493c3e9aa115cd4a9841dce65e97bc6c94422ae1b57fcf32be49a071311d41","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["cargo fmt --all -- --check","cargo clippy -p engine --all-targets --all-features -- -D warnings","cargo test -p engine --test test_product_golden_path_g2","cargo test -p engine --test test_product_golden_path_evidence","cargo test -p engine --test test_product_golden_path_recovery","cargo test -p engine","bash scripts/check_wire_codegen_drift.sh","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"verification_family":"source_focused_full","worker_tier":"T1"}
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
