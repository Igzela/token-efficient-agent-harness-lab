# Next Decision

Last updated: 2026-08-28.

This document owns one current execution window. Accepted receipts belong in
`docs/CURRENT_STATUS.md`; blocked successors belong in `docs/FUTURE_ROUTE.md`;
live PR, CI, review, ruleset, Issue, and mergeability facts require fresh
GitHub readback.

## Current Direction

The owner-approved Autonomous Steward campaign has completed PR0 baseline
recovery, PR1 contract freeze, PR2 Shadow Steward acceptance, and PR3
provider-free executor acceptance. The current routed window is PR4 and its
provider-free promotion contract is now `READY_FOR_EXECUTION` on accepted main
`7e6869c77b49ef6ea1909f4308d1e152efd66f23`. Automatic merge, Provider calls,
product or target effects, release, deployment, and destructive operations
remain unauthorized by this repository-maintenance capsule.

## Authoritative Forward Order

```text
[completed: PE7-AUTONOMOUS-STEWARD-PR0 — COMPLETE, accepted baseline and control-plane recovery]
[completed: PE7-AUTONOMOUS-STEWARD-PR1 — COMPLETE, Mission/Stage/WorkCard contract and read-only compatibility boundary]
[completed: PE7-AUTONOMOUS-STEWARD-PR2 — COMPLETE, provider-free read-only Shadow Steward]
[completed: PE7-AUTONOMOUS-STEWARD-PR3 — COMPLETE, provider-free autonomous executor]
[window: PE7-AUTONOMOUS-STEWARD-PR4 — READY_FOR_EXECUTION, provider-free canary and single-writer cutover]
```

## Active Routing

1. `PE7-AUTONOMOUS-STEWARD-PR4` — `READY_FOR_EXECUTION`

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

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-AUTONOMOUS-STEWARD-PR3` — COMPLETE on accepted main
`84fdb7b12cd7cd1bebd0214f56592944dbe42ee3`.

**Class:** `IMPLEMENT`

**Outcome:** Run the provider-free canary and perform the explicit single-writer cutover from the legacy controller to the Steward, enabling guarded merge only after ruleset and exact-head gates are proved.

**Allowed delta:** Within `docs/`, `scripts/agent-control/`, and `tests/` only: fault injection, canary fixtures, emergency-stop/cutover wiring, guarded merge integration, and bounded operator evidence; no Provider, production, deployment, or destructive effect.

**Exit:** Crash, timeout, bad output, path conflict, stale head, CI/review failure, GitHub ambiguity, and restart cases pass; one real provider-free Mission reaches merge with zero routine owner questions and exactly one active writer.

**Stop:** Both controllers can write, emergency stop or rollback is unavailable, review/CI can be bypassed, API ambiguity is replayed blindly, or auto-merge is enabled before all gates are proved.

**Promotion proof:** Accepted main `7e6869c77b49ef6ea1909f4308d1e152efd66f23` supplies the current owner, caller, test, path, operation, destination, and decision evidence. The promotion evidence digest is `34a27ad11dbb7c0b606908ac76ce18f13d7569d73c21f3c9c4e2eedfbecc0fdb`; the current future-route manifest digest is `76a7d06a53cfe7f702529ef69b4601d4f61fca2cae2b9f426f799b2783c6b34f`; the resulting candidate digest is `ea2cfea92e8e0f449c5f1b366543cbee6e203607a9b9344c5bd3ba7a822401dd`.

The contract is bounded to the existing Steward, legacy loop/control, journal,
worker, service, GitHub-observation, test, and canonical-document owners:

1. **Owner evidence:** `scripts/agent-control/steward.py` (`Provider-free`) and `scripts/agent-control/local_loop.py` (`local_loop.py`).
2. **Caller evidence:** `execute_stage` through `tests/test_agent_steward.py`; `poll` through `tests/test_agent_local_loop.py`.
3. **Test evidence:** `execute_stage` and `poll` are each consumed by their corresponding owner tests; fault evidence is retained in `tests/test_agent_steward_faults.py`.
4. **Allowed and read paths:** the exact paths in the capsule below; no symlink, parent traversal, private path, or provider path is admitted.
5. **Ordered implementation slices:** Steward execution/recovery/journal/workers; legacy lifecycle/control/service boundary; fault/restart/single-writer tests; canonical authority and rollback documents.
6. **Verification:** `python -m unittest discover -s tests -p test_agent_*.py`, `python tools/check_security_baseline.py`, `python scripts/check_agent_handoff.py`, and `git diff --check`.
7. **Operations:** preserve the documented rollback boundary; retain the existing bounded service restart path; retain rebuildable journal replay evidence.
8. **Evidence destinations:** fault and unknown-outcome regression evidence in `tests/test_agent_steward_faults.py`; bounded operator evidence in `docs/RUNBOOK.md`.
9. **Decisions:** schema, evaluator, authority, and recovery remain unchanged at their existing owners.
10. **External boundary:** external effect limit is zero; authority consumption, secret values, private paths, Provider calls, target writes, automatic merge, release, deployment, and destructive effects remain forbidden.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{
  "allowed_outputs": [
    "A provider-free change limited to the independently proved current-main allowed paths.",
    "Exact-head verification and review evidence through the existing lifecycle owners."
  ],
  "allowed_paths": [
    "docs/ARCHITECTURE_BOOK.md",
    "docs/CURRENT_STATUS.md",
    "docs/FUTURE_ROUTE.md",
    "docs/MODULE_MAP.md",
    "docs/NEXT_DECISION.md",
    "docs/RUNBOOK.md",
    "scripts/agent-control/control_state.py",
    "scripts/agent-control/dispatcher.py",
    "scripts/agent-control/local_loop.py",
    "scripts/agent-control/steward.py",
    "scripts/agent-control/steward.service",
    "scripts/agent-control/steward_github.py",
    "scripts/agent-control/steward_journal.py",
    "scripts/agent-control/steward_service.py",
    "scripts/agent-control/steward_workers.py",
    "tests/test_agent_control_state.py",
    "tests/test_agent_local_loop.py",
    "tests/test_agent_steward.py",
    "tests/test_agent_steward_faults.py",
    "tests/test_agent_steward_journal.py"
  ],
  "authority_consumption_allowed": false,
  "dispatch_lane": "provider_free_repository_maintenance",
  "expected_artifacts": [
    "fault and unknown-outcome regression evidence (tests/test_agent_steward_faults.py:OUTCOME_UNKNOWN)",
    "bounded operator evidence and rollback procedure (docs/RUNBOOK.md:Steward)"
  ],
  "external_effect_limit": 0,
  "forbidden_changes": [
    "Do not use FUTURE_ROUTE static paths as current-main authority.",
    "Do not create a second controller, ledger, queue, lease, store, or workflow owner.",
    "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."
  ],
  "forbidden_next_actions": [
    "Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.",
    "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.",
    "Do not start a successor whose promotion candidate has not been independently accepted.",
    "Do not use FUTURE_ROUTE static paths as current-main authority.",
    "Do not create a second controller, ledger, queue, lease, store, or workflow owner.",
    "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."
  ],
  "goal": "Run the provider-free canary and perform the explicit single-writer cutover from the legacy controller to the Steward, enabling guarded merge only after ruleset and exact-head gates are proved.",
  "ordered_steps": [
    "scripts/agent-control/steward.py, scripts/agent-control/steward_service.py, scripts/agent-control/steward_journal.py, scripts/agent-control/steward_workers.py: provider-free Steward execution, recovery, journal replay, worker fault handling, and bounded dispatch",
    "scripts/agent-control/local_loop.py, scripts/agent-control/control_state.py, scripts/agent-control/dispatcher.py, scripts/agent-control/steward_github.py, scripts/agent-control/steward.service: legacy lifecycle boundary, emergency stop, guarded merge observation, and service ownership",
    "tests/test_agent_steward.py, tests/test_agent_steward_faults.py, tests/test_agent_steward_journal.py, tests/test_agent_local_loop.py, tests/test_agent_control_state.py: provider-free canary, single-writer, fault, restart, and emergency-stop regression evidence",
    "docs/ARCHITECTURE_BOOK.md, docs/NEXT_DECISION.md, docs/RUNBOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md: canonical authority, acceptance, rollback, operations, and ownership evidence"
  ],
  "packet_id": "PE7-AUTONOMOUS-STEWARD-PR4",
  "packet_state": "READY_FOR_EXECUTION",
  "pause_gates": [
    "Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.",
    "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.",
    "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.",
    "Stop before a Provider, target, automatic merge, authority consumption, or external effect.",
    "Do not retry a possibly executed external effect whose outcome is unknown."
  ],
  "plan_lane_state": "plan_lane_active",
  "prerequisite_receipts": [
    "PR #634 exact head `fed967ebf03bf43ea452f1b450b972b991b0d92d`; merge `84fdb7b12cd7cd1bebd0214f56592944dbe42ee3`; exact-head `PASS`; canonical workflow `33167984966`"
  ],
  "prerequisites": ["PE7-AUTONOMOUS-STEWARD-PR3"],
  "private_paths_allowed": false,
  "promotion_evidence_sha256": "34a27ad11dbb7c0b606908ac76ce18f13d7569d73c21f3c9c4e2eedfbecc0fdb",
  "read_paths": [
    "docs/ARCHITECTURE_BOOK.md",
    "docs/CURRENT_STATUS.md",
    "docs/FUTURE_ROUTE.md",
    "docs/MODULE_MAP.md",
    "docs/NEXT_DECISION.md",
    "docs/RUNBOOK.md",
    "scripts/agent-control/control_state.py",
    "scripts/agent-control/dispatcher.py",
    "scripts/agent-control/local_loop.py",
    "scripts/agent-control/steward.py",
    "scripts/agent-control/steward.service",
    "scripts/agent-control/steward_github.py",
    "scripts/agent-control/steward_journal.py",
    "scripts/agent-control/steward_service.py",
    "scripts/agent-control/steward_workers.py",
    "tests/test_agent_control_state.py",
    "tests/test_agent_local_loop.py",
    "tests/test_agent_steward.py",
    "tests/test_agent_steward_faults.py",
    "tests/test_agent_steward_journal.py"
  ],
  "risk_class": "none",
  "rollback": "preserve the documented rollback boundary (proved by docs/ARCHITECTURE_BOOK.md:rollback)",
  "route_manifest_sha256": "76a7d06a53cfe7f702529ef69b4601d4f61fca2cae2b9f426f799b2783c6b34f",
  "schema_version": "weak_agent_dispatch.v1",
  "secret_values_allowed": false,
  "verification": [
    "python -m unittest discover -s tests -p test_agent_*.py",
    "python tools/check_security_baseline.py",
    "python scripts/check_agent_handoff.py",
    "git diff --check"
  ],
  "verification_family": "source_focused_full",
  "worker_tier": "T1"
}
-->


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
