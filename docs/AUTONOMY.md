# Autonomy and Testing Contract

Last updated: 2026-09-06.

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

### Research Mainline: Testing, Review, and Merge Rules

The bounded closed-loop research mainline (`MISSION-RESEARCH-20260901` and its
accepted Stages such as `steward-stage-1-3dc4142fec0b3eb0`) is governed by the
standard Steward lifecycle, testing, review, and merge rules in this document.
Research Stages and WorkCards:

- are confined to their owner-approved allowed paths and change categories and
  obtain evidence only through finite frozen canonical experiments on the
  common RWE basis; they grant no live Provider effect, spend, merge, release,
  deployment, evaluator, or adoption authority by themselves;
- are tested, reviewed, and merged only under the exact-head CI matrix and
  exact `PASS` independent-review gates defined above, on one coherent
  base...head diff; and
- may not mutate the evaluator, task corpus, verification contract, comparison
  budget, or experiment identity to rescue a result.

Level-1 (transfer/replication/memory+skill) and Level-2/Meta (R4/R5/R6)
disposition require complete lower-rung evidence, hard quality/safety/
comparability gates, and explicitly authorized adoption before any change to
the active Harness. The exact advancement gates are owned by
`docs/ROADMAP.md`; module and authority ownership by `docs/ARCHITECTURE.md`.

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

### Owner-direct existing-PR repair lane

Steward-dispatched work continues to require an accepted Mission, Stage, and
WorkCard. A separate cold-start lane exists only for repairing one already
existing Draft PR when an authenticated GitHub `OWNER` comment carries the
bounded `steward-owner-direct-repair:v1` marker. The marker binds exactly the
repository, PR number, current head SHA and branch, an authorization ID,
allowed repository paths, and provider-free verification commands. GitHub's
live PR state must prove that the PR is open, based on `main`, and still Draft;
the comment's OWNER association and issue URL must also match. Historical
markers whose repository, PR, head, or branch no longer matches the live PR
are retained but ignored. Zero currently applicable valid markers, or multiple
currently applicable valid markers, fail closed.

The lane is represented by the separate
`agent_owner_direct_repair_entry.v1` session projection. It does not create or
read a Mission, Stage, WorkCard, Steward journal, or Steward service, and its
`steward_continuity` field explicitly remains unavailable while its
`execution_authority` field can be confirmed by the live GitHub binding. The
checkout must already be the bound PR branch at the bound exact head; the lane
permits only in-scope coding, declared provider-free verification, and a push
to that same branch. It never authorizes `main` writes, provider spend,
deployment, destructive effects, adoption, review/CI bypass, or merge.

After a repair, the PR remains Draft until the normal exact-head independent
review, canonical CI matrix, and guarded merge workflow authorize progression.
The owner-direct binding is not a review verdict, CI result, merge authority,
or replacement lifecycle owner.

A Mission reaches `COMPLETE` only when both conditions are satisfied:
1. its repository-maintenance Stage lifecycle is settled; and
2. every Mission-level acceptance obligation has a machine-verifiable terminal disposition.
If preplanned Stages are exhausted while obligations remain unresolved, Steward generates
dynamic follow-up Stages or enters `RESEARCH_PENDING`, preventing false-positive completion.
Ordinary maintenance missions without an acceptance ledger retain their standard bounded behavior.

### Stop Taxonomy: Routine Recovery vs Owner Pause

| Category | Trigger | Disposition |
|---|---|---|
| **Routine Recovery** | Worker failure, timeout, formatting error, test failure, CI check failure, review requested changes, git main drift, empty diff, missing workflow run ID, orphan merge dispatch | Handled automatically by Steward (retry, split card, repair round, rebase, exact-candidate quarantine, replacement replan) under standing Mission approval without interrupting user |
| **Owner Pause (`PAUSED_FOR_OWNER`)** | Material mission goal/completion/forbidden-change change, authority/budget/time/effect expansion, unapproved destructive or hard-to-rollback effect, unresolvable safety conflict, genuine emergency stop | Execution halts; reports reason and awaits explicit owner decision |
| **External uncertainty** | Lost/timeout mutation result or unavailable PR/main authority without standing recovery authority | `OUTCOME_UNKNOWN` / recovery-required; only read-only reconciliation is permitted and a possibly issued mutation is never repeated |

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
- Production WorkCards use the authenticated local Codex CLI through
  `CodexWorkCardWorker`; it consumes the WorkCard objective, scope, checks,
  evidence, attempts, and environment constraints. Its distinct read-only
  Codex reviewer is not merge-capable. The wrapper uses the account-selected
  Codex default model within the bounded T0-T2 policy; an explicit
  `AGENT_CODEX_MODEL` override is optional and operator-controlled. The full
  managed executable, credential, filesystem, and fail-closed boundary is
  canonical in
  [`docs/ARCHITECTURE.md`](ARCHITECTURE.md#managed-codex-credential-boundary).
  Marker and PR4B adapters are test-only compatibility surfaces.
- **Codex Lifecycle Hooks Autonomous Operating Contract**:
  - **H0 Capability Gating**: Production workers gate hook invocation on `CodexHookProbe`. All 14 capabilities (`hooks.basic`, `session_start`, `pre_tool`, `post_tool`, `permission_request`, `compact`, `stop`, `interrupt`, `subagent`, `async`, `mcp_tool`, `isolated_codex_home`, `hook_trust_bootstrap`, `definition_hash_invalidation`) must be actively evaluated; native per-handler discovery trust readback (`trusted`) and definition hash invalidation (`modified`) are verified, and missing core capabilities fail closed with deterministic diagnostics. Trust provisioning failure refuses the run (`codex_hooks_provisioning_failed`); post-run execution attestation rejects unguarded outcomes (`codex_hooks_execution_unattested`), because the runtime silently skips untrusted hooks.
  - **H1 Context & Compaction Invariants**: Bounded context injection at `SessionStart` isolates WorkCard requirements. The `PreCompact` checkpoint plus `SessionStart(source="compact")` rehydration contract guarantees that long-running context compactions cannot cause the agent to lose its assigned scope or produce goal drift. Receipts persist only redacted input; PASS evidence must be bound to WorkCard, scope, code state, command, and a real success signal.
  - **H2 Worktree Boundary Enforcement**: `PreToolUse` strictly enforces `allowed_paths`, worktree isolation, and fail-closed validation on missing context, preventing edits to forbidden files or destructive operations. `PermissionRequest` enforces fail-closed evaluation, approving only provably scoped low-risk actions.
  - **H3 Completion Continuation Loop**: The `Stop` hook evaluates declared WorkCard acceptance and verification evidence (`allowed_paths`, `focused_tests` / `verification_evidence.json`), rather than raw git status modifications. When incomplete, a bounded continuation retry budget (default: 2 attempts) intercepts the stop signal via top-level `decision="block"` and prompts continuation without operator intervention. Budget exhaustion records an explicit incomplete status.

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
allow only read-only remote reconciliation after restart (with bounded local
checkpoint worktree materialization), and require authoritative merged-PR plus
accepted-main readback before advancing a Stage. A legacy
quarantine may advance only after the architecture contract's GitHub
`CLOSED_UNMERGED` fact is journaled; owner authority is never treated as that
fact.

For a WorkCard interrupted after its implementation checkpoint, restart
recovery selects only the current journal tail and one ordered attempt whose
`WORKER_STARTED`, `WORKER_CHECKPOINT`, and subsequent verification facts bind
the Mission, Stage, card, base, derived branch, exact head, scoped diff, and
implementation session. The existing local branch must already point at the
checkpoint head and the base must be its ancestor; a missing derived worktree
may then be re-materialized without moving the branch. Recovery resumes at
deterministic verification or independent review, and never invokes the
implementation worker again. Missing, stale, mixed-attempt, dirty, or
identity-mismatched checkpoint evidence remains `RECOVERY_REQUIRED`.
If resumed deterministic verification itself fails, the checkpoint remains
`VERIFYING` and the invocation returns `RECOVERY_REQUIRED`; a later attempt may
retry only that verifier, never a new implementation attempt.

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
