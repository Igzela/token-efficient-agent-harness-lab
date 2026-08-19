# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-REHYDRATION-CONTRACT-1` is complete. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker. The current window is `PE7-CWS-PROJECTOR-CORE-1`: implement the pure deterministic working-set projector. Disposition is `REIMPLEMENT`. No Provider, store table, or RUN-1.

## Authoritative Forward Order

```text
[window: PE7-CWS-PROJECTOR-CORE-1 — READY_FOR_EXECUTION, provider-free; pure projector]


```

## Active Routing

1. `PE7-CWS-PROJECTOR-CORE-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-PROJECTION-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #580 exact head `0a750a3a5cda92b419efbfb35f89f5cfee0fe429`; squash merge `4129ca5d08cd7a2e89ad2485864ba28900ecc645`.

## Completed (PE7-CWS-REHYDRATION-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #581 exact head `b7b4037bd31731e1ba0f16904006d38bf4c78b82`; squash merge `1b6d73fce72cb195578ae5af784203f7de274e9f`; exact-head review comments `5345676315` and `5345676536`; canonical workflow `32281612446`; exact-head check `32281612505`.

## Packet PE7-CWS-PROJECTOR-CORE-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-REHYDRATION-CONTRACT-1`

**Class:** `IMPLEMENT`

**Outcome:** Pure deterministic projector and residency transitions behind existing context owners. Implementation-selection disposition: `REIMPLEMENT`.

**Allowed delta:** `engine/src/context_working_set.rs`, `engine/src/lib.rs`, `engine/src/workflow/context_pack/assembly.rs`, `engine/src/workflow/context_pack/mod.rs`, `tools/test_check_agent_handoff.py`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`.

**Exit:** Ordering, pinning, supersession, duplicate, token/byte bound, deterministic replay, delete/rebuild, stale-source, and forbidden-eviction tests pass.

**Stop:** Ambient credentials, persisted truth, scoring eviction of PINNED, second owner, or treating INGRESS `candidate_status` as disposition.

### Twelve-field contract

1. **Outcome and non-goals.** Projector core only. No reducer, cache partition, Provider, or RUN-1.
2. **Prerequisites and evidence.** Rehydration COMPLETE on `1b6d73fc`.
3. **Owners and paths.** New derived module plus existing `context_pack` owner as the only consumer.
4. **Frozen invariants.** PINNED never evicted by bound pressure on HOT/WARM. Harvest status is not disposition.
5. **Only semantic delta.** Pure projection + thin owner adapter.
6. **Forbidden changes.** No schema, store table, Provider, or TRANSPLANT of UNKNOWN harvest.
7. **Ordered slices.** Record `REIMPLEMENT`; implement projector; wire owner adapter; tests.
8. **Failure taxonomy.** Stale PINNED and integrity mismatch fail closed.
9. **Verification.** Focused `cargo test -p engine --lib context_working_set`, handoff, security baseline, diff check.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** `engine/src/context_working_set.rs` plus receipt in `docs/CURRENT_STATUS.md`.
12. **Next action.** Promote `PE7-CWS-TOOL-RESULT-REDUCTION-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-PROJECTOR-CORE-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Implement the pure deterministic working-set projector with REIMPLEMENT disposition.","allowed_paths":["engine/src/context_working_set.rs","engine/src/lib.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","tools/test_check_agent_handoff.py","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["engine/src/context_working_set.rs","engine/src/lib.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","tools/test_check_agent_handoff.py","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md","engine/src/workflow/context_pack/"],"allowed_outputs":["A rebuildable projector that cannot evict PINNED authority and is consumed by the existing context_pack owner."],"prerequisites":["PE7-CWS-REHYDRATION-CONTRACT-1"],"prerequisite_receipts":["PE7-CWS-REHYDRATION-CONTRACT-1 COMPLETE: PR #581 exact head `b7b4037bd31731e1ba0f16904006d38bf4c78b82`; squash merge `1b6d73fce72cb195578ae5af784203f7de274e9f`; exact-head review comments `5345676315` and `5345676536`; canonical workflow `32281612446`; exact-head check `32281612505`"],"forbidden_changes":["Do not treat INGRESS candidate_status as TRANSPLANT.","Do not add a store table or Provider call.","Do not start PE7-RWE-CR-RUN-1."],"ordered_steps":["Record REIMPLEMENT.","Implement projector.","Wire context_pack adapter.","Pass focused tests."],"verification":["cargo test -p engine --lib context_working_set -- --test-threads=1","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this documentation and engine PR; the projector module is derived-only and leaves Store, Provider, and context_pack ownership unchanged.","pause_gates":["Stop if PINNED could be evicted by score.","Stop before Provider or RUN-1."],"expected_artifacts":["engine/src/context_working_set.rs"],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1."],"worker_tier":"T1","known_store_mutations":[]}
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
