# Trial 1 Plan: Multi-Task Queue and Token Budget Efficiency Validation

## Purpose

Trial 1 will validate whether the existing Harness App MVP0-MVP8 can help a
human operator manage multiple candidate tasks from a real local project while
preserving the sealed read-only control-plane boundary.

The trial is intended to exercise:

- repository audit
- non-executable planning
- plan store
- plan review workbench
- review guidance
- portfolio triage
- operations diagnostics
- lower-budget variant comparison

This plan is a planning artifact only. It does not execute Trial 1, add runtime
features, or authorize target repository writes.

## Current Baseline

| Field | Baseline |
| --- | --- |
| Harness repository | `/home/igzela/Projects/token-efficient-agent-harness-lab` |
| Harness commit | `b69b97bd69aac50eb975dda3418a14e4d82caa1f` |
| Target repository candidate | `/home/igzela/Projects/alters-lab` |
| Target commit | `af86b90923eb87291f0b4fcf2a1079383361ba45` |
| Trial 0 final result | `PASS` |
| Trial 0 warnings | `[]` |
| Trial 0 blockers | `[]` |

Trial 1 does not assume permission to write `/home/igzela/Projects/alters-lab`.
Any target repository mutation requires separate explicit human approval later.

## Non-Goals

Trial 1 planning and execution must not introduce:

- Stage 5
- MVP9
- real provider or model API calls
- sandbox, process, container, or VM execution
- autonomous workers
- target repository writes
- approval, run, execute, assign, deploy, or merge controls
- mutation of `/home/igzela/Projects/alters-lab` unless separately approved
- productionization
- persistent event log changes

## Trial Inputs

Trial 1 should use 3-5 candidate tasks from the target repository. Each
candidate is a non-executable planning input only.

| Task ID | Objective | Task Type | Risk Level | Expected Context Budget | Expected Execution Budget | Expected Review Concerns | Expected Evidence Needed | Trial 1 Use |
| --- | --- | --- | --- | ---: | ---: | --- | --- | --- |
| `trial1-docs-governance` | Review governance docs for stale status language after the latest closeout. | `docs_review` | low | 1200 | 800 | Avoid changing task state or phase meaning. | file list, relevant excerpts, final diff if later approved | Exercises low-risk docs cleanup planning. |
| `trial1-audit-health` | Re-run project harness audit and summarize any drift from the Trial 0 PASS baseline. | `audit_review` | low | 1000 | 600 | Distinguish target repo issues from harness auditor issues. | audit output, warnings, blockers, target commit | Exercises read-only audit and evidence routing. |
| `trial1-small-code-review` | Inspect one small implementation area and identify whether a narrow reliability hardening issue exists. | `code_review` | medium | 2200 | 1200 | Avoid broad refactors or unapproved fixes. | files inspected, findings, no-change status | Exercises budgeted code-review planning without mutation. |
| `trial1-provider-boundary` | Evaluate a provider-like request and confirm it is blocked or gated by existing policy. | `boundary_review` | high | 1800 | 900 | Must not call providers or create credentials. | policy excerpts, blocked/gated rationale | Exercises high-risk gating and blocked-plan ranking. |
| `trial1-budget-pressure` | Compare a broad context request with a lower-budget variant to decide if summary context is enough. | `budget_review` | medium | 3500 | 1800 | Avoid unnecessary context expansion. | high-budget plan, lower-budget variant, review guidance | Exercises token pressure and lower-budget comparison. |

## Planned Workflow

This workflow is for future Trial 1 execution. This planning change does not run
these steps.

1. Start the local Harness App server.
2. Register the local `alters-lab` repository if needed.
3. Refresh operations status.
4. Confirm the target audit is still `PASS`, or record and explain any warning.
5. Generate 3-5 non-executable plans from the candidate inputs.
6. For at least one budget-pressure plan, generate a lower-budget variant.
7. Use the plan review workbench to compare plans.
8. Use review guidance to inspect recommended options.
9. Use portfolio triage to rank review priority.
10. Confirm diagnostics remain `OK` and `recent_errors` remain empty, or explain
    any derived errors.
11. Confirm `/home/igzela/Projects/alters-lab` remains unchanged.
12. Record whether the app helps choose the next safe human action.

## Success Metrics

Trial 1 is successful if:

- the target repository remains clean and unmodified
- the audit remains `PASS`, or any warning is explained
- 3-5 non-executable plans are created only in app-owned state
- at least one lower-budget variant is created and compared
- portfolio triage ranks blocked, gated, and budget-pressure plans in a useful
  order
- review guidance suggests lower-budget or split-plan options when appropriate
- the user can identify the top review priority without reading raw JSON
- diagnostics remain `OK`, or provide useful explanation
- no forbidden controls are introduced or used
- the user can choose one of:
  - stop
  - create a human-approved target repository change
  - perform reliability hardening
  - refine docs or demo packaging

## Failure and Block Criteria

Trial 1 should stop if:

- the target repository is modified without explicit approval
- the app implies execution authority
- provider, model, sandbox, or worker behavior appears
- plan store data is written inside the target repository
- diagnostics show a blocked component status that the operator cannot
  understand
- portfolio triage or review guidance creates a confusing or misleading
  recommendation
- lower-budget variant comparison cannot be understood by the user
- audit no longer returns `PASS` and the reason is not explainable

## Evidence to Record Later

The later Trial 1 report should capture:

- commands run
- harness commit
- target repository commit
- registry path
- plans path
- plan IDs
- task objectives
- plan statuses
- token budgets
- lower-budget variant comparison
- review guidance outputs
- triage ranking
- diagnostics status
- `recent_errors`
- optional screenshots
- target repository mutation check
- user observations
- final verdict:
  - `ACCEPTABLE_FOR_MULTI_TASK_TRIAL`
  - `ACCEPTABLE_WITH_NOTES`
  - `BLOCKED_FOR_MULTI_TASK_USE`

## Required Commands for Future Trial Execution

These commands are for a future Trial 1 execution. They are not run by this
planning change.

```bash
python3 tools/check_security_baseline.py
PYTHONPATH=src python3 -m unittest discover -s tests
node --check web/dashboard/app.js
python3 tools/harness_app_server.py --host 127.0.0.1 --port 8769 --registry /tmp/harness-trial1-registry.json --plans /tmp/harness-trial1-plans.json
```

Future browser target:

```text
http://127.0.0.1:8769/
```

Future target mutation check:

```bash
git -C /home/igzela/Projects/alters-lab status -sb
```

## Decision After Trial 1

If Trial 1 passes, do not automatically start MVP9.

The next decision should be one of:

- stop
- docs or demo packaging
- targeted reliability hardening
- Trial 2 on a different real local project
- future production PRD

Any target repository write requires explicit human approval.
