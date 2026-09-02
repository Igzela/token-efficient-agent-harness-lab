# Autonomy and Testing Contract

Last updated: 2026-09-01.

This document defines the autonomy governance, lifecycle state machine, review convergence protocol, exact-head CI, and guarded merge contracts for the Autonomous Steward system.

## Scope of Steward Autonomy

Autonomous Steward autonomy is planning, bounded execution, verification,
recovery, and repository-maintenance delivery inside a user-approved Mission.
It does not mean autonomous acceptance of a research claim or authority to
change the product's experimental basis. In particular, Steward cannot:

- turn a better score into an accepted Harness or adoption decision;
- mutate the evaluator, task corpus, verification contract, or comparison
  budget to rescue a result;
- authorize unlimited Provider spend, resume a parked effect, or treat a
  contract/preflight as live authorization;
- replace the active Harness, promote Level-2 or Meta, or bypass transfer,
  replication, human approval, or hard quality/safety/comparability gates.

Research Missions may use Steward for Stage decomposition and re-planning, but
their evidence remains owned by the existing RWE/evaluation and
Harness-Evolution boundaries. A successful candidate returns to the common
evidence loop before any explicitly authorized adoption.

## Lifecycle State Machine

The repository-maintenance outer loop is governed by a single lifecycle state machine:

```text
MISSION_RUNNING → STAGE_PLANNED → WORKCARDS_RUNNING → STAGE_INTEGRATED
→ STAGE_PR_DRAFT → STAGE_PR_READY → WAITING_CI_REVIEW
→ (REPAIR / REPLAN | WAITING_FOR_MERGE) → MERGE_DISPATCHED
→ MERGE_READBACK → NEXT_STAGE | COMPLETE
```

`steward_service.py` owns those transitions in its restart-safe journal loop.
`steward.py` is its isolated K=2 WorkCard execution seam, not a second
lifecycle owner.  Every production iteration holds the one service `flock`,
reads Issue #208's `agent-emergency-stop` label before dispatch or merge, and
records intent before a Ready, candidate-supersede, or merge-workflow mutation.

### Stop Taxonomy: Routine Recovery vs Owner Pause

| Category | Trigger | Disposition |
|---|---|---|
| **Routine Recovery** | Worker failure, timeout, formatting error, test failure, CI check failure, review requested changes, git main drift, empty diff | Handled automatically by Steward (retry, split card, repair round, rebase) without interrupting user |
| **Owner Pause (`PAUSED_FOR_OWNER`)** | Material mission goal/completion/forbidden-change change, authority/budget/time/effect expansion, unapproved destructive or hard-to-rollback effect, unresolvable safety conflict | Execution halts; reports reason and awaits explicit owner decision |
| **External uncertainty** | Lost/timeout mutation result or unavailable PR/main authority | `OUTCOME_UNKNOWN` / recovery-required; only read-only reconciliation is permitted and a possibly issued mutation is never repeated |

## Three-Tier Contract Hierarchy

### 1. MaintenanceMission
- Binds user-approved natural language goal to a proposal digest.
- Contains explicit repository, allowed path scopes, and change categories.
- Defines finite attempts, runtime seconds, call counts, and budget ceilings.
- One approval is read from an authenticated GitHub Issue comment: GitHub
  supplies the `OWNER` identity; its immutable comment ID, exact Mission ID,
  proposal SHA-256, and current accepted-main SHA are checked before the
  journal atomically consumes it.  The CLI cannot manufacture an approval.

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
- `steward_journal.py` is the sole durable lifecycle owner; its hash-chained
  records include lease, approval replay, stage, mutation-intent, and
  PR/head-bound accepted-main receipts.
- Production WorkCards use the existing authenticated local OpenCode wrapper
  through `OpenCodeWorkCardWorker`; it consumes the WorkCard objective, scope,
  checks, evidence, attempts, and environment constraints. Its distinct
  read-only OpenCode reviewer is not merge-capable. The wrapper maps T0/T1 to
  `opencode-go/deepseek-v4-flash` and T2 to `opencode-go/deepseek-v4-pro`
  through the operator's existing authenticated login; it does not persist or
  manufacture credentials. The sandbox receives a generated minimal
  `opencode.json` provider declaration plus the authenticated `auth.json`,
  never the operator's complete config. Marker and PR4B adapters are test-only
  compatibility surfaces.

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
     └─ open blockers remaining → Autonomous Stage Replan / Split / Alternative Implementation
                                  (enters PAUSED_FOR_OWNER only if crossing hard boundaries)
```

### Review Rules:
- `MAX_SUBSTANTIVE_REVIEW_ROUNDS = 2` (R1 + R2) per candidate head.
- `MAX_AUTONOMOUS_REPAIR_BATCHES = 1` per review cycle.
- Exact `PASS` is the sole merge-authorizing verdict.
- Findings separate severity from disposition (`block_current_head` vs `defer`).
- Deferred notes do not block merge eligibility.
- If review repair batches are exhausted on a specific head, Steward does not loop review infinitely. Within active Mission authority, Steward autonomously replans the stage, splits cards, or attempts alternative implementations before pausing for owner.

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
3. **Guarded Merge Owner Delegation**: All merges are strictly delegated to the sole canonical merge workflow (`.github/workflows/agent-merge.yml`). Direct `gh pr merge` is prohibited in repository runtime. Merge occurs only when branch ruleset, exact-head CI, exact `PASS` review receipt, zero open blockers, and single PR squash-merge conditions are met. Readback must prove the merged PR number and expected head produced the exact GitHub `main` merge commit; local `HEAD` is never a fallback.

### Merge-dispatch recovery lifecycle

The durable recovery, authority/evidence split, emergency-stop boundary, and
restart/idempotency contract are canonical in
[`docs/ARCHITECTURE.md`](ARCHITECTURE.md#merge-dispatch-recovery-and-emergency-stop-contract).
This autonomy contract owns only lifecycle integration: record the merge
intent before dispatch, persist the returned run identity before settling it,
allow only read-only reconciliation after restart, and require authoritative
merged-PR plus accepted-main readback before advancing a Stage. A legacy
quarantine may advance only after the architecture contract's GitHub
`CLOSED_UNMERGED` fact is journaled; owner authority is never treated as that
fact.

## Recovery and Rollback

- Every Stage and Mission specifies an exact rollback target (e.g. `revert:<SHA>`).
- If a Stage fails or encounters an unrecoverable conflict, the branch is reset or reverted cleanly without affecting `main`.
- The Issue-label emergency stop halts new WorkCard, Ready, supersede, and
  merge dispatch while retaining the active Mission and recovery evidence.
  Existing unresolved merge intents may undergo read-only canonical
  reconciliation while the stop remains active; the legacy quarantine is an
  external effect and therefore requires the separate exact control
  transition described above. No replan or external effect may follow while
  the stop is active. Clearing it does not create a second Mission approval.
