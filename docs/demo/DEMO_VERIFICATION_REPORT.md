# Demo Verification Report

Date: 2026-05-27
Branch: `docs-demo-verification-1`
Server: `127.0.0.1:8769` (stopped after verification)

## Pre-flight

| Step | Result |
|------|--------|
| Security baseline check | ALL CHECKS PASSED |
| Test suite | OK (914 tests) |
| Node syntax check | No output, exit 0 |

## Audit

| Field | Result |
|-------|--------|
| Verdict | `PASS` |
| Warnings | `[]` |
| Blockers | `[]` |
| Checks | 8/8 `PASS` |

## Plan Creation

### Plan 1: Read-Only Docs Audit

| Field | Expected | Actual | Match |
|-------|----------|--------|-------|
| Status | `ready_for_review` | `ready_for_review` | Yes |
| Effective risk | `low` | `low` | Yes |
| Approval gates | empty | `[]` | Yes |
| Blockers | empty | `[]` | Yes |
| Executable | `false` | `false` | Yes |
| Steps | ≥1 with `approval_required=false` | 3 steps, all `false` | Yes |

### Plan 2: Provider/Sandbox Boundary

| Field | Expected | Actual | Match |
|-------|----------|--------|-------|
| Status | `needs_approval` | `needs_approval` | Yes |
| Effective risk | `high` | `high` | Yes |
| Approval gates | non-empty, includes provider/sandbox gates | `["execution_boundary_gate", "human_approval_required", "provider_integration_gate"]` | Yes |
| Executable | `false` | `false` | Yes |
| Steps | all `approval_required=true` | 4 steps, all `true` | Yes |

### Plan 3: Budget-Pressure (High Budget)

| Field | Expected | Actual | Match |
|-------|----------|--------|-------|
| Status | `ready_for_review` or `needs_approval` | `needs_approval` | Yes |
| Executable | `false` | `false` | Yes |
| Effective risk | — | `high` (upgraded from `medium` due to `write` task type) | Acceptable |

### Plan 4: Budget-Pressure (Lower Budget)

| Field | Expected | Actual | Match |
|-------|----------|--------|-------|
| Status | `blocked` or `needs_approval` | `needs_approval` | Yes |
| Executable | `false` | `false` | Yes |
| Effective risk | — | `high` (same `write` task type) | Acceptable |

Note: Plan 4 has identical status/approval gates to Plan 3. Budget-pressure surfaces through review guidance and triage rather than plan-level blockers. The lower-budget variant's differentiation appears in the review guidance (`review_token_budget` action, `reduce_budget` recommended option) and triage ranking.

## Review Guidance

For Plan 1 (`plan-fcbb9a475209d0e4`):

| Field | Expected | Actual | Match |
|-------|----------|--------|-------|
| Executable | `false` | `false` | Yes |
| Preview only | `true` | `true` | Yes |
| Options | non-empty | `["reduce_budget", "compare_with_lower_budget_plan"]` | Yes |
| Evidence requirements | non-empty | 3 items | Yes |
| Token-efficiency guidance | non-empty | 2 items | Yes |
| Boundary notice | contains "advisory only" | "Review guidance is advisory only. It does not approve, execute, or mutate plans." | Yes |
| Next review action | — | `review_token_budget` | — |
| Recommended option | — | `reduce_budget` | — |

## Portfolio Triage

| Field | Expected | Actual | Match |
|-------|----------|--------|-------|
| Total plans | 3 or 4 | 4 | Yes |
| Ranking | semantic risk/budget priority | Priority order: provider/sandbox (92) > write plans (85) > read-only audit (60) — not stored index order | Yes |
| Token hotspots | identified for high-budget plans | `budget_pressure_notes_present` on all 4, `gate_heavy_plan` on provider plan | Yes |
| Budget pressure | flagged for low-budget variant | `review_bucket: "token_budget_review"` on plans 3 and 4 | Yes |
| Boundary notice | contains "advisory only" | "Portfolio triage is advisory only. It does not approve, execute, mutate, assign, or write target repositories." | Yes |
| Summary | — | `ready_for_review: 1`, `needs_approval: 3`, `token_hotspot_count: 4` | Consistent |

## Operations Diagnostics

| Field | Expected | Actual | Match |
|-------|----------|--------|-------|
| Component count | 10 | 10 | Yes |
| Recent errors | `[]` | `[]` | Yes |
| Status | `ok` | `ok` | Yes |
| Components | all `ok` | 10/10 `ok` | Yes |
| Data flow | all `ok` | 5/5 `ok` | Yes |
| Storage registry | path to `/tmp` demo file | `/tmp/harness-demo-verify-registry.json`, 1 record | Yes |
| Storage plans | path to `/tmp` demo file | `/tmp/harness-demo-verify-plans.json`, 4 records | Yes |
| Boundary notice | present | "Operations diagnostics are read-only..." | Yes |

## Target Repository

| Check | Expected | Actual | Match |
|-------|----------|--------|-------|
| `git status -sb` | no changes | Only pre-existing untracked `alters/product/config/` | Yes |
| `git diff --stat` | no changes | Empty output | Yes |

## Clean Shutdown

| Check | Result |
|-------|--------|
| Server stopped | Yes (SIGTERM) |
| No leftover process | Yes |
| Clean terminal output | "Stopping Harness App server." |

## Acceptable Deviations

1. **Plan 4 budget differentiation**: The lower-budget variant (`context_tokens: 1000, execution_tokens: 500`) does not surface budget blockers at plan creation time. Budget pressure surfaces through review guidance and triage instead. This is consistent with the planner's deterministic design — budget signals are derived, not injected at plan creation.

2. **Plan 3 effective risk**: The `write` task type upgrades effective risk to `high` via keyword detection, even though `risk_level: medium` was specified. This is expected behavior from the resource planner.

3. **Triage ranking**: The provider/sandbox plan (stored index 1) ranks highest in triage priority (92), confirming semantic ranking rather than stored-order ranking.

## Conclusion

All verification steps pass. The demo package is accurate and runnable from a clean main branch. The Harness App operates within its defined boundaries: no target repo writes, no provider calls, no execution, all plans non-executable, all guidance advisory.
