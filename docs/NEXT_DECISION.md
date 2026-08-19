# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-PROJECTION-CONTRACT-1` is complete. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker. The current window is `PE7-CWS-REHYDRATION-CONTRACT-1`: freeze exact rehydration for reduced or cold context. No Provider, store, projector implementation, or RUN-1.

## Authoritative Forward Order

```text
[window: PE7-CWS-REHYDRATION-CONTRACT-1 — READY_FOR_EXECUTION, provider-free; source-bound rehydration]


```

## Active Routing

1. `PE7-CWS-REHYDRATION-CONTRACT-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-INGRESS-INVENTORY-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #579 exact head `b91f207eba8d5910dd97c626c458be0e369c577e`; squash merge `76d21ea2fd4d8a691bc83c28d680e5affff77ba2`; exact-head review comment `5345445854`; canonical workflow `32279656821`; exact-head check `32279656781`.

## Completed (PE7-CWS-PROJECTION-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #580 exact head `0a750a3a5cda92b419efbfb35f89f5cfee0fe429`; squash merge `4129ca5d08cd7a2e89ad2485864ba28900ecc645`; exact-head review comments `5345585496` and `5345585819`; canonical workflow `32280864211`; exact-head check `32280864192`.

## Packet PE7-CWS-REHYDRATION-CONTRACT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-PROJECTION-CONTRACT-1`

**Class:** `CONTRACT`

**Outcome:** Freeze exact rehydration for reduced or `COLD` context through source-bound artifact/range references or deterministic rerun recipes, including integrity, freshness, redaction, private/secret, unavailable, and outcome-unknown behavior.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md` only.

**Exit:** Every rehydratable class has an exact source identity/hash, bounded retrieval recipe, stale/integrity failure rule, and proof that rehydration never implies permission to repeat an external effect.

**Stop:** A reduced item cannot recover verification evidence; recovery depends only on free-form summaries; sensitive raw content would gain a new durable location; rerun semantics are ambiguous.

### Twelve-field contract

1. **Outcome and non-goals.** Contract and named test vectors only. No projector, reducer, new artifact store, or RUN-1.
2. **Prerequisites and evidence.** Projection contract COMPLETE on `4129ca5d`.
3. **Owners and paths.** Canonical docs; existing Git, artifact/evidence, and Store owners remain retrieval owners.
4. **Frozen invariants.** Residency classes stay PINNED/HOT/WARM/COLD. Rehydration is derived reconstruction, not a second truth.
5. **Only semantic delta.** Handle, recipe kinds, integrity/freshness/redaction/unavailable rules, and EFFECT non-authorization.
6. **Forbidden changes.** No Provider, schema, hidden transcript store, permission expansion, or harvest disposition.
7. **Ordered slices.** Record handle and recipe policy; bind fail-closed vectors; stop before projector-core.
8. **Failure taxonomy.** Missing/stale/mismatched hash is UNAVAILABLE, never empty success. Outcome-unknown stays unknown.
9. **Verification.** Handoff, security baseline, diff check.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** Rehydration policy in `docs/CURRENT_STATUS.md`.
12. **Next action.** Promote `PE7-CWS-PROJECTOR-CORE-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-REHYDRATION-CONTRACT-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Freeze exact source-bound rehydration without implementing a projector or repeating an effect.","allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["A rehydration policy where reduced/COLD items recover via exact source identity and never authorize a repeated EFFECT."],"prerequisites":["PE7-CWS-PROJECTION-CONTRACT-1"],"prerequisite_receipts":["PE7-CWS-PROJECTION-CONTRACT-1 COMPLETE: PR #580 exact head `0a750a3a5cda92b419efbfb35f89f5cfee0fe429`; squash merge `4129ca5d08cd7a2e89ad2485864ba28900ecc645`; exact-head review comments `5345585496` and `5345585819`; canonical workflow `32280864211`; exact-head check `32280864192`"],"forbidden_changes":["Do not implement a projector in this packet.","Do not create a hidden transcript or artifact store.","Do not treat rehydration as permission to repeat an EFFECT.","Do not start PE7-RWE-CR-RUN-1."],"ordered_steps":["Record handle and recipe kinds.","Bind integrity, freshness, redaction, and unavailable rules.","Bind named fail-closed test vectors.","Stop before projector-core implementation."],"verification":["git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this documentation PR.","pause_gates":["Stop when a reduced item has only a summary.","Stop when rerun would be an EFFECT.","Stop before Provider or RUN-1."],"expected_artifacts":["Rehydration policy in docs/CURRENT_STATUS.md."],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1."],"worker_tier":"T2","known_store_mutations":[]}
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
