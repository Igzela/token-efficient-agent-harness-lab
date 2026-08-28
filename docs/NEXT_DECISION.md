# Next Decision

Last updated: 2026-08-28.

This document owns one current execution window. Accepted receipts belong in
`docs/CURRENT_STATUS.md`; blocked successors belong in `docs/FUTURE_ROUTE.md`;
live PR, CI, review, ruleset, Issue, and mergeability facts require fresh
GitHub readback.

## Current Direction

The owner-approved Autonomous Steward campaign has completed PR0 baseline
recovery, PR1 contract freeze, PR2 Shadow Steward acceptance, and PR3
provider-free executor acceptance. The current routed window is PR4, but it
remains `BLOCKED_PREREQUISITE` pending explicit planning and promotion of the
provider-free canary and single-writer cutover. Automatic merge, Provider
calls, product or target effects, release, deployment, and destructive
operations are not authorized by this blocked route. No `READY_FOR_EXECUTION`
packet is currently exposed.

## Authoritative Forward Order

```text
[completed: PE7-AUTONOMOUS-STEWARD-PR0 — COMPLETE, accepted baseline and control-plane recovery]
[completed: PE7-AUTONOMOUS-STEWARD-PR1 — COMPLETE, Mission/Stage/WorkCard contract and read-only compatibility boundary]
[completed: PE7-AUTONOMOUS-STEWARD-PR2 — COMPLETE, provider-free read-only Shadow Steward]
[completed: PE7-AUTONOMOUS-STEWARD-PR3 — COMPLETE, provider-free autonomous executor]
[window: PE7-AUTONOMOUS-STEWARD-PR4 — BLOCKED_PREREQUISITE, provider-free canary and single-writer cutover]
```

## Active Routing

1. `PE7-AUTONOMOUS-STEWARD-PR4` — `BLOCKED_PREREQUISITE`

## Completed (PE7-AUTONOMOUS-STEWARD-PR3)

**Historical state:** accepted on `main`; PR3 is complete and its provider-free
executor is the prerequisite for the blocked canary and single-writer cutover.

**Historical evidence:** PR #634 exact head
`fed967ebf03bf43ea452f1b450b972b991b0d92d`, exact-head `PASS`, canonical PR
workflow `33167984966`, merged as
`84fdb7b12cd7cd1bebd0214f56592944dbe42ee3`. Post-merge `main` workflow
`33169425071` passed all required jobs. Independent receipt comment
`5452081999` and the service-entrypoint E2E receipt reached
`WAITING_FOR_MERGE`. The provider-free service and rebuildable journal remain
repository-maintenance projections; the legacy controller remains the sole
lifecycle writer and automatic merge remains disabled.

## Packet PE7-AUTONOMOUS-STEWARD-PR4

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-AUTONOMOUS-STEWARD-PR3` — COMPLETE on accepted main
`84fdb7b12cd7cd1bebd0214f56592944dbe42ee3`.

**Class:** `IMPLEMENT`

**Outcome:** Run the provider-free canary and perform the explicit single-writer cutover from the legacy controller to the Steward, enabling guarded merge only after ruleset and exact-head gates are proved.

**Allowed delta:** Fault injection, canary fixtures, emergency-stop/cutover wiring, guarded merge integration, and bounded operator evidence; no Provider, production, deployment, or destructive effect.

**Exit:** Crash, timeout, bad output, path conflict, stale head, CI/review failure, GitHub ambiguity, and restart cases pass; one real provider-free Mission reaches merge with zero routine owner questions and exactly one active writer.

**Stop:** Both controllers can write, emergency stop or rollback is unavailable, review/CI can be bypassed, API ambiguity is replayed blindly, or auto-merge is enabled before all gates are proved.

Promotion requires a refreshed accepted-main/live-GitHub audit, exact allowed
paths, verification and rollback contract, stop conditions, and a new
machine-bound dispatch capsule. This blocked route grants no implementation,
GitHub mutation, Provider, target, release, deployment, or destructive
authority.


## Common Execution Protocol

- Keep the changing PR Draft while iterating; batch repairs before final
  exact-head review and Ready CI.
- A new head invalidates prior review and CI; a new `main` invalidates stale
  baseline conclusions.
- PR3 is provider-free and bounded to repository maintenance. The service
  journal is a rebuildable projection, existing state/review/worktree/
  verification owners remain canonical, and automatic merge stays disabled.
- GitHub API ambiguity or a mutation with unknown outcome requires readback;
  `OUTCOME_UNKNOWN` is never treated as success or retried blindly.

## Hard Stops

- `DECISION_REQUIRED` on conflicting owner direction, unprovable contract or
  identity, missing rollback, secret exposure, unknown external mutation,
  second-writer activation, or any service journal crossing into authority.
- Never weaken exact-head review, canonical CI, expected-head merge,
  credential, effect, target, release, deployment, recovery, or single-writer
  boundaries.
- Never treat the plan, archive refs, branch-local prose, fixture evidence, or
  worker self-report as accepted capability.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` contains only blocked PR5-PR7 routing; the active blocked
PR4 contract remains in this document. Promotion requires the refreshed
accepted PR3 evidence and a new exact dispatch capsule.
