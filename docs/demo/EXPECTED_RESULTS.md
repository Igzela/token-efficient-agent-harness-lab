# Expected Results

What each demo step should produce.

## Security Baseline Checker

```
RESULT: ALL CHECKS PASSED
```

All five checks (secret scan, import scan, active routing guard, governance boundary guard, stage-0 event guard) pass.

## Test Suite

```
OK
```

All tests pass. No failures, no errors.

## Node Check

No output. Exit code 0 means `web/dashboard/app.js` has no syntax errors.

## Audit (Clean alters-lab)

| Field     | Expected Value                              |
|-----------|---------------------------------------------|
| Verdict   | `PASS` or `PASS_WITH_NOTES`                 |
| Warnings  | empty list, or non-blocking structural notes |
| Blockers  | empty list                                  |
| Checks    | all present, each with status `PASS` or `PASS_WITH_NOTES` |

## Plan 1: Read-Only Docs Audit

| Field          | Expected Value                        |
|----------------|---------------------------------------|
| Status         | `ready_for_review`                    |
| Executable     | `false`                               |
| Effective risk | `low`                                 |
| Approval gates | empty list                            |
| Blockers       | empty list                            |
| Steps          | at least one step with `approval_required=false` |

## Plan 2: Provider/Sandbox Boundary Task

| Field          | Expected Value                                           |
|----------------|----------------------------------------------------------|
| Status         | `needs_approval`                                         |
| Executable     | `false`                                                  |
| Effective risk | `high`                                                   |
| Approval gates | non-empty, includes provider/sandbox boundary gates      |
| Blockers       | may include boundary-related blockers                    |

## Plan 3: Budget-Pressure Variants

**High-budget version:**

| Field          | Expected Value                        |
|----------------|---------------------------------------|
| Status         | `ready_for_review` or `needs_approval`|
| Executable     | `false`                               |

**Lower-budget version:**

| Field          | Expected Value                                          |
|----------------|---------------------------------------------------------|
| Status         | `blocked` or `needs_approval`                           |
| Review action  | should surface `review_token_budget` or `reduce_budget` |
| Blockers       | may include budget-related blockers                     |
| Token notes    | may include budget efficiency warnings                  |

## Review Guidance

For any selected plan:

| Field                | Expected Value                                          |
|----------------------|---------------------------------------------------------|
| Executable           | `false`                                                 |
| Preview only         | `true`                                                  |
| Options              | non-empty list of advisory options                      |
| Evidence requirements| non-empty list                                          |
| Token-efficiency     | non-empty list of guidance items                        |
| Boundary notice      | contains "advisory only"                                |

## Portfolio Triage

| Field                | Expected Value                                          |
|----------------------|---------------------------------------------------------|
| Total plans          | 3 or 4 (depending on how many were created)             |
| Ranking              | by semantic risk/budget priority, not merely stored order|
| Token hotspots       | identified for high-budget plans                        |
| Budget pressure      | flagged for low-budget variant                          |
| Boundary notice      | contains "advisory only"                                |

## Operations Diagnostics

| Field                | Expected Value                                          |
|----------------------|---------------------------------------------------------|
| Component count      | `10`                                                    |
| Recent errors        | empty list `[]`                                         |
| Status               | `ok`                                                    |
| Components           | all report `ok`                                         |
| Data flow            | all steps report `ok`                                   |
| Storage              | registry and plans show record counts                   |

## Target Repository

```bash
git -C /home/igzela/Projects/alters-lab status -sb
git -C /home/igzela/Projects/alters-lab diff --stat
```

Both should show no changes. The target repository is never written to.

## Acceptable Deviations

- **Audit verdict:** If the target repo is not `alters-lab`, the audit may return `PASS_WITH_NOTES` instead of `PASS`. This is expected for repos with different harness control file structures.
- **Empty plan store warning:** The dashboard may show an initial warning or empty state until the first plan is created. This is cosmetic.
- **Browser layout:** Dashboard layout may differ by viewport size. Functionality is unchanged.
- **Plan IDs:** Plan IDs are deterministic hashes. The exact ID strings will differ from examples but the structure is stable.
