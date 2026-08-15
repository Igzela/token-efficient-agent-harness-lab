# Next Decision

Last updated: 2026-08-15.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The primary-route scope decision is owned by `docs/CURRENT_STATUS.md`. The minimal AC0 data/trace freeze and provider-free AC2 typed-execution contract are accepted; the active window is the additive AC2 typed boundary core. Deferred runtime-inventory and shared-`ProcessSupervisor` hardening is not part of the active packet.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1`, `PE7-RWE-MR-CORPUS-SAMPLING-1`, `PE7-RWE-MR-OPERATIONS-EVIDENCE-1`, and `PE7-RWE-MR-PROTOCOL-FREEZE-1` are accepted on main with their source-bound measurement, corpus, operations, and protocol contracts. The reconstructable snapshot replacement and provider-free DB preflight are accepted. The later DB RUN is retained as a non-baseline controlled failure and removed from the forward AC prerequisite chain; this planning decision does not claim an EFFECT receipt, T3 closeout, or decision-grade baseline.

## Authoritative Forward Order

```text
[window: PE7-AC2-BOUNDARY-CORE-1 — READY_FOR_EXECUTION, provider-free]

```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-AC2-BOUNDARY-CORE-1` — `READY_FOR_EXECUTION`

## Packet PE7-AC2-BOUNDARY-CORE-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC2-CONTRACT-1 — COMPLETE on accepted main `9dfe0535102890182165fc309f968f42c2526a41` (PR #469 exact head `142fad048f1d9e8dfb40aa61145108a2fe48f871`; merge `591f8c607804813fe0b809f92f494cb6bcee7820`; exact-head `PASS`; canonical workflow `31871125792`).

**Class:** `IMPLEMENT`

**Outcome:** Implement the typed boundary and adapters without migrating all callers.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md`, `engine/src/node_executor.rs`, `engine/src/storage/local_product_store/product_tasks.rs`, and `engine/tests/test_product_golden_path_g2.rs`; additive types/mappers only; no second executor, journal, scheduler, budget, or public behavior owner.

**Exit:** Exhaustive mapping tests, unknown-outcome negative tests, serialization compatibility where applicable, and no caller-visible semantic drift.

**Stop:** Mapping guesses effect status, drops usage uncertainty, or requires evaluator/verifier policy.

### Bounded execution contract

1. **Goal and non-goals.** Add the typed boundary mapping and focused owner/caller/test coverage; do not migrate unrelated callers or create a second authority.
2. **Accepted binding.** Promotion was derived from accepted main `9dfe0535102890182165fc309f968f42c2526a41`; route manifest SHA is `99493c3e9aa115cd4a9841dce65e97bc6c94422ae1b57fcf32be49a071311d41`; promotion evidence SHA is `c776330974b23e41ca017a9e99f219e197ac02d11edbeaa7229088bc0c7e4f40`.
3. **Owners and paths.** Owner: `engine/src/node_executor.rs`; caller: `engine/src/storage/local_product_store/product_tasks.rs`; focused test: `engine/tests/test_product_golden_path_g2.rs`; canonical docs and `docs/MODULE_MAP.md` are evidence destinations/owner proof only.
4. **Frozen invariants.** `ProcessOutcome` remains executor evidence; ProductTask lifecycle, leases, spend, verification, approval, output, audit, recovery, and target authority remain with existing owners.
5. **Only semantic delta.** Additive typed mappings and adapters plus focused negative/compatibility tests; no caller-wide migration in this packet.
6. **Forbidden changes.** No static route hint as authority; no effect, T3 action, Provider, target, automatic merge, schema migration, or second owner.
7. **Ordered implementation slice.** `engine/src/node_executor.rs`, `engine/src/storage/local_product_store/product_tasks.rs`, and `engine/tests/test_product_golden_path_g2.rs`: add the typed boundary mapping and focused owner/caller/test coverage without migrating unrelated callers.
8. **Failure and recovery.** Unknown or incomplete usage remains non-success and non-retryable; existing executor/store reconciliation and rollback owners remain unchanged.
9. **Verification.** Promotion preflight is `cargo fmt --all -- --check`; `cargo clippy -p engine --all-targets --all-features -- -D warnings`; `bash scripts/check_wire_codegen_drift.sh`; `git diff --check`. Implementation exit additionally requires focused Rust mapping, serialization, and unknown-outcome negative tests plus the applicable full Rust/TypeScript/stack checks; no source acceptance is claimed without those results.
10. **Compatibility and rollback.** Revert the additive boundary head while retaining the accepted AC2 contract and existing executor/store recovery paths.
11. **Evidence destination.** Record the implementation evidence in `docs/CURRENT_STATUS.md` under the AC2 boundary capability.
12. **Next action.** Keep the PR Draft while changing; perform final stable-head Standards/Spec review, canonical CI, manual squash merge, closeout, and refresh main.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["A provider-free change limited to the independently proved current-main allowed paths.","Exact-head verification and review evidence through the existing lifecycle owners."],"allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/node_executor.rs","engine/src/storage/local_product_store/product_tasks.rs","engine/tests/test_product_golden_path_g2.rs"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["Record the boundary implementation evidence in the accepted status document. (docs/CURRENT_STATUS.md:AC2)"],"external_effect_limit":0,"forbidden_changes":["Do not use FUTURE_ROUTE static paths as current-main authority.","Do not create a second controller, ledger, queue, lease, store, or workflow owner.","Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."],"forbidden_next_actions":["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.","Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.","Do not start a successor whose promotion candidate has not been independently accepted.","Do not use FUTURE_ROUTE static paths as current-main authority.","Do not create a second controller, ledger, queue, lease, store, workflow owner.","Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."],"goal":"Implement the typed boundary and adapters without migrating all callers.","ordered_steps":["engine/src/node_executor.rs, engine/src/storage/local_product_store/product_tasks.rs, engine/tests/test_product_golden_path_g2.rs: Add the typed boundary mapping and focused owner/caller/test coverage without migrating unrelated callers."],"packet_id":"PE7-AC2-BOUNDARY-CORE-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.","Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.","Stop before a Provider, target, automatic merge, authority consumption, or external effect.","Do not retry a possibly executed external effect whose outcome is unknown."],"plan_lane_state":"plan_lane_active","prerequisite_receipts":["PR #469 exact head `142fad048f1d9e8dfb40aa61145108a2fe48f871`; merge `591f8c607804813fe0b809f92f494cb6bcee7820`; exact-head `PASS`; canonical workflow `31871125792`"],"prerequisites":["PE7-AC2-CONTRACT-1"],"private_paths_allowed":false,"promotion_evidence_sha256":"c776330974b23e41ca017a9e99f219e197ac02d11edbeaa7229088bc0c7e4f40","read_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/node_executor.rs","engine/src/storage/local_product_store/product_tasks.rs","engine/tests/test_product_golden_path_g2.rs"],"risk_class":"none","rollback":"Retain the accepted rollback owner. (proved by docs/NEXT_DECISION.md:rollback)","route_manifest_sha256":"99493c3e9aa115cd4a9841dce65e97bc6c94422ae1b57fcf32be49a071311d41","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["cargo fmt --all -- --check","cargo clippy -p engine --all-targets --all-features -- -D warnings","cargo test -p engine","bash scripts/check_wire_codegen_drift.sh","git diff --check"],"verification_family":"source_focused_full","worker_tier":"T1"}
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
