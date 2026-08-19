# Next Decision

Last updated: 2026-08-19.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

The read-only RWE preflight repair is accepted on `262b67b675c36859c3dee6e1556fa0090654b75c`. `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` is the current provider-free contract window: freeze redacted credential readiness and the two-arm comparison protocol on existing owners, then record a real read-only preflight. A missing Store, missing credential symbol, or any blocker is not live-ready. No Provider call, authority consumption, target write, or replay is authorized.

## Authoritative Forward Order

```text
[window: PE7-RWE-CR-PROTOCOL-PREFLIGHT-1 — READY_FOR_EXECUTION, provider-free; protocol/preflight freeze]


```

## Active Routing

1. `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` — `READY_FOR_EXECUTION`

## Completed (PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #572 exact head `0f63ad49c2b5ba87bf5e661bcbae9fd5fab9a9a8`; squash merge `262b67b675c36859c3dee6e1556fa0090654b75c`; exact-head review receipt comment `5328249811`; canonical workflow `32137758400`; exact-head check `32137758389`. Closeout PR #575 squash merge `55170c8b66847608b747eb5d7323f56543ba0bdf`.

## Packet PE7-RWE-CR-PROTOCOL-PREFLIGHT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1` — COMPLETE on accepted main `262b67b675c36859c3dee6e1556fa0090654b75c`.

**Class:** `CONTRACT`

**Outcome:** Freeze redacted credential presence and the two-arm comparison window/allocation/concealment/drift/capacity pairing with two unissued authorization slots; run the existing read-only preflight and record actual ready/blocker fields. Do not run an arm or claim live-ready from a blocked preflight.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/FUTURE_ROUTE.md`, `engine/src/rwe/frozen_rwe_bindings.rs`, `engine/src/rwe/live_baseline_coordinator.rs`, `engine/src/storage/local_product_store/mod.rs`, and `engine/tests/test_pg_integration.rs` only.

**Exit:** Comparison protocol freeze and redacted credential owner are accepted; a captured provider-free preflight shows actual readiness. `ready=true` is required before promoting `PE7-RWE-CR-RUN-1`.

**Stop:** Credential values would be read, frozen hashes would change, a Store would be created, authority would be issued, or live-ready would be claimed from blockers.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze protocol/preflight only. Do not run, issue, consume, call a Provider, or write a target.
2. **Prerequisites and evidence.** Repair COMPLETE on `262b67b675c36859c3dee6e1556fa0090654b75c`.
3. **Owners and paths.** Existing read-only Store/auth/preflight, frozen comparison binding, and operator corpus/protocol/schedule hashes.
4. **Frozen invariants.** Old/new identities and corpus/protocol/schedule hashes stay unchanged.
5. **Only semantic delta.** Redacted credential presence and two-arm comparison protocol projection.
6. **Forbidden changes.** No Provider, effect, schema/migration, hash mutation, credential-value read, or second owner.
7. **Ordered slices.** Redacted presence; two-arm protocol freeze; real read-only preflight capture.
8. **Failure taxonomy.** Fail closed on missing Store, missing credential symbol, identity collision, or hash drift; unknown stays unknown.
9. **Verification.** Exact dispatch-capsule commands.
10. **Compatibility and rollback.** Revert this PR; no store cleanup.
11. **Exit artifact.** Accepted freeze plus captured preflight JSON/stderr.
12. **Next action.** Promote `PE7-RWE-CR-RUN-1` only after `ready=true`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-RWE-CR-PROTOCOL-PREFLIGHT-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Freeze the contemporary two-arm comparison protocol and redacted credential readiness on the accepted read-only preflight owner without issuing authority or running a replay.","allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","engine/src/rwe/frozen_rwe_bindings.rs","engine/src/rwe/live_baseline_coordinator.rs","engine/src/storage/local_product_store/mod.rs","engine/tests/test_pg_integration.rs"],"read_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","engine/src/bin/rwe_live_baseline.rs","engine/src/rwe/frozen_rwe_bindings.rs","engine/src/rwe/live_baseline_coordinator.rs","engine/src/rwe/operator_corpus.rs","engine/src/storage/local_product_store/mod.rs","engine/tests/test_operator_evidence.rs","engine/tests/test_pg_integration.rs"],"allowed_outputs":["Redacted credential-presence projection that never reads credential values.","Existing-owner two-arm comparison protocol freeze bound to accepted hashes.","Provider-free read-only preflight evidence that does not claim live-ready without ready=true."],"prerequisites":["PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1"],"prerequisite_receipts":["PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1 COMPLETE: PR #572 exact head `0f63ad49c2b5ba87bf5e661bcbae9fd5fab9a9a8`; squash merge `262b67b675c36859c3dee6e1556fa0090654b75c`; exact-head review receipt comment `5328249811`; canonical workflow `32137758400`; exact-head check `32137758389`; existing-owner SQLite read-only Store/auth/preflight seam and old/new identity projection accepted; no Provider call, credential-value read, target write, authority consumption, or effect; not a live-ready claim"],"forbidden_changes":["Do not create a second runtime, scheduler, store, controller, evaluator, authority, or evidence owner.","Do not read, output, persist, or validate credential values.","Do not mutate frozen corpus, protocol, or schedule hashes.","Do not issue or consume authority, call a Provider, write a target, or perform an effect.","Do not invent a live-ready claim from a blocked preflight."],"ordered_steps":["Project redacted parent-process credential presence through the existing environment-symbol owner without decoding the secret.","Bind the accepted old/new identities to a frozen two-arm window, allocation, concealment, drift, capacity pairing, and two unissued authorization slots.","Run the existing read-only preflight entry point and record actual ready/blocker fields."],"verification":["cargo fmt --all -- --check","cargo test -p engine current_comparison_manifest_rejects_protocol_freeze_mutation","cargo test -p engine viability_preflight_reports_redacted_credential_presence_without_value","cargo test -p engine viability_preflight_is_read_only_without_store_creation_or_auth_touch","cargo test -p engine read_only_open_accepts_idle_wal_index_when_wal_is_empty","git diff --check","bash scripts/check_wire_codegen_drift.sh","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this protocol/preflight PR. The accepted read-only repair remains on main and requires no store cleanup because this packet issues no authority and creates no operator Store.","pause_gates":["Stop when an owner, caller, path, or readiness fact cannot be re-proved from accepted main.","Stop before any schema/data write, credential-value read, Provider, authority action, target write, or effect.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting."],"expected_artifacts":["Redacted credential presence on the read-only preflight path.","Two-arm comparison protocol freeze with two unissued authorization slots.","Captured provider-free preflight output that does not claim ready unless ready=true."],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1 until this packet has an accepted zero-mismatch ready preflight.","Do not treat a missing-store or missing-credential blocker as live-ready."],"verification_family":"source_focused_full","worker_tier":"T2","risk_class":"none","known_store_mutations":[]}
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

`docs/FUTURE_ROUTE.md` is routing-only. No future sketch authorizes a replay until this packet is accepted with a ready preflight.
