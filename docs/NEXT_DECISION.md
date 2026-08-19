# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` is complete without `ready=true`. `PE7-RWE-CR-RUN-1` remains parked: the only existing Store is pre-tenant and empty. The current executable window is `PE7-CWS-INGRESS-INVENTORY-1`, a provider-free read-only inventory that must not change prompts, runtime, Provider, memory, store, or schema, and must not start RUN-1.

## Authoritative Forward Order

```text
[window: PE7-CWS-INGRESS-INVENTORY-1 — READY_FOR_EXECUTION, provider-free; context ingress inventory]


```

## Active Routing

1. `PE7-CWS-INGRESS-INVENTORY-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8` — parked `DECISION_REQUIRED` because captured preflight is not `ready=true` on the pre-tenant empty Store. Not an executable EFFECT.

## Completed (PE7-RWE-CR-PROTOCOL-PREFLIGHT-1)

**State:** `COMPLETE`

**Accepted evidence:** PRs #576 / #577 / #578; merges `837ae2aa` / `9c25d193` / `90d093f4`. Captured CLI not `ready=true`.

## Packet PE7-CWS-INGRESS-INVENTORY-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1`

**Class:** `CONTRACT`

**Outcome:** Enumerate every production and repository-maintenance path that places context in front of a model, plus a factual non-authoritative upstream harvest matrix. No implementation-selection disposition.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md` only.

**Exit:** Zero-unknown ingress matrix and harvest matrix with non-final `candidate_status` only.

**Stop:** A model-visible input lacks an owner; harvest records a final TRANSPANT/ADAPT/REIMPLEMENT/REJECT; unpublished source is treated as a transplant candidate; RUN-1 is started; a Store is created.

### Twelve-field contract

1. **Outcome and non-goals.** Inventory only. No prompt/runtime/Provider/store change. No RUN-1.
2. **Prerequisites and evidence.** Protocol COMPLETE on `90d093f4`. RUN remains blocked on missing ready preflight.
3. **Owners and paths.** Canonical docs only; inventory cites existing engine/scripts owners without editing them.
4. **Frozen invariants.** Authority, store, evaluator, and recovery owners stay put.
5. **Only semantic delta.** Ingress matrix plus harvest facts with `REFRESH_AT_PROMOTION` identities.
6. **Forbidden changes.** No Provider, effect, schema, second owner, or final selection disposition.
7. **Ordered slices.** Enumerate ingress paths; record harvest candidates as non-authoritative; stop before projection contract.
8. **Failure taxonomy.** Unknown stays unknown; missing owner is stop.
9. **Verification.** Handoff, security baseline, diff check.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** Ingress and harvest matrices in `docs/CURRENT_STATUS.md`.
12. **Next action.** Promote `PE7-CWS-PROJECTION-CONTRACT-1`. Do not start RUN-1.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-INGRESS-INVENTORY-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Enumerate model-visible context ingress paths and a non-authoritative upstream harvest matrix without changing runtime owners or starting RUN-1.","allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","scripts/session_context.py","scripts/agent-control/prompt_builder.py","engine/src/provider","engine/src/http_server"],"allowed_outputs":["A zero-unknown ingress matrix bound to existing owners.","A harvest matrix with non-final candidate_status only."],"prerequisites":["PE7-RWE-CR-PROTOCOL-PREFLIGHT-1"],"prerequisite_receipts":["PE7-RWE-CR-PROTOCOL-PREFLIGHT-1 COMPLETE: Freeze PR #576 exact head `7b9e51bd12d7cb4007915edb9d5809f2db488416`; squash merge `837ae2aadc0470713121361d5c529d6936e8926f`; exact-head review comment `5344672600`; canonical workflow `32273292076`; exact-head check `32273291960`; idle-SHM PR #577 exact head `1bfffe1c620cff79caf37bd566f9ee80073d252e`; squash merge `9c25d193d3b85ad9e7cc66af21a0c78ba0171d7a`; exact-head review comment `5345103991`; canonical workflow `32276756829`; exact-head check `32276756856`; captured `rwe-live-baseline preflight` against existing `.agent-control-plane/local-team.db` failed closed at principal auth (`no such column: tenant_id`); not a live-ready claim"],"forbidden_changes":["Do not edit engine runtime owners in this packet.","Do not start PE7-RWE-CR-RUN-1.","Do not record a final TRANSPLANT, ADAPT, REIMPLEMENT, or REJECT disposition.","Do not treat unpublished Command Code source as a transplant candidate.","Do not create a second license or source-registry owner."],"ordered_steps":["Enumerate repository-maintenance and production model-visible ingress paths.","Bind each path to owner, authority class, sensitivity, reduction, and recoverability.","Record harvest candidates with non-final candidate_status and REFRESH_AT_PROMOTION identities."],"verification":["git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this documentation PR. No store or runtime cleanup.","pause_gates":["Stop when an ingress path lacks an owner.","Stop before any Provider, store write, or RUN-1.","Stop when exact-head review or canonical CI is missing."],"expected_artifacts":["Ingress matrix in docs/CURRENT_STATUS.md.","Harvest matrix with non-final candidate_status only."],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1.","Do not treat harvest candidate_status as implementation selection."],"worker_tier":"T2","known_store_mutations":[]}
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

`docs/FUTURE_ROUTE.md` is routing-only. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker, not an executable EFFECT.
