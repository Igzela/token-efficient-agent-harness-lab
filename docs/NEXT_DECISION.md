# Next Decision

Last updated: 2026-08-17.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze, AC2 typed execution, AC3 Golden Path responsibility split, AC4 transaction views, AC5 composition root, and AC6 Rust-authoritative schema convergence are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC7 removal manifest is accepted; `PE7-AC7-CLEANUP-1` is now the sole provider-free deletion-only window. No provider call, target write, or effect is authorized.

## Authoritative Forward Order

```text
[window: PE7-AC7-CLEANUP-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-AC7-CLEANUP-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC6-COMPATIBILITY-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #558 exact head `646e076cba8b349d45880dd84d4520109a11db69`; merge `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; exact-head `PASS`; canonical workflow `32006997709`.
## Completed (PE7-AC7-REMOVAL-MANIFEST-1)

**Historical state:** `COMPLETE`

**Accepted evidence:** PR #560 exact head `5567c670cb0338bf3bf089db95757714365829ec`; squash merge `eb692703ab3b3d030478b539fff4496014e45c7a`; exact-head review receipt comment `5314324232`; canonical workflow `32015963930`; exact-head check `32015963768`.

**Prerequisite:** PE7-AC6-COMPATIBILITY-CLOSEOUT-1 — COMPLETE on accepted main `73fed5fedf2361ee546b831b3e87acb6f0a096ec` (PR #558 exact head `646e076cba8b349d45880dd84d4520109a11db69`; merge `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; closeout PR #559 exact head `ea89603dbd25b16f958853a5425a5088b4352134`; canonical workflow `32013680486`; exact-head `PASS`).

**Class:** `CONTRACT`

**Outcome:** Freeze a deletion manifest grouped by one canonical owner and rollback point, with zero-caller proof and compatibility disposition per item.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md.

**Exit:** Exact files/symbols/tests/docs to delete, replacement owner, zero-caller proof, negative searches, fixture/script/SDK/Dashboard/replay checks, compatibility disposition per item, and batch order.

**Stop:** Any production, recovery, replay, fixture, script, or consumer dependency remains.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze a deletion manifest owned by the AC7 architecture contract and grouped into runtime-owner evidence/rollback batches, with zero-caller proof and compatibility disposition per item; do not implement any runtime-owner deletion in this packet.
2. **Prerequisites and evidence.** Accepted main `73fed5fedf2361ee546b831b3e87acb6f0a096ec`; checked route manifest SHA `637bbc7b9c98021ce7af373fbfa04b7caa90a6024047bdd84b95dccb9ff5ac3e`; predecessor receipt PR #558 exact head `646e076cba8b349d45880dd84d4520109a11db69`; predecessor merge `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; closeout receipt PR #559 exact head `ea89603dbd25b16f958853a5425a5088b4352134`; canonical workflow `32013680486`; current-main evidence SHA `251c2c655078d715eb1dd954ffe2442426054f261d938b1b07390c3b3d9ac2f3`.
3. **Owner and paths.** Sole packet owner: the AC7 removal-manifest contract in `docs/ARCHITECTURE_BOOK.md`, projected by `docs/MODULE_MAP.md` and this route document. Runtime-owner evidence groups (not packet owners or ownership transfers): rollback group `ac7-http-compatibility-surface` covers `engine/src/http_server/routes.rs` route registration and `engine/src/http_server/handlers/product_tasks.rs::api_approve_and_output_product_task`, with authority test `product_approval_and_output_have_separate_authority_and_confirmation`; rollback group `ac7-local-store-compatibility` covers `engine/src/storage/local_product_store/product_tasks.rs::approve_and_output_product_task_for_tenant` and `::approve_and_output_product_task`, with the exact evidence/G3/recovery test functions enumerated in `docs/ARCHITECTURE_BOOK.md`; rollback group `ac7-consumer-compatibility-surface` covers Python `approve_and_output_product_task`, TypeScript `approveAndOutputProductTask`, and Dashboard `approveAndOutputProductTask`; replacements are the existing separate approve/output pairs.
4. **Frozen invariants.** Packet identity, sole manifest-contract owner, route manifest SHA `637bbc7b9c98021ce7af373fbfa04b7caa90a6024047bdd84b95dccb9ff5ac3e`, accepted-main SHA `73fed5fedf2361ee546b831b3e87acb6f0a096ec`, predecessor receipt, closeout receipt, CodeGraph call path, exact caller/test inventory, runtime-owner group boundaries, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** `docs/ARCHITECTURE_BOOK.md`: freeze the three exact AC7 rollback groups, replacements, compatibility dispositions, rollback point, and zero-caller/negative-search gate; `docs/MODULE_MAP.md`: bind the owner boundary; `docs/NEXT_DECISION.md`: bind the exact candidate inventory and successor cleanup batch order.
8. **Failure, recovery, and stop taxonomy.** Cleanup: No temporary resources created (proved by docs/ARCHITECTURE_BOOK.md:cleanup); retention: Retain canonical schemas and audit trail invariants (proved by docs/ARCHITECTURE_BOOK.md:audit); decisions: authority unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore); evaluator unchanged (docs/ARCHITECTURE_BOOK.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:rollback); schema unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore).
9. **Verification.** Candidate inventory must bind the exact route, handler, both LocalProductStore compatibility symbols, Python/TypeScript/Dashboard wrappers, one authority assertion, five evidence functions, four G3 functions, and two recovery functions. On accepted `main` `73fed5fedf2361ee546b831b3e87acb6f0a096ec`, run `codegraph explore "approve_and_output_product_task approve_and_output_product_task_for_tenant api_approve_and_output_product_task POST /api/v1/product/tasks/:task_id/approve-and-output"` and bind the route → handler → tenant-helper → compatibility-helper path in the review evidence. Contract checks: `bash scripts/check_wire_codegen_drift.sh`; `bash scripts/verify_rust_typescript_stack.sh`; `uv run --no-project python tools/check_security_baseline.py`; `uv run --no-project python scripts/check_agent_handoff.py`; `git diff --check`. The successor `PE7-AC7-CLEANUP-1` must additionally prove a zero-match fixed-string search across `engine/src`, `engine/tests`, `sdk`, `dashboard`, `scripts`, `tools`, and `tests` after deletion, run the candidate-specific Rust authority/behavior/recovery tests, the Python SDK tests, the TypeScript/Dashboard checks, and the applicable fixture/script/replay checks before closeout.
10. **Compatibility, rollback, and retention.** Revertable documentation diff with zero database migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)
11. **Exit artifact.** Evidence destinations: accepted AC6 closeout and PR #559 receipt in `docs/CURRENT_STATUS.md`; the accepted AC7 manifest, owner boundary, PR #560 receipt, and cleanup promotion are now synchronized in the canonical documents.
12. **Next action.** Execute the promoted deletion-only cleanup packet under its separate owner-scoped batch and rollback gates.

## Packet PE7-AC7-CLEANUP-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-AC7-REMOVAL-MANIFEST-1` — COMPLETE on accepted main `eb692703ab3b3d030478b539fff4496014e45c7a` (PR #560 exact head `5567c670cb0338bf3bf089db95757714365829ec`; squash merge `eb692703ab3b3d030478b539fff4496014e45c7a`; exact-head review receipt comment `5314324232`; canonical workflow `32015963930`; exact-head check `32015963768`).

**Class:** `IMPLEMENT`

**Outcome:** Delete only the accepted AC7 deprecated compatibility surface, in the manifest's three owner-scoped rollback batches, while preserving the existing separate approve/output authority paths and all audit, recovery, and idempotency semantics.

**Allowed delta:** `engine/src/http_server/routes.rs`, `engine/src/http_server/handlers/product_tasks.rs`, `engine/src/storage/local_product_store/product_tasks.rs`, `sdk/python/src/agent_control_plane_sdk/client.py`, `sdk/typescript/src/index.ts`, `dashboard/src/lib/api-client.ts`, the four enumerated Rust golden-path test files, applicable SDK/Dashboard tests, and the five canonical route/status documents for closeout synchronization. No schema, migration, new owner, provider, target, or effect change.

**Exit:** The fixed-string inventory has zero matches across tracked source, tests, SDK, Dashboard, scripts, tools, fixtures, and replay paths; each owner-scoped batch has candidate-specific behavior/recovery evidence; full applicable Rust, PostgreSQL, Python, TypeScript, Dashboard, wire-drift, security, handoff, and diff checks pass; the independent closeout packet is ready.

**Stop:** A hidden caller appears, a deletion changes authority/order/behavior, a recovery or audit invariant cannot be proved, or a single PR would consolidate owner/rollback groups beyond this exact manifest.

### Twelve-field contract

1. **Outcome and non-goals.** Perform deletion only; do not redesign the separate approve/output paths, move LocalProductStore authority, alter schemas, or add compatibility substitutes.
2. **Prerequisites and evidence.** Accepted main `eb692703ab3b3d030478b539fff4496014e45c7a`; manifest receipt PR #560 exact head `5567c670cb0338bf3bf089db95757714365829ec`; merge `eb692703ab3b3d030478b539fff4496014e45c7a`; exact-head review receipt comment `5314324232`; canonical workflow `32015963930`; exact-head check `32015963768`; route manifest SHA `637bbc7b9c98021ce7af373fbfa04b7caa90a6024047bdd84b95dccb9ff5ac3e`; pre-cleanup rollback tree is accepted main.
3. **Owner and paths.** Preserve existing runtime owners. Execute three separately evidenced batches in fixed order: consumer wrappers; HTTP route/handler and obsolete authority assertion; LocalProductStore methods plus enumerated Rust callers. This packet owns no runtime authority transfer.
4. **Frozen invariants.** The manifest candidate set, replacements, group boundaries, pre-cleanup rollback point, separate approve/output authority order, audit trail, CAS/idempotency behavior, and recovery semantics remain immutable.
5. **Only semantic delta.** Remove the exact deprecated symbols and their enumerated direct callers after each batch's zero-caller proof; do not change any neighboring canonical path.
6. **Forbidden changes.** No provider, target, effect, T3 action, schema/migration, generated-wire change, second runtime/store/controller/evaluator, or unrelated cleanup.
7. **Ordered implementation slices.** (1) remove Python/TypeScript/Dashboard composite wrappers and prove consumer checks; (2) remove the composite HTTP route/handler and obsolete authority assertion; (3) remove the two LocalProductStore compatibility methods and migrate/remove only the enumerated Rust test callers; run the scoped negative search after every batch.
8. **Failure, recovery, and stop taxonomy.** Revert the current owner-scoped batch to the pre-batch tree on any hidden caller, behavior drift, failed parity, or unproved recovery; retain canonical schemas, audit, CAS/idempotency, and separate authority paths.
9. **Verification.** Run the manifest fixed-string search with a fail-closed zero-match result after deletion; run the candidate-specific authority/evidence/G3/recovery Rust tests, SDK/Dashboard checks, applicable fixture/script/replay checks, `bash scripts/check_wire_codegen_drift.sh`, `bash scripts/verify_rust_typescript_stack.sh`, `cargo test -p engine --features pg-tests -- --test-threads=1`, Python tests, security baseline, agent handoff, and `git diff --check`.
10. **Compatibility, rollback, and retention.** The accepted pre-cleanup main tree is the rollback point; no migration or durable-data mutation is permitted; retain all canonical schemas and audit/recovery evidence.
11. **Exit artifact.** Record the exact deletion head, zero-match inventory, batch evidence, review receipt, canonical CI, merge, and refreshed main in `docs/CURRENT_STATUS.md`; the next packet independently verifies convergence and Harness identities.
12. **Next action.** Keep the implementation PR Draft while changing; run one final exact-head Standards/Spec review, mark Ready once, wait for canonical required CI, manually squash merge, refresh main, and promote `PE7-AC7-CLOSEOUT-1` only after accepted evidence is synchronized.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{
  "allowed_outputs": [
    "A provider-free change limited to the independently proved current-main allowed paths.",
    "Exact-head verification and review evidence through the existing lifecycle owners."
  ],
  "allowed_paths": [
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
    "sdk/typescript",
    "dashboard",
    "docs/ARCHITECTURE_BOOK.md",
    "docs/CURRENT_STATUS.md",
    "docs/FUTURE_ROUTE.md",
    "docs/MODULE_MAP.md",
    "docs/NEXT_DECISION.md"
  ],
  "authority_consumption_allowed": false,
  "dispatch_lane": "provider_free_repository_maintenance",
  "expected_artifacts": [
    "Deletion-only AC7 cleanup PR limited to the frozen route, handler, LocalProductStore methods, consumer wrappers, and enumerated callers.",
    "Owner-scoped batch receipts proving zero callers after each batch and a final zero-match fixed-string inventory across source, tests, SDK, Dashboard, scripts, tools, fixtures, and replay paths.",
    "Candidate-specific Rust authority/evidence/G3/recovery, Python SDK, TypeScript/Dashboard, wire-drift, security, handoff, and full-stack verification evidence.",
    "Exact-head review receipt and canonical CI evidence bound to the final cleanup PR head before merge."
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
  "goal": "Delete the frozen AC7 compatibility surface in separately evidenced owner-scoped batches while preserving canonical authority, audit, recovery, and rollback invariants.",
  "ordered_steps": [
    "consumer source paths: remove only the deprecated Python, TypeScript, and Dashboard composite wrappers; run consumer checks and the scoped negative search.",
    "HTTP source paths: remove only the deprecated route/handler and obsolete authority assertion; run route/authority checks and the scoped negative search.",
    "LocalProductStore and Rust test paths: remove only the two compatibility methods and enumerated direct callers; run behavior/recovery checks and the scoped negative search.",
    "canonical docs: synchronize the exact cleanup head, evidence, rollback, and next closeout route without changing the manifest."
  ],
  "packet_id": "PE7-AC7-CLEANUP-1",
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
    "PR #560 exact head `5567c670cb0338bf3bf089db95757714365829ec`; squash merge `eb692703ab3b3d030478b539fff4496014e45c7a`; exact-head review receipt comment `5314324232`; canonical workflow `32015963930`; exact-head check `32015963768`"
  ],
  "prerequisites": [
    "PE7-AC7-REMOVAL-MANIFEST-1"
  ],
  "private_paths_allowed": false,
  "promotion_evidence_sha256": "637bbc7b9c98021ce7af373fbfa04b7caa90a6024047bdd84b95dccb9ff5ac3e",
  "read_paths": [
    "START_HERE.md",
    "AGENTS.md",
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
    "sdk/typescript",
    "dashboard",
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
    "test \"$(git rev-parse origin/main)\" = \"eb692703ab3b3d030478b539fff4496014e45c7a\" && codegraph explore \"approve_and_output_product_task approve_and_output_product_task_for_tenant api_approve_and_output_product_task POST /api/v1/product/tasks/:task_id/approve-and-output\"",
    "bash scripts/check_wire_codegen_drift.sh",
    "bash scripts/verify_rust_typescript_stack.sh",
    "uv run --no-project python tools/check_security_baseline.py",
    "uv run --no-project python scripts/check_agent_handoff.py",
    "git diff --check",
    "PE7-AC7-CLEANUP-1 successor gate: repeat the fixed-string search with zero matches across tracked source, SDK, Dashboard, fixture, script, replay, and authority-test paths; run the candidate-specific Rust, Python SDK, TypeScript/Dashboard, fixture/script/replay checks before closeout."
  ],
  "verification_family": "source_focused_full",
  "worker_tier": "T1"
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
