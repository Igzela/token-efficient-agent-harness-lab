# Next Decision

Last updated: 2026-08-24.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-HE-EC3-INSTRUMENTATION-1` is complete. The current window is `PE7-HE-EC3-ENFORCEMENT-1`: bind the accepted lifecycle-cost contract to existing admission and spend owners, enforce reservation-before-execution and exact terminal reconciliation, and prove one default-off verified-delivery loop without a live effect.

## Authoritative Forward Order

```text
[completed: PE7-HE-EC3-INSTRUMENTATION-1 — COMPLETE, provider-free; capture, persist, and project immutable lifecycle-cost evidence]
[window: PE7-HE-EC3-ENFORCEMENT-1 — READY_FOR_EXECUTION, provider-free; reserve and reconcile equal lifecycle envelopes through existing authorities]
```

## Active Routing

1. `PE7-HE-EC3-ENFORCEMENT-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-BENCHMARK-PREFLIGHT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #588 exact head `c806f75c5910b117c3cf7e44ad1c6a6503e48ddd`; squash merge `1569c70e9f2034bb4f7bc5ccbc24d889b66645ab`.

## Completed (PE7-CWS-BENCHMARK-RUN-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #589 exact head `0f9cad12a850a7ed2ffcc823ebd2da29318c5ae6`; squash merge `84b1933bc3d9e657acae94d9e5f14810c0651917`; exact-head review comments `5347630853` and `5347631083`; canonical workflow `32298813456`; exact-head check `32298813444`; `executed=false`; `provider_posts=0`.

## Completed (PE7-CWS-ANALYSIS-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #590 exact head `da09ea576154e55e532d2de5477972f2c5c516d5`; squash merge `1544c8d0a3f1b196fdb4b560759609662cd5f432`; exact-head review comments `5347818781` and `5347818993`; canonical workflow `32301497907`; exact-head check `32301497898`; `INSUFFICIENT_DEFAULT_OFF`; active Harness `84b1933bc3d9e657acae94d9e5f14810c0651917`.

## Completed (PE7-HE-EC1-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #591 exact head `50661a622c19e1f6da1f934a43bcbbaa4b52a003`; squash merge `e116e212ed043d773e215f2ba029e5b2f1763e4d`; exact-head review comments `5348443154` and `5348443354`; canonical workflow `32306087501`.

## Completed (PE7-HE-EC1-IDENTITY-LINEAGE-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #592 exact head `155fa749effdcd790fb954eefcf64d12790d21b6`; squash merge `3dc2d3b12fbb95ec2b26220681cba5ad7547c6d2`; exact-head review comments `5348823217` and `5348823396`; canonical workflow `32309602816`.

## Completed (PE7-HE-EC1-CAUSAL-MANIFEST-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #593 exact head `c00f24dac433d9b3fc23f5b0df746c89442097dd`; squash merge `b2fa400395a0502bf52ea5fd9468af5830766422`; exact-head review comments `5349023567` and `5349023702`; canonical workflow `32311374839`.

## Completed (PE7-HE-EC1-MUTATION-REGISTRY-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #594 exact head `b3199736d85312083c45a3522211ae086f5fe756`; squash merge `b970226181957de98859f26f03db3bf101b1f8a0`; exact-head review comments `5349266593` and `5349266718`; canonical workflow `32313718374`.

## Completed (PE7-HE-EC2-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #595 exact head `e0585701dec206fca5645299d65cbb3341257008`; squash merge `f996ded631f12f74f42528c70e76ccf0f040bdfd`; exact-head review comments `5349652629` and `5349652752`; canonical workflow `32317253205`.

## Completed (PE7-HE-EC2-HOLDOUT-SEAL-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #596 exact head `cffd49edfc36fe602cc311f025367cadb15a425a`; squash merge `5c367b85d79f680b5f76b7aa4f2f1656c0a460ae`; exact-head review comments `5349963606` and `5349963725`; canonical workflow `32320235684`.

## Completed (PE7-HE-EC2-SENTINEL-CONFORMANCE-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #597 exact head `4e39a52a265d4a9e3a6902c68da142b424b15c36`; squash merge `dbe20eccb4980e595958d615cf937ba34cfdaed2`; exact-head review comments `5350149695` and `5350149805`; canonical workflow `32321977265`.

## Completed (PE7-HE-EC2-PREDICTION-OUTCOME-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #600 exact head `0ccdbefa59e18b241cba7cb6f26f3d267608a9a9`; squash merge `ac2b2f640406ca766b0cd567c2782e426d8dad2b`; exact-head review comments `5385499271` and `5385499345`; canonical workflow `32633510108`.

## Completed (PE7-HE-EC3-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #603 exact head `c1c1c23eb68d11f38fd85623f412dd13b5c867e1`; squash merge `d1b939865e5dcf3b11093e1e6932078e55068054`; exact-head review receipt comment `5386543180` with Spec companion `5386543304`; canonical workflow `32646001459`; exact-head check `32646001422`.

## Completed (PE7-HE-EC3-INSTRUMENTATION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #608 exact head `36474545563bd1b91015d4e3f2005f12dd43bde9`; squash merge `789b7dba9afdd5e1e6e41d191ebcbbfa933b2c12`; exact-head review receipt comment `5390448906`; canonical workflow `32687392603`; exact-head check `32687392611`.

## Packet PE7-HE-EC3-ENFORCEMENT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-HE-EC3-INSTRUMENTATION-1` — COMPLETE on accepted main `789b7dba9afdd5e1e6e41d191ebcbbfa933b2c12`.

**Class:** `IMPLEMENT`

**Outcome:** Enforce equal per-arm and global lifecycle envelopes through existing admission/spend owners and complete one default-off verified-delivery closed loop for a single admitted Harness.

**Allowed delta:** `docs/*.md`, `engine/src/harness_evolution.rs`, `engine/src/storage/local_product_store/*.rs`, `engine/src/storage/local_product_store/pg_backend/migrations.rs`, `engine/src/http_server/handlers/operator_evidence.rs`, `engine/tests/*.rs`, and `tests/test_session_context.py`; provider-free reservation/reconciliation, admission/pause/terminal integration, deterministic fixtures, and redacted operator queries through existing ProductTask, budget, runtime, verification, recovery, and Store owners; no second Harness or live effect.

**Exit:** Happy, insufficient-budget, overrun, missing-usage, failure, cancellation, late-write, outcome-unknown, restart, cleanup, reconciliation, rollback, and parity fixtures prove one source-bound task reaches one truthful terminal outcome with joined delivery and full cost.

**Stop:** Token equality replaces lifecycle budget, repair/rescue cost is lost, an effect starts without reservation, outcome unknown becomes retryable/success, enforcement bypasses an owner, or a second scheduler/store/budget owner appears.

### Twelve-field contract

1. **Outcome and non-goals.** Provider-free lifecycle-envelope enforcement and exact-once reconciliation for one default-off verified-delivery path; no Provider call, target write, live effect, or later packet.
2. **Prerequisites and evidence.** EC3 instrumentation COMPLETE on accepted main: PR #608 exact head `36474545563bd1b91015d4e3f2005f12dd43bde9`; squash merge `789b7dba9afdd5e1e6e41d191ebcbbfa933b2c12`; workflow `32687392603`; review `5390448906`; exact-head `32687392611`.
3. **Owners and paths.** Existing ProductTask, budget/spend, runtime, verification, recovery, terminal-evidence, LocalProductStore, and operator-evidence owners remain authoritative; paths are capsule-bound.
4. **Frozen invariants.** Reserve the complete per-arm/global envelope before execution; reconcile actual lifecycle cost exactly once; preserve failure, cancellation, repair, rescue, recovery, missing-usage, late-write, and outcome-unknown attribution.
5. **Only semantic delta.** Bind EC3 contract to existing admission/spend/terminal paths and add minimum provider-free default-off integration/parity evidence.
6. **Forbidden changes.** No token-only budget, caller/model self-report, implicit zero, outcome-unknown retry, Provider, target, live effect, second owner, or successor implementation.
7. **Ordered work cards.** Reservation; terminal usage/reconciliation; failure/recovery/rollback; default-off fixtures; SQLite/PostgreSQL/restart/concurrency parity; redacted projection; full checks; stop before CL0.
8. **Failure taxonomy.** Insufficient/overrun envelope, missing usage, reservation race, duplicate terminalization, late write, cancellation, cleanup failure, unknown outcome, conflict, rollback refusal, owner violation.
9. **Verification.** Focused lifecycle-budget, ProductTask/recovery, SQLite/PostgreSQL, full-stack, security, handoff, diff, exact-head review, and canonical CI checks.
10. **Compatibility and rollback.** Reuse v38 and existing Store owners; any additive state needs backend parity and recovery tests. Revert before live use; retain unknown durable state for explicit recovery.
11. **Exit artifact.** Existing-owner reservation/reconciliation, default-off one-task evidence, failure/recovery/rollback/parity tests, and redacted operator model.
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then promote the next packet; do not start CL0 here.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-HE-EC3-ENFORCEMENT-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Enforce equal per-arm and global lifecycle envelopes through existing admission and spend owners with exact reservation and terminal reconciliation for one default-off verified-delivery loop.","allowed_paths":["docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/harness_evolution.rs","engine/src/storage/local_product_store/harness_evolution.rs","engine/src/storage/local_product_store/costs.rs","engine/src/storage/local_product_store/product_tasks.rs","engine/src/storage/local_product_store/workflow_runs.rs","engine/src/storage/local_product_store/schema.rs","engine/src/storage/local_product_store/migrations.rs","engine/src/storage/local_product_store/integrity.rs","engine/src/storage/local_product_store/pg_backend/migrations.rs","engine/src/http_server/handlers/operator_evidence.rs","engine/tests/test_product_golden_path_g1.rs","engine/tests/test_product_golden_path_recovery.rs","engine/tests/test_pg_integration.rs","engine/tests/test_data_operations.rs","engine/tests/test_recursive_execution.rs","tests/test_session_context.py"],"read_paths":["docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/harness_evolution.rs","engine/src/execution_usage/mod.rs","engine/src/storage/local_product_store/harness_evolution.rs","engine/src/storage/local_product_store/costs.rs","engine/src/storage/local_product_store/budget_pause_decisions.rs","engine/src/storage/local_product_store/budget_evidence_artifacts.rs","engine/src/storage/local_product_store/product_tasks.rs","engine/src/storage/local_product_store/workflow_runs.rs","engine/src/storage/local_product_store/schema.rs","engine/src/storage/local_product_store/migrations.rs","engine/src/storage/local_product_store/integrity.rs","engine/src/storage/local_product_store/pg_backend/migrations.rs","engine/src/http_server/handlers/operator_evidence.rs","engine/tests/test_product_golden_path_g1.rs","engine/tests/test_product_golden_path_recovery.rs","engine/tests/test_pg_integration.rs","engine/tests/test_data_operations.rs","engine/tests/test_recursive_execution.rs","tests/test_session_context.py"],"allowed_outputs":["Existing-owner reservation and exact-once lifecycle reconciliation.","Default-off one-task verified-delivery path with fail-closed budget, usage, failure, cancellation, recovery, and outcome-unknown behavior.","SQLite/PostgreSQL parity and redacted operator evidence."],"prerequisites":["PE7-HE-EC3-INSTRUMENTATION-1"],"prerequisite_receipts":["PE7-HE-EC3-INSTRUMENTATION-1 COMPLETE: PR #608 exact head `36474545563bd1b91015d4e3f2005f12dd43bde9`; squash merge `789b7dba9afdd5e1e6e41d191ebcbbfa933b2c12`; exact-head review receipt comment `5390448906`; canonical workflow `32687392603`; exact-head check `32687392611`"],"forbidden_changes":["No Provider call, target write, enable, or live effect.","No bypass or second admission, spend, scheduler, runtime, evaluator, Store, audit, or rollback owner.","No token-only budget, implicit zero, dropped repair cost, or retry of outcome-unknown work.","Do not start PE7-HE-CL0-PILOT-1 or later effects."],"ordered_steps":["Bind the EC3 contract to existing ProductTask admission/spend owners and reserve the complete envelope before execution.","Join terminal usage exactly once across success, failure, cancellation, repair, recovery, late-write, missing-usage, and outcome-unknown states.","Add default-off golden-path and SQLite/PostgreSQL/restart/concurrency parity fixtures through the real owner path.","Expose redacted metadata, run full checks/review, and stop before any effect."],"verification":["cargo fmt --all -- --check","cargo test -p engine --lib ec3_lifecycle_budget -- --test-threads=1","cargo test -p engine --test test_product_golden_path_g1 -- --test-threads=1","cargo test -p engine --test test_product_golden_path_recovery -- --test-threads=1","cargo test -p engine --features pg-tests -- --test-threads=1","scripts/ci/run_rust_tests.py","bash scripts/verify_rust_typescript_stack.sh","bash scripts/check_wire_codegen_drift.sh","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"rollback":"Revert before live use. Reservation and reconciliation stay transactional/idempotent through existing Store owners; outcome-unknown or non-empty durable state is retained for explicit recovery, never deleted or retried speculatively.","pause_gates":["Execution requires a committed complete per-arm/global reservation.","Preserve outcome-unknown when terminal evidence, usage, lease, or cleanup cannot be proved.","DECISION_REQUIRED if existing owners cannot enforce the boundary without a second owner.","Stop before CL0 or any Provider/target effect."],"expected_artifacts":["Existing-owner reservation and exact-once reconciliation with fail-closed terminals.","Default-off one-task fixtures covering budget, usage, failure, cancellation, recovery, restart, late-write, and rollback.","SQLite/PostgreSQL parity and redacted operator evidence."],"forbidden_next_actions":["Do not start PE7-HE-CL0-PILOT-1.","Do not start Level-1, recursive, Meta, R4, R5, R6, or dashboard packets."],"worker_tier":"T1","known_store_mutations":["Reuse existing ProductTask, budget, spend, attempt, terminal-evidence, audit, and rollback owners; add no second store or authority family."]}
-->

## Common Execution Protocol

- `READY_FOR_EXECUTION` packets require a valid dispatch capsule; blocked or decision-required packets carry no executable capsule.
- Refresh accepted `main`, the current packet, exact PR head, CI, and review receipts before each transition.
- Keep a changing PR Draft; run focused/local full checks, then one final stable-head Standards/Spec review, Ready, canonical exact-head CI, manual squash merge, and main refresh.
- No Provider call, credential-value read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner in this packet.
- Unknown, stale, missing, or conflicting evidence remains fail-closed; never retry an effect whose outcome may be unknown.

## Hard Stops

- `DECISION_REQUIRED` when a caller, owner, path, semantics, compatibility fact, rollback, or evidence cannot be re-proved from accepted `main`.
- No Provider call, credential-value read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner.
- Unknown or possibly executed effects are never treated as success or retried.
## Future Route Boundary

`docs/FUTURE_ROUTE.md` is routing-only. RUN-1 remains a retained live-ready blocker.
