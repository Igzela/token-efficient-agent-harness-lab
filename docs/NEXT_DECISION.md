# Next Decision

Last updated: 2026-08-15.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The current provider-free packet is the AC3 Golden Path responsibility contract; it does not change state semantics, public compatibility, or authority ownership.

## Authoritative Forward Order

```text
[window: PE7-AC3-CONTRACT-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-AC3-CONTRACT-1` — `READY_FOR_EXECUTION`

## Packet PE7-AC3-CONTRACT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC2-CALLER-MIGRATION-1 — merge-backed COMPLETE on accepted main `8beeacfaf657cdac91e98579caf09005d3422065` (PR #478 exact head `4c748ce5f7988da9f61dd1e4650351b5d6c8bf72`; merge `36d7b33a5483cff63715b7981794aff1de614ae2`; exact-head `PASS`; canonical workflow `31886172712`). The merge is an ancestor of accepted main; this promotion synchronizes its already-accepted receipt into `CURRENT_STATUS.md` and does not create implementation evidence.

**Class:** `CONTRACT`

**Outcome:** Freeze the Golden Path responsibility matrix, state transitions, audit identities, pure inputs/outputs, effect ports, store commands, and migration sequence.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, engine/src/http_server/handlers/product_tasks.rs, engine/src/product_golden_path.rs, engine/src/storage/local_product_store/product_tasks.rs, engine/tests/test_product_golden_path_g3.rs, engine/tests/test_product_golden_path_recovery.rs. The source and test entries are read-only evidence bindings; this CONTRACT packet authorizes no executable source or test edits.

**Exit:** A file-level extraction contract with golden-trace equivalence and exact forbidden ownership imports.

**Stop:** Responsibility cannot be separated without changing authority order or creating a second state machine.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze the Golden Path responsibility matrix, state transitions, audit identities, pure inputs/outputs, effect ports, store commands, and migration sequence.
2. **Prerequisites and evidence.** Accepted main `8beeacfaf657cdac91e98579caf09005d3422065`; checked route manifest SHA `373c31903eb3c0a98996ecab383b5dcfaef32e6e08d91028f661bac637ce7ded`; predecessor receipt PR #478 exact head `4c748ce5f7988da9f61dd1e4650351b5d6c8bf72`; merge `36d7b33a5483cff63715b7981794aff1de614ae2`; exact-head `PASS`; canonical workflow `31886172712`; current-main evidence SHA `b851c1a10e1b2271fd6ac9da85bc409a51f080395e29adb38623f3ea2f5a3ab8`.
3. **Owners and paths.** Owners: engine/src/product_golden_path.rs; callers: engine/src/http_server/handlers/product_tasks.rs, engine/src/storage/local_product_store/product_tasks.rs; tests: engine/tests/test_product_golden_path_g3.rs, engine/tests/test_product_golden_path_recovery.rs.
4. **Frozen invariants.** Packet identity, route manifest SHA `373c31903eb3c0a98996ecab383b5dcfaef32e6e08d91028f661bac637ce7ded`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Promote the accepted AC3 contract and bind its file-level ownership split.; engine/src/product_golden_path.rs, engine/src/http_server/handlers/product_tasks.rs, engine/src/storage/local_product_store/product_tasks.rs: Freeze orchestration and existing verification callers without changing state, persistence, or external-effect semantics.; engine/tests/test_product_golden_path_g3.rs, engine/tests/test_product_golden_path_recovery.rs: Retain existing Golden Path and recovery coverage as equivalence anchors.
8. **Failure, recovery, and stop taxonomy.** Cleanup: No cleanup or runtime mutation is authorized; retain all existing ProductTask, audit, recovery, and evidence paths.; retention: Retain the accepted AC2 caller-migration receipt, exact-head review, canonical CI, merge, and refreshed-main evidence in CURRENT_STATUS.; decisions: schema unchanged: docs/CURRENT_STATUS.md; evaluator unchanged: docs/CURRENT_STATUS.md; authority unchanged: docs/MODULE_MAP.md; recovery unchanged: docs/ARCHITECTURE_BOOK.md.
9. **Verification.** Canonical operator command: uv run --no-project python scripts/check_agent_handoff.py; machine-allowlisted capsule display: python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revert the documentation-only promotion and restore the prior AC2 current window if the refreshed contract cannot be proven from accepted main.
11. **Exit artifact.** Evidence destinations: docs/CURRENT_STATUS.md: accepted prerequisite and closeout evidence, docs/NEXT_DECISION.md: promoted current execution window, docs/FUTURE_ROUTE.md: refreshed routing inventory.
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### Frozen AC3 contract bindings

The concrete AC3 contract is frozen by reference to existing canonical owners and trace anchors; this packet does not duplicate or redefine them:

| Contract field | Frozen binding |
|---|---|
| Responsibility matrix | `docs/MODULE_MAP.md` Verification row and `docs/ARCHITECTURE_BOOK.md` Product Golden Path section: orchestration, LocalProductStore mutation, and external effects remain separate owners. |
| State transitions and golden traces | `docs/CURRENT_STATUS.md` Provider-free golden traces and parity anchors: intake/admission, compile/execute/verify, recovery/compensation, and delegation/approval/output. No new state or transition is introduced. |
| Audit identities | Exact tenant/workspace, ProductTask version, plan/run/node attempt, lease/owner token, executable/provider/model, budget, source/tree, artifact, approval, output receipt, and audit bindings named by `ARCHITECTURE_BOOK.md` and `MODULE_MAP.md`. |
| Pure inputs and outputs | Existing `ProductTaskIntakeRequest`/`ValidatedProductTaskIntake`, executable-graph projection, verification result, and redacted task projections; no new durable field or public projection. |
| Effect ports and store commands | Existing scheduler/executor/provider/output ports and the LocalProductStore commands named in the `CURRENT_STATUS.md` persistence row; this contract invokes none and adds no mutation path. |
| Migration sequence | `PE7-AC3-CONTRACT-1` → `PE7-AC3-ORCHESTRATOR-CORE-1` → `PE7-AC3-PORT-MIGRATION-1`; later packets must preserve the bindings above and prove golden-trace equivalence before changing code. |
| Forbidden ownership imports | Orchestration must not become a store/provider/approval/output owner; LocalProductStore remains the sole mutation owner; effect owners must not mutate ProductTask lifecycle or mint authority. Any required ownership move is `DECISION_REQUIRED`. |

The source and test paths listed in the capsule are read-only evidence bindings for this CONTRACT packet. The separate `read_only_paths` field, the explicit forbidden-change rule, and the docs-only verification family prevent them from authorizing implementation edits here.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/http_server/handlers/product_tasks.rs", "engine/src/product_golden_path.rs", "engine/src/storage/local_product_store/product_tasks.rs", "engine/tests/test_product_golden_path_g3.rs", "engine/tests/test_product_golden_path_recovery.rs"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["docs/CURRENT_STATUS.md: accepted prerequisite and closeout evidence", "docs/NEXT_DECISION.md: promoted current execution window", "docs/FUTURE_ROUTE.md: refreshed routing inventory"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target.", "Do not edit executable source or test paths in this CONTRACT packet; they are read-only evidence bindings for later implementation packets."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target.", "Do not edit executable source or test paths in this CONTRACT packet; they are read-only evidence bindings for later implementation packets."], "goal": "Freeze the Golden Path responsibility matrix, state transitions, audit identities, pure inputs/outputs, effect ports, store commands, and migration sequence.", "operator_verification": ["uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "ordered_steps": ["docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Promote the accepted AC3 contract and bind its file-level ownership split.", "engine/src/product_golden_path.rs, engine/src/http_server/handlers/product_tasks.rs, engine/src/storage/local_product_store/product_tasks.rs: Freeze orchestration and existing verification callers without changing state, persistence, or external-effect semantics.", "engine/tests/test_product_golden_path_g3.rs, engine/tests/test_product_golden_path_recovery.rs: Retain existing Golden Path and recovery coverage as equivalence anchors."], "packet_id": "PE7-AC3-CONTRACT-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #478 exact head `4c748ce5f7988da9f61dd1e4650351b5d6c8bf72`; merge `36d7b33a5483cff63715b7981794aff1de614ae2`; exact-head `PASS`; canonical workflow `31886172712`"], "prerequisites": ["PE7-AC2-CALLER-MIGRATION-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "b851c1a10e1b2271fd6ac9da85bc409a51f080395e29adb38623f3ea2f5a3ab8", "read_only_paths": ["engine/src/http_server/handlers/product_tasks.rs", "engine/src/product_golden_path.rs", "engine/src/storage/local_product_store/product_tasks.rs", "engine/tests/test_product_golden_path_g3.rs", "engine/tests/test_product_golden_path_recovery.rs"], "read_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/http_server/handlers/product_tasks.rs", "engine/src/product_golden_path.rs", "engine/src/storage/local_product_store/product_tasks.rs", "engine/tests/test_product_golden_path_g3.rs", "engine/tests/test_product_golden_path_recovery.rs"], "risk_class": "none", "rollback": "Revert the documentation-only promotion and restore the prior AC2 current window if the refreshed contract cannot be proven from accepted main.", "route_manifest_sha256": "373c31903eb3c0a98996ecab383b5dcfaef32e6e08d91028f661bac637ce7ded", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "docs_evidence_review", "worker_tier": "T2"}
-->

## Common Execution Protocol

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
