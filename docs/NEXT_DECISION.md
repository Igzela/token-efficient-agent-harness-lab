# Next Decision

Last updated: 2026-08-29.

This document owns one current execution window. Accepted receipts belong in
`docs/CURRENT_STATUS.md`; blocked successors belong in `docs/FUTURE_ROUTE.md`;
live PR, CI, review, ruleset, Issue, and mergeability facts require fresh
GitHub readback.

## Current Direction

The owner-approved Autonomous Steward campaign has completed PR0 baseline
recovery, PR1 contract freeze, PR2 Shadow Steward acceptance, PR3
provider-free executor acceptance, and PR4A Autonomous Integration Readiness.
PR4A is accepted on main at merge `2e812da126b563665a99a950541f17517b9a4c70`
from PR #640; its exact-head review and canonical PR checks passed, and
post-merge canonical workflow `33210031557` passed all required jobs on that
merge SHA. The current routed window is PR4B, but it is blocked until a fresh
finite T3 authority is issued for the named canary and single-writer effect
envelope. The legacy controller remains the sole lifecycle writer and
automatic merge remains disabled.

## Active Routing

1. `PE7-AUTONOMOUS-STEWARD-PR4B` — `BLOCKED_PREREQUISITE`

**Immediate predecessor bridge:** PR4A is accepted on `main` (PR #640 exact
head `29b4e291d36c21eb5676ce6e47ca08662c095beb`, merge
`2e812da126b563665a99a950541f17517b9a4c70`, exact-head review `PASS`,
canonical PR workflow `33208836187`, post-merge `main` workflow
`33210031557`). It proves provider-free Mission activation and Stage
integration readiness only; it does not authorize the PR4B effect.

## Packet PE7-AUTONOMOUS-STEWARD-PR4B

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-AUTONOMOUS-STEWARD-PR4A` — COMPLETE on accepted main
`2e812da126b563665a99a950541f17517b9a4c70`.

**Class:** `EFFECT`

**Worker tier:** `T3`

**Risk class:** `external_effect`; the effect limit must be finite and
nonzero, freshly authorized, and bound to the named targets and operations.

**Outcome:** Run the separately authorized provider-free canary, prove
emergency stop and rollback, cut over to exactly one lifecycle writer, and
enable guarded merge only after exact-head, independent review, canonical CI,
ruleset, and recovery gates pass.

**Required prerequisite:** Generate a fresh live audit/capsule that binds the
Vader runner, systemd service identities, GitHub control-state operations,
emergency stop, single-writer cutover, guarded merge, rollback, finite effect
budget, expected state, target identity, and idempotent readback. No current
accepted receipt supplies that T3 authority.

**Allowed delta after authorization:** Only the explicitly named existing
Vader runner and systemd-managed Steward/legacy service operations, existing
GitHub control-state mutations for enable/disable and emergency-stop, the
single-writer cutover, guarded merge activation, and their bounded evidence
and rollback. No new controller, queue, ledger, store, evaluator, workflow
owner, or document owner may be introduced.

**Stop:** Do not execute PR4B, consume authority, call a Provider, write a
target, change production state, deploy, release, enable automatic merge, or
switch lifecycle writers without that fresh finite authority and exact
readback. Preserve the accepted PR4A evidence and route. Any ambiguous or
outcome-unknown external mutation remains stopped and is not retried blindly.

## Hard Stops

- `DECISION_REQUIRED` on conflicting owner direction, unprovable contract or
  identity, missing rollback, secret exposure, unknown external mutation,
  second-writer activation, or any service journal crossing into authority.
- Ordinary implementation, test, review, CI, main-drift, tool, and recoverable
  conflict failures remain repairable within their accepted packet; they are
  not wait reasons.
- Never weaken exact-head review, canonical CI, expected-head merge,
  credential, effect, target, release, deployment, recovery, or single-writer
  boundaries.
- Never treat a plan, capsule, branch-local prose, fixture, or worker self-report
  as accepted capability or T3 authority.

## Common Execution Protocol

- `READY_FOR_EXECUTION` and `IN_PROGRESS` are executable packet states only
  when their prerequisites, authority, scope, rollback, and verification are
  current and proved from accepted main; PR4B is not in either state.
- Ordinary implementation, test, review, CI, main-drift, tool, and recoverable
  conflict failures remain repairable inside an accepted packet. They do not
  authorize skipping PR4B's T3 gate or starting PR5 early.
- A new main, PR head, review receipt, CI result, or canonical-document change
  invalidates stale evidence. GitHub mutations require exact readback, and
  `OUTCOME_UNKNOWN` is never treated as success or retried blindly.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` retains the blocked successor order PR4B through PR7.
PR4B is the current blocked window; PR5, PR6, and PR7 remain blocked behind it.
No later packet may be started until its predecessor is accepted and its own
authority and verification contract are refreshed from accepted main.
