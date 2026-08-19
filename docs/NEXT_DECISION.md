# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` is complete: two-arm protocol freeze, redacted credential presence, idle WAL-index read-only open, and a captured real `rwe-live-baseline preflight` that is not `ready=true`. `PE7-RWE-CR-RUN-1` is the current window and remains `DECISION_REQUIRED` because the only existing LocalProductStore is a pre-tenant empty file. Do not create or migrate a Store, issue authority, call a Provider, write a target, or run a replay.

## Authoritative Forward Order

```text
[window: PE7-RWE-CR-RUN-1 — DECISION_REQUIRED, provider-free; live-ready preflight missing]


```

## Active Routing

1. `PE7-RWE-CR-RUN-1` — `DECISION_REQUIRED`

## Completed (PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #572 exact head `0f63ad49c2b5ba87bf5e661bcbae9fd5fab9a9a8`; squash merge `262b67b675c36859c3dee6e1556fa0090654b75c`.

## Completed (PE7-RWE-CR-PROTOCOL-PREFLIGHT-1)

**State:** `COMPLETE`

**Accepted evidence:** Freeze PR #576 exact head `7b9e51bd12d7cb4007915edb9d5809f2db488416`; squash merge `837ae2aadc0470713121361d5c529d6936e8926f`; exact-head review comment `5344672600`; canonical workflow `32273292076`; exact-head check `32273291960`. Idle-SHM follow-on PR #577 exact head `1bfffe1c620cff79caf37bd566f9ee80073d252e`; squash merge `9c25d193d3b85ad9e7cc66af21a0c78ba0171d7a`; exact-head review comment `5345103991`; canonical workflow `32276756829`; exact-head check `32276756856`. Captured CLI against existing `.agent-control-plane/local-team.db` opened read-only then failed closed at principal auth (`no such column: tenant_id`; zero keys/tasks). `ready=true` is not claimed.

## Packet PE7-RWE-CR-RUN-1

**State:** `DECISION_REQUIRED`

**Prerequisite:** `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1`

**Class:** `EFFECT`

**Outcome:** Execute the randomized/interleaved old/new replay exactly once under accepted global stop rules.

**Allowed delta:** Registered effects only after a later `ready=true` provider-free preflight; no arm-specific retry, schedule change, or protocol repair.

**Exit:** Complete blinded arm assignments, attempts, lifecycle costs, drift, review, failures, cleanup, and restricted/redacted evidence.

**Stop:** The only existing LocalProductStore is pre-tenant and empty; captured preflight is not `ready=true`; creating or migrating a Store, inserting keys, or seeding Golden Path would be a data write; allocation integrity would break; or any step would invent readiness or hide an unknown effect.

### Decision-required boundary

The protocol freeze is accepted on `837ae2aa` / `9c25d193`. A real `rwe-live-baseline preflight` against `.agent-control-plane/local-team.db` failed closed: `principal auth failed: no such column: tenant_id`. That file has zero `api_key_metadata` and zero `product_tasks`. This packet has no dispatch capsule and no execution authority until a later accepted ready preflight exists against a current-schema existing Store.

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

`docs/FUTURE_ROUTE.md` is routing-only. `PE7-RWE-CR-RUN-1` was removed from that index because it is the parked current window here; it is not an executable EFFECT until a later accepted ready preflight and dispatch capsule exist.
