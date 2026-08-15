# Next Decision

Last updated: 2026-08-15.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze, the AC2 typed contract, and the fail-closed AC2 boundary repair are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The current provider-free packet is the mechanical AC2 caller migration; it does not add an owner or alter public compatibility.

## Authoritative Forward Order

```text
[window: PE7-AC2-CALLER-MIGRATION-1 — READY_FOR_EXECUTION, provider-free]
```

## Active Routing

1. `PE7-AC2-CALLER-MIGRATION-1` — `READY_FOR_EXECUTION`

## Packet PE7-AC2-CALLER-MIGRATION-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-AC2-BOUNDARY-REPAIR-1` — COMPLETE; accepted receipt is in `docs/CURRENT_STATUS.md`.

**Class:** `IMPLEMENT`

**Outcome:** Migrate the enumerated ProductTask verification and managed-review callers from ad hoc process-outcome success checks to the canonical `ProcessBoundaryMapping`, so known success requires a started effect and unknown evidence remains non-success and non-retryable.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `engine/src/storage/local_product_store/product_tasks.rs`, `engine/src/storage/local_product_store/managed_acceptance.rs`, `engine/tests/test_product_golden_path_g2.rs`, `engine/tests/test_product_golden_path_evidence.rs`, and `engine/tests/test_local_product_store.rs`; reuse `engine/src/node_executor.rs` as the sole mapping owner.

**Exit:** Every in-scope production success gate uses `ProcessBoundaryMapping::is_known_success()`; missing, ambiguous, or unknown process evidence fails closed; `process_outcome.v1` and public/wire/schema contracts are unchanged; focused and full checks pass.

**Stop:** A caller needs semantics not represented by the accepted mapping, a public or schema change becomes necessary, an unknown result would be retried or accepted, or the change would create another runtime/store/authority owner.

### Bounded execution contract

1. **Goal and non-goals.** Replace only the enumerated raw `state == exited` plus `exit_code == 0` success gates in ProductTask verification and managed deterministic review. Do not redesign `ProcessOutcome`, add AC1 supervision, migrate legacy advisory dispatch, or change provider behavior.
2. **Accepted binding.** Accepted main is `b75cd81620ed51aefce5d245855cf00f1bb6385b`; predecessor receipt is PR #475 exact head `e89a24bd6776282bbd52ee72cc5be8ecc66acbc2`, merge `b75cd81620ed51aefce5d245855cf00f1bb6385b`, exact-head `PASS`, canonical workflow `31882791484`.
3. **Owners and paths.** `engine/src/node_executor.rs` remains the canonical mapping owner. Caller changes are limited to `engine/src/storage/local_product_store/product_tasks.rs` and `engine/src/storage/local_product_store/managed_acceptance.rs`; focused coverage stays with the existing ProductTask verification and managed-review tests.
4. **Frozen invariants.** `ProcessOutcome` construction, `process_outcome.v1` serialization, ProductTask lifecycle/CAS/audit/recovery, usage reconciliation, provider credential handling, target boundaries, and `LocalProductStore` authority remain unchanged.
5. **Only semantic delta.** Success is accepted only from `ProcessBoundaryMapping { effect: Started, outcome: KnownSuccess }`. `NotStarted + KnownFailure` is failure; `Unknown` is neither success nor retry authorization.
6. **Forbidden changes.** No new runtime, scheduler, store, journal, queue, lease, evaluator, authority owner, public API, wire/schema migration, Provider call, target write, T3/EFFECT action, auto-merge, or speculative retry.
7. **Ordered implementation slice.** Update the ProductTask verification failure/status helper and terminal verification receipt gate; update the managed deterministic reviewer gate; add focused negative coverage for unknown, not-started, and contradictory mappings; preserve existing evidence projections.
8. **Failure and recovery.** A failed PR leaves accepted `main` unchanged. Unknown or possibly executed effects remain in the existing reconciliation/compensation path and are never replayed because a caller now sees `Unknown`.
9. **Verification.** Run `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets --all-features -- -D warnings`, focused ProductTask/managed-review tests, `cargo test -p engine`, `bash scripts/check_wire_codegen_drift.sh`, `uv run --no-project python tools/check_security_baseline.py`, `uv run --no-project python scripts/check_agent_handoff.py`, and `git diff --check`; canonical CI remains required.
10. **Compatibility and rollback.** No serialized field, public API, or migration changes. Roll back only with a replacement PR that preserves fail-closed mapping and does not restore raw success checks.
11. **Evidence destination.** Record exact caller paths, focused/full checks, stable-head review, canonical CI, merge, and refreshed-main evidence in `docs/CURRENT_STATUS.md`.
12. **Next action.** Keep the implementation PR Draft while changing; perform one final stable-head Standards/Spec review, mark Ready once, wait for canonical exact-head CI, manually squash merge, refresh `main`, then promote AC3.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"accepted_binding_source":"accepted main b75cd81620ed51aefce5d245855cf00f1bb6385b; predecessor PE7-AC2-BOUNDARY-REPAIR-1 receipt PR #475 exact head e89a24bd6776282bbd52ee72cc5be8ecc66acbc2, merge b75cd81620ed51aefce5d245855cf00f1bb6385b, canonical workflow 31882791484","allowed_outputs":["A provider-free caller migration limited to the independently proved current-main allowed paths.","Exact-head verification and review evidence through the existing lifecycle owners."],"allowed_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","engine/src/storage/local_product_store/product_tasks.rs","engine/src/storage/local_product_store/managed_acceptance.rs","engine/tests/test_product_golden_path_g2.rs","engine/tests/test_product_golden_path_evidence.rs","engine/tests/test_local_product_store.rs"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["Record caller-migration implementation and verification evidence in docs/CURRENT_STATUS.md under AC2."],"external_effect_limit":0,"forbidden_changes":["Do not add AC1 ProcessSupervisor or a second runtime/executor/store/authority owner.","Do not change public wire/schema contracts or create a migration.","Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."],"forbidden_next_actions":["Do not treat unknown effect or outcome state as success or retryable.","Do not use FUTURE_ROUTE static paths as current-main authority.","Do not create a second controller, ledger, queue, lease, store, workflow owner.","Do not retry a possibly executed external effect whose outcome is unknown."],"goal":"Migrate enumerated ProductTask verification and managed-review callers to the accepted ProcessBoundaryMapping without changing wire/schema or authority ownership.","known_store_mutations":["Existing ProductTask verification evidence/status projection only; no new store mutation or owner."],"ordered_steps":["Replace the in-scope raw process success checks with the canonical typed mapping; add focused negative coverage; run local focused/full verification; update accepted status at closeout."],"packet_id":"PE7-AC2-CALLER-MIGRATION-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop when an owner, caller, state, reason, test, path, operation, destination, or decision cannot be re-proved from accepted main.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.","Stop before a Provider, target, automatic merge, authority consumption, or external effect.","Do not retry a possibly executed external effect whose outcome is unknown."],"plan_lane_state":"plan_lane_active","prerequisite_receipts":["PE7-AC2-BOUNDARY-REPAIR-1 COMPLETE: PR #475 exact head `e89a24bd6776282bbd52ee72cc5be8ecc66acbc2`; merge `b75cd81620ed51aefce5d245855cf00f1bb6385b`; exact-head `PASS` (receipt comment `5302063445`); canonical workflow `31882791484`"],"prerequisites":["PE7-AC2-BOUNDARY-REPAIR-1"],"private_paths_allowed":false,"promotion_evidence_sha256":"f8824b570933ed1eb67c198cd09a967ac794245278580617df22a21eec0704b6","read_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","engine/src/node_executor.rs","engine/src/storage/local_product_store/product_tasks.rs","engine/src/storage/local_product_store/managed_acceptance.rs","engine/tests/test_product_golden_path_g2.rs","engine/tests/test_product_golden_path_evidence.rs","engine/tests/test_local_product_store.rs"],"risk_class":"none","rollback":"Do not change accepted main on failed migration; replace the candidate with a fail-closed repair if a caller cannot preserve unknown/non-retry semantics.","route_manifest_sha256":"aaf4ce0bef27939437762ab2022cf484fd2cc5117f7eebbb32cdf4ff02d63652","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["cargo fmt --all -- --check","cargo clippy -p engine --all-targets --all-features -- -D warnings","cargo test -p engine --test test_product_golden_path_g2","cargo test -p engine --test test_product_golden_path_evidence","cargo test -p engine","bash scripts/check_wire_codegen_drift.sh","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"verification_family":"source_focused_full","worker_tier":"T1"}
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
