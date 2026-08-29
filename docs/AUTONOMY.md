# Autonomy and Testing Contract

Last updated: 2026-08-29.

This document defines the autonomy governance, lifecycle state machine, review convergence protocol, exact-head CI, and guarded merge contracts for the Autonomous Steward system.

## Lifecycle State Machine

The repository-maintenance outer loop is governed by a single lifecycle state machine:

```text
IDLE → PROPOSING → WAITING_APPROVAL → RUNNING → VERIFYING → INTEGRATING → (REPLAN | COMPLETE)
                                          │
                                          └──► PAUSED_FOR_OWNER (on hard boundary stop)
```

### Stop Taxonomy: Routine Recovery vs Owner Pause

| Category | Trigger | Disposition |
|---|---|---|
| **Routine Recovery** | Worker failure, timeout, formatting error, test failure, CI check failure, review requested changes, git main drift, empty diff | Handled automatically by Steward (retry, split card, repair round, rebase) without interrupting user |
| **Owner Pause (`PAUSED_FOR_OWNER`)** | Material mission goal change, authority/budget expansion request, unapproved production/destructive action, unresolvable safety conflict, `OUTCOME_UNKNOWN` external mutation | Execution halts; reports reason and awaits explicit owner decision |

## Three-Tier Contract Hierarchy

### 1. MaintenanceMission
- Binds user-approved natural language goal to a proposal digest.
- Contains explicit repository, allowed path scopes, and change categories.
- Defines finite attempts, runtime seconds, call counts, and budget ceilings.
- Registered owner approval is cryptographic or authenticated comment digest binding.

### 2. Stage
- Single verifiable integration result and PR boundary.
- Binds base SHA, target branch, acceptance checks, and ordered WorkCard graph.
- Merges to main only after exact-head review and all canonical CI checks pass.

### 3. WorkCard
- Fine-grained unit executable by a weak agent.
- Declares exact allowed paths and forbidden paths.
- Declares step-by-step instructions, focused tests, and expected evidence.
- Dispatched to an isolated git worktree with dedicated path locks.

## Single Writer Guarantee

- Only one active lifecycle writer is permitted at any time.
- Autonomous Steward coordinates dispatch, repair, review, and integration through a rebuildable hash-chained SQLite journal (`steward_journal.py`).
- Child worker sessions run without GitHub write credentials and without provider secrets.

## Review Convergence Protocol

Independent review is a convergence process governed by strict budgets:

```text
stable Draft candidate + local checks passed
        ↓
R1: Independent session, review_mode=full, complete base...head diff
     ├─ no open block_current_head → exact PASS (deferred notes permitted)
     └─ open blockers → 1 autonomous repair batch
                ↓
R2: Independent session, review_mode=repair_verification, complete base...head attestation
     ├─ no open block_current_head → exact PASS + deferred notes
     └─ open blockers remaining → PAUSED_FOR_OWNER / DECISION_REQUIRED (no autonomous R3)
```

### Review Rules:
- `MAX_SUBSTANTIVE_REVIEW_ROUNDS = 2` (R1 + R2)
- `MAX_AUTONOMOUS_REPAIR_BATCHES = 1`
- Exact `PASS` is the sole merge-authorizing verdict.
- Findings separate severity from disposition (`block_current_head` vs `defer`).
- Deferred notes do not block merge eligibility.

## Exact-Head CI and Guarded Merge

1. **Exact-Head Binding**: Every CI check, independent review receipt, and merge decision must attest the exact same commit SHA.
2. **Canonical CI Matrix**:
   - `exact-head-check`
   - `rust-tests`
   - `pg-integration-tests`
   - `typescript-tests`
   - `native-runtime`
   - `docker-build`
   - `python-tests`
   - `rust-typescript-cutover`
   - `context-capsule`
3. **Guarded Merge**: Merge occurs only when branch ruleset, exact-head CI, exact `PASS` review receipt, zero open blockers, and single PR squash-merge conditions are met.

## Recovery and Rollback

- Every Stage and Mission specifies an exact rollback target (e.g. `revert:<SHA>`).
- If a Stage fails or encounters an unrecoverable conflict, the branch is reset or reverted cleanly without affecting `main`.
- Emergency stop command restores the safe stopped state immediately, disabling orchestrator, auto-merge, and dispatch.
