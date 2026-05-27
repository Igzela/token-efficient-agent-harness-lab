# Trial 2 Candidate Selection

Date: 2026-05-27

## 1. Executive Summary

**Recommended candidate:** `hermes-gateway-lab`

This is a real, active project with rich documentation (25+ design docs, runbooks, safety models), production-adjacent Python/shell scripts (40+ files), and a non-trivial code structure. It is fundamentally different from alters-lab: alters-lab is a research/experimental codebase, while hermes-gateway-lab is an operational gateway/worker system with approval queues, dry-run executors, and permission boundaries.

Trial 2 should remain read-only and app-owned-state-only because the goal is generalization validation, not execution. The harness app should produce non-executable plans and advisory guidance for a second target without modifying it.

## 2. Candidate Table

| Path | Branch | Status | Latest Commit | Type | Safety Notes | Usefulness | Recommendation |
|------|--------|--------|---------------|------|-------------|------------|----------------|
| `/home/igzela/Projects/hermes-gateway-lab` | `main` | 1 modified draft | `e96c61d` checkpoint: live10a | Code + docs | Contains `.ts.net` cert/key files (Tailscale infra, not app secrets). 1 dirty draft. | High: 25+ docs, 40+ scripts, approval queue, dry-run executor, permission boundaries. Can test audit, code review, boundary, budget tasks. | **Recommended** |
| `/home/igzela/Projects/agent-learning-exchange` | `protocol/v1.1-A-...` | Clean | `d69cc03` Add exchange protocol hardening v1.1-A | Docs/protocol only | Clean tree. No code, no scripts. Very small. | Low: Only protocol docs and packet schemas. No code to review, no scripts to audit. Would produce shallow plans. | Deferred — too small |
| `/home/igzela/Projects/alters-lab` | `main` | 1 untracked dir | `faed3b0` P8-M1-R1 | Code + docs + tests | Already used in Trial 0/1. Untracked `alters/product/config/`. | N/A — already over-used. | Rejected — already used |

## 3. Recommended Trial 2 Target

| Field | Value |
|-------|-------|
| Target path | `/home/igzela/Projects/hermes-gateway-lab` |
| Current commit | `e96c61d` |
| Current cleanliness | 1 modified draft file (`drafts/hermes-local-execution-worker.service.draft`) — pre-existing, not caused by harness |
| Why different from alters-lab | alters-lab is a research/experimental codebase. hermes-gateway-lab is an operational gateway system with approval queues, dry-run executors, permission deny patterns, and systemd service drafts. It tests whether the harness can handle operational/infrastructure repos, not just research code. |
| Generalization dimension | **Operational codebase** — tests whether audit/planning generalizes beyond research repos to repos with operational concerns (approval flows, permission boundaries, systemd services, dry-run executors). |

## 4. Rejected / Deferred Candidates

### agent-learning-exchange (Deferred)

- **Reason:** Too small. Only protocol docs and packet schemas. No code, no scripts, no tests. Would produce shallow audit results and trivial plans. Not enough structure to test generalization.
- **Safety concern:** None — clean tree, safe to inspect.
- **Deferred because:** Could be revisited if a larger docs-only repo is needed for contrast.

### alters-lab (Rejected)

- **Reason:** Already used in Trial 0 and Trial 1. Reusing it would not test generalization.
- **Safety concern:** None — already proven safe.

## 5. Trial 2 Boundary

| Boundary | Rule |
|----------|------|
| Target repo writes | **Forbidden.** The app never writes to `/home/igzela/Projects/hermes-gateway-lab`. |
| Provider/model calls | **Forbidden.** No OpenAI, Anthropic, Google, or any model provider calls. |
| Sandbox/process/container/VM execution | **Forbidden.** No sandboxes, containers, or VMs. |
| Autonomous workers | **Forbidden.** No real workers spawned. |
| MVP9 | **Forbidden.** No MVP9 implementation. |
| Stage 5 | **Forbidden.** No Stage 5 implementation. |
| CA-8 | **Forbidden.** No CA-8 exists. |
| Plan execution | **Forbidden.** Plans are non-executable resource estimates only. |
| App-owned state | Registry and plans stored under `/tmp/harness-demo-*`. |

## 6. Draft Trial 2 Candidate Tasks

### Task 1: Docs Audit

| Field | Value |
|-------|-------|
| task_id | `trial2-docs-audit` |
| objective | Read-only audit of hermes-gateway-lab documentation structure and completeness |
| task_type | `audit` |
| risk_level | `low` |
| expected context_tokens | 4000 |
| expected execution_tokens | 3000 |
| expected review concern | Whether docs cover all operational components (approval queue, dry-run executor, permission boundaries, worker systemd) |
| expected evidence | List of docs found, coverage gaps, structural notes |

### Task 2: Code Review — Approval Queue

| Field | Value |
|-------|-------|
| task_id | `trial2-approval-queue-review` |
| objective | Review `scripts/approval_queue.py` for boundary compliance and safety patterns |
| task_type | `review` |
| risk_level | `medium` |
| expected context_tokens | 5000 |
| expected execution_tokens | 3000 |
| expected review concern | Whether the approval queue enforces human-in-the-loop, whether it has escape hatches, whether it respects read-only boundaries |
| expected evidence | Code structure analysis, boundary compliance assessment |

### Task 3: Boundary Inspection — Permission Deny Patterns

| Field | Value |
|-------|-------|
| task_id | `trial2-permission-boundary` |
| objective | Inspect permission_deny_dry_run.py and permission_deny_worker.py for boundary enforcement |
| task_type | `boundary` |
| risk_level | `high` |
| expected context_tokens | 4000 |
| expected execution_tokens | 3000 |
| expected review concern | Whether permission deny patterns are consistent, whether they prevent unauthorized execution, whether dry-run and worker implementations agree |
| expected evidence | Boundary compliance matrix, permission deny coverage analysis |

### Task 4: Budget-Pressure — Cross-Module Documentation

| Field | Value |
|-------|-------|
| task_id | `trial2-budget-pressure-docs` |
| objective | Write cross-module documentation connecting docs/, scripts/, and drafts/ |
| task_type | `write` |
| risk_level | `medium` |
| expected context_tokens | 6000 |
| expected execution_tokens | 4000 |
| expected review concern | Token budget pressure from needing to read many files across the repo |
| expected evidence | Coverage of how docs relate to scripts, whether drafts are superseded or active |

### Task 5: Lower-Budget Variant

| Field | Value |
|-------|-------|
| task_id | `trial2-budget-lower` |
| objective | Write cross-module documentation connecting docs/, scripts/, and drafts/ |
| task_type | `write` |
| risk_level | `medium` |
| expected context_tokens | 1500 |
| expected execution_tokens | 500 |
| expected review concern | Budget pressure — whether the planner flags insufficient budget for cross-module work |
| expected evidence | Budget pressure signals in plan status, review guidance, or triage |

## 7. Go / No-Go Decision

**RECOMMEND_TRIAL_2**

Rationale:
- hermes-gateway-lab is a real, active, operationally complex repo
- It has enough structure for 3-5 meaningful non-executable plans
- It tests generalization beyond the research-oriented alters-lab
- All boundary constraints are satisfied (read-only, no provider, no execution)
- The harness app has been verified on alters-lab and the demo package is confirmed accurate

## 8. Next Step

Create/confirm `TRIAL_2_PLAN.md` with the execution steps for Trial 2. Do not execute Trial 2 until the user explicitly approves it.

If the user approves, Trial 2 execution would:
1. Register hermes-gateway-lab in the harness app
2. Run read-only audit
3. Create the 5 candidate plans
4. Verify plan statuses, review guidance, triage, and diagnostics
5. Confirm target repo unchanged
6. Write TRIAL_2_REPORT.md
