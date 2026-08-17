# Next Decision

Last updated: 2026-08-17.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze, AC2 typed execution, AC3 Golden Path responsibility split, AC4 transaction views, AC5 composition root, and AC6 Rust-authoritative schema convergence are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. AC7 is now the sole provider-free contract-freeze window; no deletion, runtime change, provider call, target write, or effect is authorized until its exact removal manifest is accepted.

## Authoritative Forward Order

```text
[window: PE7-AC7-REMOVAL-MANIFEST-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-AC7-REMOVAL-MANIFEST-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC6-COMPATIBILITY-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #558 exact head `646e076cba8b349d45880dd84d4520109a11db69`; merge `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; exact-head `PASS`; canonical workflow `32006997709`.
## Packet PE7-AC7-REMOVAL-MANIFEST-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC6-COMPATIBILITY-CLOSEOUT-1 — COMPLETE on accepted main `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443` (PR #558 exact head `646e076cba8b349d45880dd84d4520109a11db69`; merge `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; exact-head `PASS`; canonical workflow `32006997709`).

**Class:** `CONTRACT`

**Outcome:** Freeze a deletion manifest grouped by one canonical owner and rollback point, with zero-caller proof and compatibility disposition per item.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md.

**Exit:** Exact files/symbols/tests/docs to delete, replacement owner, zero-caller proof, negative searches, fixture/script/SDK/Dashboard/replay checks, compatibility disposition per item, and batch order.

**Stop:** Any production, recovery, replay, fixture, script, or consumer dependency remains.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze a deletion manifest grouped by one canonical owner and rollback point, with zero-caller proof and compatibility disposition per item.
2. **Prerequisites and evidence.** Accepted main `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; checked route manifest SHA `637bbc7b9c98021ce7af373fbfa04b7caa90a6024047bdd84b95dccb9ff5ac3e`; predecessor receipt PR #558 exact head `646e076cba8b349d45880dd84d4520109a11db69`; merge `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; exact-head `PASS`; canonical workflow `32006997709`; current-main evidence SHA `251c2c655078d715eb1dd954ffe2442426054f261d938b1b07390c3b3d9ac2f3`.
3. **Owners and paths.** Canonical removal surface: `engine/src/http_server/routes.rs` (route registration), `engine/src/http_server/handlers/product_tasks.rs` (`api_approve_and_output_product_task`), and `engine/src/storage/local_product_store/product_tasks.rs` (`approve_and_output_product_task` and `approve_and_output_product_task_for_tenant`). Consumer wrappers: `sdk/python/src/agent_control_plane_sdk/client.py` (`approve_and_output_product_task`), `sdk/typescript/src/index.ts` (`approveAndOutputProductTask`), and `dashboard/src/lib/api-client.ts` (`approveAndOutputProductTask`). Test inventory: `engine/tests/test_product_golden_path_authority.rs` (unauthorized legacy-route assertion), `engine/tests/test_product_golden_path_evidence.rs` (store behavior cases), `engine/tests/test_product_golden_path_g3.rs` (G3 compatibility cases), and `engine/tests/test_product_golden_path_recovery.rs` (recovery/reuse cases).
4. **Frozen invariants.** Packet identity, route manifest SHA `637bbc7b9c98021ce7af373fbfa04b7caa90a6024047bdd84b95dccb9ff5ac3e`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/ARCHITECTURE_BOOK.md, docs/MODULE_MAP.md: Freeze AC7 removal manifest grouping obsolete symbols/routes by owner and rollback group
8. **Failure, recovery, and stop taxonomy.** Cleanup: No temporary resources created (proved by docs/ARCHITECTURE_BOOK.md:cleanup); retention: Retain canonical schemas and audit trail invariants (proved by docs/ARCHITECTURE_BOOK.md:audit); decisions: authority unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore); evaluator unchanged (docs/ARCHITECTURE_BOOK.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:rollback); schema unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore).
9. **Verification.** Candidate inventory must bind the exact route, handler, both LocalProductStore compatibility symbols, Python/TypeScript/Dashboard wrappers, and all four test files above. Run `codegraph explore "approve_and_output_product_task approve_and_output_product_task_for_tenant api_approve_and_output_product_task POST /api/v1/product/tasks/:task_id/approve-and-output"` against accepted `main` and bind its call path in the review evidence. Contract checks: `bash scripts/check_wire_codegen_drift.sh`; `bash scripts/verify_rust_typescript_stack.sh`; `uv run --no-project python tools/check_security_baseline.py`; `uv run --no-project python scripts/check_agent_handoff.py`; `git diff --check`. The successor `PE7-AC7-CLEANUP-1` must additionally prove a zero-match fixed-string search across `engine/src`, `engine/tests`, `sdk`, `dashboard`, `scripts`, `tools`, and `tests` after deletion, run the candidate-specific Rust authority/behavior/recovery tests, the Python SDK tests, the TypeScript/Dashboard checks, and the applicable fixture/script/replay checks before closeout.
10. **Compatibility, rollback, and retention.** Revertable documentation diff with zero database migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)
11. **Exit artifact.** Evidence destinations: Accepted closeout of PE7-AC6-COMPATIBILITY-CLOSEOUT-1 in Accepted Packet Receipts table (docs/CURRENT_STATUS.md:Accepted), Active window promotion for PE7-AC7-REMOVAL-MANIFEST-1 under Active Routing (docs/NEXT_DECISION.md:READY_FOR_EXECUTION).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{
  "allowed_outputs": [
    "A provider-free change limited to the independently proved current-main allowed paths.",
    "Exact-head verification and review evidence through the existing lifecycle owners."
  ],
  "allowed_paths": [
    "docs/ARCHITECTURE_BOOK.md",
    "docs/CURRENT_STATUS.md",
    "docs/FUTURE_ROUTE.md",
    "docs/MODULE_MAP.md",
    "docs/NEXT_DECISION.md"
  ],
  "authority_consumption_allowed": false,
  "dispatch_lane": "provider_free_repository_maintenance",
  "expected_artifacts": [
    "Accepted closeout of PE7-AC6-COMPATIBILITY-CLOSEOUT-1 in Accepted Packet Receipts table (docs/CURRENT_STATUS.md:Accepted)",
    "Active window promotion for PE7-AC7-REMOVAL-MANIFEST-1 under Active Routing (docs/NEXT_DECISION.md:READY_FOR_EXECUTION)",
    "Exact AC7 candidate inventory, owner/path binding, zero-caller proof plan, and compatibility disposition ready for the next packet's canonical architecture/module document update.",
    "Accepted-main CodeGraph call-path evidence bound to the exact candidate symbols and route: routes.rs route -> product_tasks.rs HTTP handler -> tenant helper -> compatibility helper; direct callers are the four Rust test files and wrappers are Python, TypeScript, and Dashboard."
  ],
  "external_effect_limit": 0,
  "forbidden_changes": [
    "Do not use FUTURE_ROUTE static paths as current-main authority.",
    "Do not create a second controller, ledger, queue, lease, store, or workflow owner.",
    "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."
  ],
  "forbidden_next_actions": [
    "Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.",
    "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.",
    "Do not start a successor whose promotion candidate has not been independently accepted.",
    "Do not use FUTURE_ROUTE static paths as current-main authority.",
    "Do not create a second controller, ledger, queue, lease, store, or workflow owner.",
    "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."
  ],
  "goal": "Freeze a deletion manifest grouped by one canonical owner and rollback point, with zero-caller proof and compatibility disposition per item.",
  "ordered_steps": [
    "docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md: reconcile the accepted predecessor, sole current window, and routing manifest without creating a second executable route.",
    "docs/ARCHITECTURE_BOOK.md: reconcile and freeze the exact AC7 removal candidates, symbols, callers, tests, owner, and rollback group.",
    "docs/MODULE_MAP.md: record the accepted ownership/deletion boundary without creating a parallel owner.",
    "docs/NEXT_DECISION.md: bind the exact candidate paths and candidate-specific verification/stop gates."
  ],
  "packet_id": "PE7-AC7-REMOVAL-MANIFEST-1",
  "packet_state": "READY_FOR_EXECUTION",
  "pause_gates": [
    "Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.",
    "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.",
    "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.",
    "Stop before a Provider, target, automatic merge, authority consumption, or external effect.",
    "Do not retry a possibly executed external effect whose outcome is unknown."
  ],
  "plan_lane_state": "plan_lane_active",
  "prerequisite_receipts": [
    "PR #558 exact head `646e076cba8b349d45880dd84d4520109a11db69`; merge `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; exact-head `PASS`; canonical workflow `32006997709`"
  ],
  "prerequisites": [
    "PE7-AC6-COMPATIBILITY-CLOSEOUT-1"
  ],
  "private_paths_allowed": false,
  "promotion_evidence_sha256": "251c2c655078d715eb1dd954ffe2442426054f261d938b1b07390c3b3d9ac2f3",
  "read_paths": [
    "docs/ARCHITECTURE_BOOK.md",
    "docs/CURRENT_STATUS.md",
    "docs/FUTURE_ROUTE.md",
    "docs/MODULE_MAP.md",
    "docs/NEXT_DECISION.md",
    "engine/src/http_server/routes.rs",
    "engine/src/http_server/handlers/product_tasks.rs",
    "engine/src/storage/local_product_store/product_tasks.rs",
    "sdk/python/src/agent_control_plane_sdk/client.py",
    "sdk/typescript/src/index.ts",
    "dashboard/src/lib/api-client.ts",
    "engine/tests/test_product_golden_path_authority.rs",
    "engine/tests/test_product_golden_path_evidence.rs",
    "engine/tests/test_product_golden_path_g3.rs",
    "engine/tests/test_product_golden_path_recovery.rs",
    "sdk/python/tests",
    "scripts",
    "tools",
    "tests"
  ],
  "risk_class": "none",
  "rollback": "Revertable documentation diff with zero database migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)",
  "route_manifest_sha256": "637bbc7b9c98021ce7af373fbfa04b7caa90a6024047bdd84b95dccb9ff5ac3e",
  "schema_version": "weak_agent_dispatch.v1",
  "secret_values_allowed": false,
  "verification": [
    "rg -n --fixed-strings -e 'approve-and-output' -e 'api_approve_and_output_product_task' -e 'approve_and_output_product_task' -e 'approveAndOutputProductTask' -- engine/src engine/tests sdk dashboard scripts tools tests",
    "test \"$(git rev-parse origin/main)\" = \"4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443\" && codegraph explore \"approve_and_output_product_task approve_and_output_product_task_for_tenant api_approve_and_output_product_task POST /api/v1/product/tasks/:task_id/approve-and-output\"",
    "bash scripts/check_wire_codegen_drift.sh",
    "bash scripts/verify_rust_typescript_stack.sh",
    "uv run --no-project python tools/check_security_baseline.py",
    "uv run --no-project python scripts/check_agent_handoff.py",
    "git diff --check",
    "PE7-AC7-CLEANUP-1 successor gate: repeat the fixed-string search with zero matches across tracked source, SDK, Dashboard, fixture, script, replay, and authority-test paths; run the candidate-specific Rust, Python SDK, TypeScript/Dashboard, fixture/script/replay checks before closeout."
  ],
  "verification_family": "docs_evidence_review",
  "worker_tier": "T2"
}
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
