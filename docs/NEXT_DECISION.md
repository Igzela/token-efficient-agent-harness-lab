# Next Decision

Last updated: 2026-08-26.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

The Harness-Evolution C0 loop is closed and the `PE7-HE-MX1-CONTRACT-1` three-axis experiment contract is accepted on `main`, including exact descriptor, admission, comparability, allocation, budget, estimand, and `INCOMPARABLE` boundaries. The provider-free `PE7-HE-MX1-CORE-1` implementation is complete and accepted on `main`: the shared Harness run seam, exact three-axis descriptor manifest, arm-zero and one admitted second Harness adapters, baseline/memory-only/skill-only Strategy adapters, two frozen ModelPlans, deterministic matrix planning, and `INCOMPARABLE` projections. The current window is `PE7-HE-MX1-PILOT-1`, which remains blocked until a separately authorized finite matrix effect is available; no effect is executed from this document.

## Authoritative Forward Order

```text
[completed: PE7-HE-MX1-CORE-1 — COMPLETE, provider-free; shared run seam, exact arm manifest, Strategy adapters, deterministic matrix planning, and INCOMPARABLE projections]
[window: PE7-HE-MX1-PILOT-1 — BLOCKED_PREREQUISITE, separately authorized EFFECT required; preregistered matrix ladder only]
```

## Active Routing

1. `PE7-HE-MX1-PILOT-1` — `BLOCKED_PREREQUISITE` (separate effect authorization required)

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-HE-MX1-CORE-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #621 exact head `199c12756e58ffaa6041a22cd01f23ce7a1eda15`; merge `628577c5e8cb404c4dcc2e689925414bbfda70ab`; exact-head `PASS`; canonical workflow `32848799358`.

## Packet PE7-HE-MX1-PILOT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-MX1-CORE-1 is complete on accepted main `628577c5e8cb404c4dcc2e689925414bbfda70ab` (PR #621 exact head `199c12756e58ffaa6041a22cd01f23ce7a1eda15`; exact-head `PASS`; canonical workflow `32848799358`).

**Class:** `EFFECT`

**Outcome and non-goals:** Execute the preregistered ladder, stopping after `1x2x1` or `1x2x3` when comparability/evidence fails and reaching the minimum `2x2x3` matrix only when every lower-rung gate passes. No arm repair after observation, production routing change, active-Harness adoption, target default-branch write, or unregistered cell.

**Allowed delta:** Separately authorized finite matrix effects in disposable worktrees; no arm repair after observation, production routing change, active-Harness adoption, target default-branch write, or unregistered cell.

**Owners:** Existing Harness run seam, LocalProductStore, budget/spend, runtime, verification, terminal-evidence, recovery, and operator-evidence owners remain authoritative; no new scheduler, store, evaluator, or output owner.

**Forbidden changes:** No target `main` write, merge, release, deployment, active-Harness replacement, second arm, unbounded task, or Provider/effect without a separately authorized finite request and valid credentials. Do not weaken fail-closed unknown-outcome, cleanup, or rollback handling.

**Exit evidence:** Every scheduled and rejected/skipped cell has a terminal reason, exact three-axis identity, verified output, full lifecycle cost including rescue/escalation, drift/contamination evidence, cleanup, and blinded evaluation.

**Stop conditions:** Any lower-rung hard gate fails, an arm becomes incomparable, allocation/authority drifts, hidden rescue occurs, outcome is unknown, evidence is selective, or full-matrix budget/stops are exceeded.

**Next action:** Obtain and verify the separate effect authorization and the frozen preflight binding (exact descriptor manifest digest plus schedule digest). Until a valid finite `GO` receipt and existing-owner task outcome are recorded, do not execute this packet and do not create a dispatch capsule.

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
