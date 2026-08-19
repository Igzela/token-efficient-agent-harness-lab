# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-INGRESS-INVENTORY-1` is complete. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker. The current window is `PE7-CWS-PROJECTION-CONTRACT-1`: freeze the derived working-set residency contract. No Provider, store, or RUN-1.

## Authoritative Forward Order

```text
[window: PE7-CWS-PROJECTION-CONTRACT-1 — READY_FOR_EXECUTION, provider-free; working-set residency contract]


```

## Active Routing

1. `PE7-CWS-PROJECTION-CONTRACT-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-INGRESS-INVENTORY-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #579 exact head `b91f207eba8d5910dd97c626c458be0e369c577e`; squash merge `76d21ea2fd4d8a691bc83c28d680e5affff77ba2`; exact-head review comment `5345445854`; canonical workflow `32279656821`; exact-head check `32279656781`.

## Packet PE7-CWS-PROJECTION-CONTRACT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-INGRESS-INVENTORY-1`

**Class:** `CONTRACT`

**Outcome:** Freeze source/hash identity, item kind, residency (`PINNED`/`HOT`/`WARM`/`COLD`), and deterministic promotion/demotion. Authority and blockers cannot be evicted by relevance scoring.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md` only.

**Exit:** Lexicographic residency policy recorded. No second memory/store/evaluator.

**Stop:** Scoring could demote authority/blockers; upstream harvest dictates safety semantics; RUN-1 starts.

### Twelve-field contract

1. **Outcome and non-goals.** Contract only. No projector implementation. No RUN-1.
2. **Prerequisites and evidence.** Ingress inventory COMPLETE on `76d21ea2`.
3. **Owners and paths.** Canonical docs; cites existing context owners.
4. **Frozen invariants.** Ingress owners remain source of truth.
5. **Only semantic delta.** Residency and eviction policy.
6. **Forbidden changes.** No Provider, schema, second owner, or harvest disposition.
7. **Ordered slices.** Record policy; stop before rehydration contract.
8. **Failure taxonomy.** Unknown evidence stays unknown; cannot evict PINNED authority.
9. **Verification.** Handoff, security baseline, diff check.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** Residency policy in `docs/CURRENT_STATUS.md`.
12. **Next action.** Promote `PE7-CWS-REHYDRATION-CONTRACT-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-PROJECTION-CONTRACT-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Freeze the derived working-set residency contract without implementing a projector or starting RUN-1.","allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["A residency and eviction policy that cannot demote authority or blockers by relevance score."],"prerequisites":["PE7-CWS-INGRESS-INVENTORY-1"],"prerequisite_receipts":["PE7-CWS-INGRESS-INVENTORY-1 COMPLETE: PR #579 exact head `b91f207eba8d5910dd97c626c458be0e369c577e`; squash merge `76d21ea2fd4d8a691bc83c28d680e5affff77ba2`; exact-head review comment `5345445854`; canonical workflow `32279656821`; exact-head check `32279656781`"],"forbidden_changes":["Do not implement a projector in this packet.","Do not start PE7-RWE-CR-RUN-1.","Do not allow relevance scoring to evict PINNED authority."],"ordered_steps":["Record residency classes.","Bind eviction order.","Stop before rehydration implementation."],"verification":["git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this documentation PR.","pause_gates":["Stop when scoring could evict authority.","Stop before Provider or RUN-1."],"expected_artifacts":["Residency policy in docs/CURRENT_STATUS.md."],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1."],"worker_tier":"T2","known_store_mutations":[]}
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
