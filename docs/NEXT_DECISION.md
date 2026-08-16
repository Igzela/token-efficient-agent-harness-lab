# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The current provider-free packet is the AC3 Golden Path responsibility contract; it does not change state semantics, public compatibility, or authority ownership.

## Authoritative Forward Order

```text
[window: PE7-AC3-CONTRACT-1 — IN_PROGRESS, provider-free]

```

## Active Routing

1. `PE7-AC3-CONTRACT-1` — `IN_PROGRESS`

## Packet PE7-AC3-CONTRACT-1

**State:** `IN_PROGRESS`

**Prerequisite:** PE7-AC2-CALLER-MIGRATION-1 — merge-backed COMPLETE on accepted main `8beeacfaf657cdac91e98579caf09005d3422065` (PR #478 exact head `4c748ce5f7988da9f61dd1e4650351b5d6c8bf72`; merge `36d7b33a5483cff63715b7981794aff1de614ae2`; exact-head `PASS`; canonical workflow `31886172712`). The merge is an ancestor of accepted main; this promotion synchronizes its already-accepted receipt into `CURRENT_STATUS.md` and does not create implementation evidence.

**Class:** `CONTRACT`

**Outcome:** Freeze the Golden Path responsibility matrix, state transitions, audit identities, pure inputs/outputs, effect ports, store commands, and migration sequence, plus the minimal route-control enforcement needed to keep this packet machine-bound without changing ProductTask runtime behavior.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py. Do not modify product source or ProductTask test paths; they are read-only evidence bindings in the dispatch capsule.

**Exit:** A file-level extraction contract with golden-trace equivalence and exact forbidden ownership imports.

**Stop:** Responsibility cannot be separated without changing authority order or creating a second state machine.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze the Golden Path responsibility matrix, state transitions, audit identities, pure inputs/outputs, effect ports, store commands, and migration sequence, plus the minimal route-control enforcement needed to keep this packet's edit/read scopes and verification contract machine-bound. Product runtime behavior remains unchanged.
2. **Prerequisites and evidence.** Accepted main `8beeacfaf657cdac91e98579caf09005d3422065`; checked route manifest SHA `373c31903eb3c0a98996ecab383b5dcfaef32e6e08d91028f661bac637ce7ded`; predecessor receipt PR #478 exact head `4c748ce5f7988da9f61dd1e4650351b5d6c8bf72`; merge `36d7b33a5483cff63715b7981794aff1de614ae2`; exact-head `PASS`; canonical workflow `31886172712`; current-main evidence SHA `e25df815c67e9b5ecabbc3897136e3e74cdc3c6558776384539f9f4256555c9f`.
3. **Owners and paths.** Product owners: engine/src/product_golden_path.rs; callers: engine/src/http_server/handlers/product_tasks.rs, engine/src/storage/local_product_store/product_tasks.rs, engine/src/storage/local_product_store/managed_acceptance.rs; tests: engine/tests/test_product_golden_path_g3.rs, engine/tests/test_product_golden_path_recovery.rs. The bounded executable delta is limited to scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/session_context.py, tests/test_agent_route_driver.py, and tests/test_session_context.py, plus the five canonical documents; product source/test paths are read-only evidence.
4. **Frozen invariants.** Packet identity, route manifest SHA `373c31903eb3c0a98996ecab383b5dcfaef32e6e08d91028f661bac637ce7ded`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract and its bounded route-control edit/read/verification enforcement; do not change ProductTask runtime behavior.
6. **Forbidden changes.** No static route hint is authority; no product runtime/source or ProductTask test edits; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** The route-control enforcement slice (scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py) was accepted by PR #479 and is bound by the existing `read_paths` superset, allowed-path subset, and owner/caller/test-in-read-scope checks; the remaining slices are the five canonical documents (bind the accepted AC3 contract and packet boundary) and the read-only product source/test paths (re-prove owner, caller, and recovery anchors without edits).
8. **Failure, recovery, and stop taxonomy.** Cleanup: No cleanup or runtime mutation is authorized; retain all existing ProductTask, audit, recovery, and evidence paths. (proved by docs/ARCHITECTURE_BOOK.md:recovery); retention: Retain the accepted AC2 caller-migration receipt, exact-head review, canonical CI, merge, and refreshed-main evidence in CURRENT_STATUS. (proved by docs/CURRENT_STATUS.md:receipt); decisions: authority unchanged (docs/MODULE_MAP.md:authority); evaluator unchanged (docs/CURRENT_STATUS.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:recovery); schema unchanged (docs/CURRENT_STATUS.md:schema).
9. **Verification.** Canonical operator command: uv run --no-project python scripts/check_agent_handoff.py; local fallback when the broken Python shim prevents uv: /usr/bin/python3 scripts/check_agent_handoff.py; git diff --check; focused and full provider-free Python tests.
10. **Compatibility, rollback, and retention.** Revert the docs and bounded route-control repair together and restore the prior AC2 current window if the refreshed contract cannot be proven from accepted main. (proved by docs/NEXT_DECISION.md:AC2)
11. **Exit artifact.** Evidence destinations: Canonical route evidence. (docs/NEXT_DECISION.md:canonical)
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### Frozen AC3 contract bindings

The concrete AC3 contract is frozen by reference to existing canonical owners and trace anchors; this packet does not duplicate or redefine them:

| Contract field | Frozen binding |
|---|---|
| Responsibility matrix | `docs/MODULE_MAP.md` Verification row and `docs/ARCHITECTURE_BOOK.md` Product Golden Path section: orchestration, LocalProductStore mutation, and external effects remain separate owners. |
| State transitions and golden traces | `docs/CURRENT_STATUS.md` Provider-free golden traces and parity anchors: intake/admission, compile/execute/verify, recovery/compensation, and delegation/approval/output. No new state or transition is introduced. |
| Audit identities | Exact tenant/workspace, ProductTask version, plan/run/node attempt, lease/owner token, executable/provider/model, budget, source/tree, artifact, approval, output receipt, and audit bindings named by `ARCHITECTURE_BOOK.md` and `MODULE_MAP.md`. |
| Pure inputs and outputs | Existing `ProductTaskIntakeRequest`/`ValidatedProductTaskIntake`, executable-graph projection, verification result, and redacted task projections; no new durable field or public projection. |
| Pure contract symbols | `validate_intake`, `compile_product_executable_graph`, `intake_contract_sha256`, and `redacted_intake_json` in `engine/src/product_golden_path.rs`; these remain pure/orchestration-side projections and add no durable field. |
| Effect ports and store commands | Existing scheduler/executor/provider/output ports; store commands `admit_product_task`, `reserve_product_task`, `transition_product_task`, `compile_and_schedule_product_task`, `finalize_product_task_after_execution`, `approve_product_task_for_tenant`, `approve_product_task`, `output_product_task_for_tenant`, `output_product_task`, `approve_and_output_product_task_for_tenant`, `recover_product_task_workspace_for_tenant`, and `fail_product_task_and_compensate`; this contract invokes none and adds no mutation path. |
| Migration sequence | `PE7-AC3-CONTRACT-1` → `PE7-AC3-ORCHESTRATOR-CORE-1` → `PE7-AC3-PORT-MIGRATION-1`; later packets must preserve the bindings above and prove golden-trace equivalence before changing code. |
| Forbidden ownership imports | `engine/src/product_golden_path.rs` must not import or call the store mutation commands above, provider adapters under `engine/src/provider/`, HTTP approval/output handlers, or target-output owners; handlers and callers remain adapters; `engine/src/storage/local_product_store/product_tasks.rs` remains the sole ProductTask mutation owner. Effect owners must not mutate ProductTask lifecycle or mint authority. Any required ownership move is `DECISION_REQUIRED`. |

The product executable source and test paths listed in the capsule's `read_paths` are read-only evidence bindings for this CONTRACT packet. The six route-control/test paths and five canonical documents are the only allowed edits; `allowed_paths` is the closed edit scope and `read_paths` is its safe superset. The explicit forbidden-change rule prevents ProductTask runtime or store ownership edits here.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["A provider-free change limited to the independently proved current-main edit paths.","Exact-head verification and review evidence through the existing lifecycle owners."],"allowed_paths":["docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","scripts/agent-control/local_verification.py","scripts/agent-control/route_driver.py","scripts/check_agent_handoff.py","scripts/session_context.py","tests/test_agent_route_driver.py","tests/test_session_context.py"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["docs/CURRENT_STATUS.md: accepted prerequisite and closeout evidence","docs/NEXT_DECISION.md: promoted current execution window","docs/FUTURE_ROUTE.md: refreshed routing inventory"],"external_effect_limit":0,"forbidden_changes":["Do not use FUTURE_ROUTE static paths as current-main authority.","Do not create a second controller, ledger, queue, lease, store, or workflow owner.","Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target.","Do not edit product runtime/source or ProductTask test paths; they are read-only evidence bindings. Only the listed route-control files and canonical documents may be edited."],"forbidden_next_actions":["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.","Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.","Do not start a successor whose promotion candidate has not been independently accepted.","Do not use FUTURE_ROUTE static paths as current-main authority.","Do not create a second controller, ledger, queue, lease, store, or workflow owner.","Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target.","Do not edit product runtime/source or ProductTask test paths; they are read-only evidence bindings. Only the listed route-control files and canonical documents may be edited."],"goal":"Freeze the Golden Path responsibility contract and enforce its bounded route-control edit/read/verification scope without changing ProductTask runtime behavior.","ordered_steps":["docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Bind the accepted AC3 contract and packet boundary.","read_paths only: engine/src/product_golden_path.rs, engine/src/http_server/handlers/product_tasks.rs, engine/src/storage/local_product_store/managed_acceptance.rs, engine/src/storage/local_product_store/product_tasks.rs, engine/tests/test_product_golden_path_g3.rs, engine/tests/test_product_golden_path_recovery.rs: Re-prove owner, caller, and recovery anchors without edits.","The route-control enforcement slice (scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py) is already accepted by PR #479."],"packet_id":"PE7-AC3-CONTRACT-1","packet_state":"IN_PROGRESS","pause_gates":["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.","Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.","Stop before a Provider, target, automatic merge, authority consumption, or external effect.","Do not retry a possibly executed external effect whose outcome is unknown."],"plan_lane_state":"plan_lane_active","prerequisite_receipts":["PR #478 exact head `4c748ce5f7988da9f61dd1e4650351b5d6c8bf72`; merge `36d7b33a5483cff63715b7981794aff1de614ae2`; exact-head `PASS`; canonical workflow `31886172712`"],"prerequisites":["PE7-AC2-CALLER-MIGRATION-1"],"private_paths_allowed":false,"promotion_evidence_sha256":"e25df815c67e9b5ecabbc3897136e3e74cdc3c6558776384539f9f4256555c9f","read_paths":["docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/http_server/handlers/product_tasks.rs","engine/src/product_golden_path.rs","engine/src/storage/local_product_store/managed_acceptance.rs","engine/src/storage/local_product_store/product_tasks.rs","engine/tests/test_product_golden_path_g3.rs","engine/tests/test_product_golden_path_recovery.rs","scripts/agent-control/local_verification.py","scripts/agent-control/route_driver.py","scripts/check_agent_handoff.py","scripts/session_context.py","tests/test_agent_route_driver.py","tests/test_session_context.py"],"risk_class":"none","rollback":"Revert the docs and bounded route-control repair together and restore the prior AC2 current window if the refreshed contract cannot be proven from accepted main.","route_manifest_sha256":"373c31903eb3c0a98996ecab383b5dcfaef32e6e08d91028f661bac637ce7ded","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"verification_family":"docs_evidence_review","worker_tier":"T2"}
-->

## Common Execution Protocol

- `READY_FOR_EXECUTION` packets require a valid dispatch capsule; blocked or decision-required packets carry no executable capsule.
- Refresh accepted `main`, the current packet, exact PR head, CI, and review receipts before each transition.
- Keep a changing PR Draft; run focused/local full checks, then one final stable-head Standards/Spec review, Ready, canonical exact-head CI, manual squash merge, and main refresh.
- No Provider call, credential read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner in this packet.
- Unknown, stale, missing, or conflicting evidence remains fail-closed; never retry an effect whose outcome may be unknown.

## Hard Stops

- `DECISION_REQUIRED` when a caller, owner, path, semantics, compatibility fact, rollback, or evidence cannot be re-proved from accepted `main`.
- No Provider call, credential read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner.
- Unknown or possibly executed effects are never treated as success or retried.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is routing-only. The promoted packet was removed from that index and its manifest was refreshed; no future sketch authorizes code or an effect until promoted into this document from accepted `main`.
