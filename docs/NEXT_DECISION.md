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
`33169425071` passed all required jobs. The provider-free service and
rebuildable journal remain repository-maintenance projections; the legacy
controller remains the sole lifecycle writer and automatic merge remains
disabled.

## Packet PE7-AUTONOMOUS-STEWARD-PR4

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-AUTONOMOUS-STEWARD-PR3` — COMPLETE on accepted main
`84fdb7b12cd7cd1bebd0214f56592944dbe42ee3`.

**Route:** See `docs/FUTURE_ROUTE.md` for the routing-only PR4 profile.
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

`docs/FUTURE_ROUTE.md` contains only blocked PR4-PR7 routing. Promotion requires
the refreshed accepted PR3 evidence and a new exact dispatch capsule.
