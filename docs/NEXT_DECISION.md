# Next Decision

Last updated: 2026-08-23.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-HE-EC3-CONTRACT-1` is complete. The current window is `PE7-HE-EC3-INSTRUMENTATION-1`: implement provider-free, immutable lifecycle-cost observations and a read-only operator projection through the existing Harness-Evolution, execution-usage, ProductTask terminal-evidence, scorecard, and `LocalProductStore` owners. This packet records reconciliation inputs only; it grants no reservation, spend, admission, candidate execution, or external-effect authority.

## Authoritative Forward Order

```text
[window: PE7-HE-EC3-INSTRUMENTATION-1 — READY_FOR_EXECUTION, provider-free; capture, persist, and project immutable lifecycle-cost evidence]
[successor: PE7-HE-EC3-ENFORCEMENT-1 — BLOCKED_PREREQUISITE, provider-free; reserve and reconcile equal lifecycle envelopes through existing authorities]
```

## Active Routing

1. `PE7-HE-EC3-INSTRUMENTATION-1` — `READY_FOR_EXECUTION`

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

## Packet PE7-HE-EC3-INSTRUMENTATION-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-HE-EC3-CONTRACT-1`

**Class:** `IMPLEMENT`

**Outcome:** Capture, normalize, persist, and project immutable lifecycle-cost observations for the accepted EC3 ontology, joining exact candidate/evaluation identity with available ProductTask terminal evidence, `ExecutionUsageEventV1`, and scorecard/VDE source digests through existing owners. Produce reconciliation inputs and an operator read model without enforcing or spending a budget.

**Allowed delta:** Only `docs/ARCHITECTURE_BOOK.md`, `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md`, `engine/src/harness_evolution.rs`, `engine/src/storage/local_product_store/harness_evolution.rs`, `engine/src/storage/local_product_store/schema.rs`, `engine/src/storage/local_product_store/migrations.rs`, `engine/src/storage/local_product_store/integrity.rs`, `engine/src/storage/local_product_store/pg_backend/migrations.rs`, `engine/src/http_server/handlers/operator_evidence.rs`, `engine/tests/test_data_operations.rs`, `engine/tests/test_product_golden_path_g1.rs`, `engine/tests/test_recursive_execution.rs`, `engine/tests/test_pg_integration.rs`, and `tests/test_session_context.py`. The three additional engine test paths are compatibility-only v37-to-v38 schema-version/count assertion updates; they add no runtime behavior. Add versioned observation/bundle/read-model types and deterministic normalization, the append-only v38 `harness_evolution_ec3_lifecycle_cost_records` table and indexes, both-backend migration/rollback/integrity/persistence/query parity, the existing read-only operator-evidence projection, focused negative tests, and the smallest canonical-document synchronization. `engine/src/execution_usage/mod.rs`, ProductTask terminal evidence, scorecards, and VDE remain read-only source owners unless a separately recorded `DECISION_REQUIRED` expands the accepted contract.

**Exit:** The production owner path records all explicit observations—including zero and unavailable states—under canonical units, rejects ambiguous/untrusted/duplicate-conflicting evidence, survives replay/restart, exposes a redacted candidate/task/run operator query, and passes SQLite/PostgreSQL migration, rollback, and behavioral parity. Every required EC3 phase/dimension is either evidenced or explicitly missing/ineligible; no fixture-only side path counts.

**Stop:** Any source owner, schema field, logical identity, join key, canonical-unit conversion, missingness rule, rollback behavior, or redaction boundary cannot be proved from accepted `main`; unavailable becomes zero; caller/model self-report becomes evidence; raw/private material leaks; a fixture bypasses the production owner path; observations become mutable or double-charged; or a second store, usage, evidence, evaluator, budget, or runtime owner appears.

### Twelve-field contract

1. **Outcome and non-goals.** Implement provider-free observation, normalization, immutable persistence, and read-only projection for the accepted EC3 lifecycle-cost contract. No reservation, reconciliation mutation, envelope enforcement, candidate execution, Provider call, target write, or usability/self-improvement claim.
2. **Prerequisites and evidence.** EC3-CONTRACT COMPLETE: PR #603 exact head `c1c1c23eb68d11f38fd85623f412dd13b5c867e1`; squash merge `d1b939865e5dcf3b11093e1e6932078e55068054`; exact-head review receipt comment `5386543180` with Spec companion `5386543304`; canonical workflow `32646001459`; exact-head check `32646001422`.
3. **Owners and paths.** `engine/src/harness_evolution.rs` owns EC3 lifecycle types and normalization; `engine/src/execution_usage/mod.rs`, ProductTask terminal evidence, native scorecards, and VDE remain source owners; `engine/src/storage/local_product_store/{harness_evolution.rs,schema.rs,migrations.rs,integrity.rs,pg_backend/migrations.rs}` remains the sole persistence/migration owner; `engine/src/http_server/handlers/operator_evidence.rs` remains the read-only projection owner. The larger-than-six read set is a T2 exception required to prove one versioned schema and SQLite/PostgreSQL/operator parity, not permission to create another owner.
4. **Frozen invariants and schema.** Additive schema v38 contains one append-only `harness_evolution_ec3_lifecycle_cost_records` table. Each row binds `observation_key`, derived `record_id`, EC3 `contract_id`, `candidate_id`, optional `evaluation_id`/`product_task_id`/`run_id`, `attempt_id`, one frozen phase, one frozen dimension and canonical integer amount (nullable only for explicit `unavailable`), trust source, source schema/version and non-sensitive source digest, terminal/failure class, `record_sha256`, redacted `body_json`, and `created_at`; indexes serve candidate plus task/run reads. Logical replay is byte-identical and idempotent; the same observation key with different bytes fails. Tokens/calls use integer counts, Provider cost uses microunits, and wall/compute/human effort use integer milliseconds. Explicit zero requires measured-direct or deterministic-derived evidence; unavailable never carries an amount and makes a required aggregate ineligible. Failed, rejected, cancelled, repair, recovery, and outcome-unknown attempts remain separately attributable.
5. **Only semantic delta.** Add `LifecycleCostObservationV1`, a sealed source-bound bundle/summary, deterministic canonical-unit and missingness normalization, v38 append-only persistence/query, and a redacted operator projection. Production-path fixtures must call the same normalizer and Store APIs. Observation records are immutable reconciliation inputs, not budget reservations or spend receipts.
6. **Forbidden changes.** No change to `ExecutionUsageEventV1`, ProductTask terminal-evidence, scorecard/VDE, evaluator, admission, spend, scheduler, runtime, target-output, or authority semantics; no raw prompt/output/transcript, credential, private path, unredacted repository content, floating-point persisted amount, mutable update/delete path, second table family, second Store/evidence owner, Provider call, ENABLE, Level-1, or successor implementation.
7. **Ordered work cards and checkpoints.** Card A: types/normalizer plus positive and adversarial unit tests; checkpoint proves every phase/dimension, unit, zero/unavailable, source, digest, duplicate-key, and redaction rule before Store work. Card B: v38 SQLite/PostgreSQL schema, migration, insert/read/replay/conflict/restart/integrity, non-empty rollback refusal, and parity tests; checkpoint passes before HTTP projection work. Card C: existing operator-evidence projection and production-path integration fixtures joining candidate/evaluation and optional ProductTask/run source digests; checkpoint proves missing/ambiguous/failure/recovery views and no raw evidence. Then run full applicable checks, synchronize accepted status/route, and stop before enforcement.
8. **Failure taxonomy.** `ec3_cost_source_untrusted`, `ec3_cost_unit_invalid`, `ec3_cost_amount_invalid`, `ec3_cost_zero_unproved`, `ec3_cost_unavailable_has_amount`, `ec3_cost_required_missing`, `ec3_cost_join_ambiguous`, `ec3_cost_observation_conflict`, `ec3_cost_record_tamper`, `ec3_cost_private_evidence`, `ec3_cost_schema_drift`, `ec3_cost_rollback_nonempty`, and `ec3_cost_owner_violation`; all fail closed and preserve already accepted rows.
9. **Verification.** `cargo fmt --all -- --check`; focused `ec3_lifecycle_cost` unit/store/operator tests; migration-upgrade, empty rollback, non-empty rollback-refusal, restart/replay/conflict, integrity, and SQLite/PostgreSQL parity tests; `cargo clippy -p engine --all-targets --all-features -- -D warnings`; `scripts/ci/run_rust_tests.py`; `cargo test -p engine --features pg-tests -- --test-threads=1`; `bash scripts/verify_rust_typescript_stack.sh`; `bash scripts/check_wire_codegen_drift.sh`; security baseline; handoff; and `git diff --check`.
10. **Compatibility and rollback.** v38 is additive and existing v37 reads/behavior remain byte-compatible. A tested v38-to-v37 rollback may drop the new table/indexes only while empty; any persisted observation blocks downgrade and requires an explicit recovery/export decision rather than deletion. Before external installation or durable data, revert the instrumentation PR. This packet has no Provider or target effect to compensate.
11. **Exit artifact.** Accepted v38 lifecycle-cost observation schema and Store API, source-bound normalization, redacted operator summary, exact negative/error map, SQLite/PostgreSQL parity evidence, rollback evidence, and a current capsule that identifies enforcement as blocked successor.
12. **Next action.** Promote `PE7-HE-EC3-ENFORCEMENT-1`; do not start it in this packet.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-HE-EC3-INSTRUMENTATION-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Capture, normalize, persist, and project immutable lifecycle-cost observations through existing owners without enforcing or spending a budget.","allowed_paths":["docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/harness_evolution.rs","engine/src/storage/local_product_store/harness_evolution.rs","engine/src/storage/local_product_store/schema.rs","engine/src/storage/local_product_store/migrations.rs","engine/src/storage/local_product_store/integrity.rs","engine/src/storage/local_product_store/pg_backend/migrations.rs","engine/src/http_server/handlers/operator_evidence.rs","engine/tests/test_data_operations.rs","engine/tests/test_product_golden_path_g1.rs","engine/tests/test_recursive_execution.rs","engine/tests/test_pg_integration.rs","tests/test_session_context.py"],"read_paths":["docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/harness_evolution.rs","engine/src/execution_usage/mod.rs","engine/src/storage/local_product_store/harness_evolution.rs","engine/src/storage/local_product_store/schema.rs","engine/src/storage/local_product_store/migrations.rs","engine/src/storage/local_product_store/integrity.rs","engine/src/storage/local_product_store/pg_backend/migrations.rs","engine/src/storage/local_product_store/product_tasks.rs","engine/src/storage/local_product_store/native_scorecard_artifacts.rs","engine/src/http_server/handlers/operator_evidence.rs","engine/tests/test_data_operations.rs","engine/tests/test_product_golden_path_g1.rs","engine/tests/test_recursive_execution.rs","engine/tests/test_pg_integration.rs","tests/test_session_context.py"],"allowed_outputs":["Versioned EC3 lifecycle-cost observation, bundle, and read-model types.","Additive LocalProductStore schema v38 with SQLite/PostgreSQL persistence, query, migration, integrity, and rollback evidence.","Redacted existing-owner operator lifecycle-cost projection and production-path fixtures."],"prerequisites":["PE7-HE-EC3-CONTRACT-1"],"prerequisite_receipts":["PE7-HE-EC3-CONTRACT-1 COMPLETE: PR #603 exact head `c1c1c23eb68d11f38fd85623f412dd13b5c867e1`; squash merge `d1b939865e5dcf3b11093e1e6932078e55068054`; exact-head review receipt comment `5386543180` with Spec companion `5386543304`; canonical workflow `32646001459`; exact-head check `32646001422`"],"forbidden_changes":["Do not enforce, reserve, reconcile-mutably, spend, admit, schedule, or execute a candidate.","Do not change ExecutionUsageEventV1, ProductTask terminal-evidence, scorecard/VDE, evaluator, budget, runtime, or authority semantics.","Do not create a second Store, usage, evidence, evaluator, budget, runtime, workspace, output, audit, or rollback owner.","Do not persist raw prompts, outputs, transcripts, credentials, private paths, unredacted repository content, or floating-point amounts.","Do not call a Provider, write a target, ENABLE the laboratory, or start PE7-HE-EC3-ENFORCEMENT-1."],"ordered_steps":["Card A: implement versioned observation/bundle/read-model types and deterministic canonical-unit, missingness, identity, digest, redaction, replay, and conflict validation; pass focused positive/adversarial tests.","Card B: implement additive v38 SQLite/PostgreSQL schema, append-only Store persistence/query, migrations, integrity inventory, empty rollback, non-empty rollback refusal, replay/restart/conflict behavior, and parity tests; pass its checkpoint before HTTP work.","Card C: extend the existing operator-evidence projection and production-path fixtures to join candidate/evaluation plus optional ProductTask/run source digests; prove missing, ambiguous, failed, recovery, zero, and unavailable views without raw evidence.","Run all applicable focused/full checks, synchronize the smallest canonical documents, and stop before enforcement."],"verification":["cargo fmt --all -- --check","cargo test -p engine --lib ec3_lifecycle_cost -- --test-threads=1","cargo test -p engine --lib local_product_store::harness_evolution -- --test-threads=1","cargo test -p engine --lib operator_evidence -- --test-threads=1","cargo clippy -p engine --all-targets --all-features -- -D warnings","scripts/ci/run_rust_tests.py","cargo test -p engine --features pg-tests -- --test-threads=1","bash scripts/verify_rust_typescript_stack.sh","bash scripts/check_wire_codegen_drift.sh","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"rollback":"Schema v38 is additive. Before durable observations, revert the PR. Empty v38 downgrades to v37 by dropping only the new table/indexes; non-empty v38 refuses downgrade and requires an explicit recovery/export decision.","pause_gates":["Do not start Card B until Card A focused tests pass on the same head.","Do not start Card C until v38 SQLite/PostgreSQL migration, rollback, integrity, replay, and parity tests pass on the same head.","Stop with DECISION_REQUIRED if any accepted source owner, join identity, schema field, redaction, rollback, or parity rule is ambiguous.","Stop before PE7-HE-EC3-ENFORCEMENT-1."],"expected_artifacts":["engine/src/harness_evolution.rs lifecycle-cost observation and normalization types","LocalProductStore v38 lifecycle-cost table and Store methods with both-backend tests","read-only operator lifecycle-cost projection with redaction and missingness tests"],"forbidden_next_actions":["Do not start PE7-HE-EC3-ENFORCEMENT-1.","Do not start PE7-HE-CL0-PILOT-1.","Do not start any Level-1, recursive, Meta, R4, R5, or R6 packet."],"worker_tier":"T2","known_store_mutations":["Additive LocalProductStore schema v38 table harness_evolution_ec3_lifecycle_cost_records plus candidate and task/run read indexes; append-only observations only; tested empty rollback and non-empty rollback refusal."]}
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
