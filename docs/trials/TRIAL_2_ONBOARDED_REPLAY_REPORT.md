# Trial 2 Onboarded Replay Report

Date: 2026-05-27

## 1. Metadata

| Field | Value |
|-------|-------|
| Date/time | 2026-05-27 (post-onboarding) |
| Harness commit | current main |
| Target repo | `/home/igzela/Projects/hermes-gateway-lab` |
| Target repo commit | `e96c61d` (before onboarding merge) |
| Target onboarding commit | `05092f0` (branch `harness-onboarding`, now merged) |
| Target merge commit | `77cf282` (PR #1 merged to target main) |
| Target baseline status | 1 modified draft file (`drafts/hermes-local-execution-worker.service.draft`) — pre-existing |
| Registry path | `/tmp/harness-trial2-registry.json` |
| Plans path | `/tmp/harness-trial2-plans.json` |
| App URL | `http://127.0.0.1:8769/` |

## 2. Context

Trial 2 originally ran against hermes-gateway-lab and returned `ACCEPTABLE_WITH_NOTES` with audit `BLOCKED` — the target repo lacked required harness control files. Onboarding was then applied locally on branch `harness-onboarding` (commit `05092f0`). This report documents the replay after onboarding.

## 3. Audit Result

| Field | Result |
|-------|--------|
| Verdict | `PASS_WITH_NOTES` |
| Blockers | 0 |
| Warnings | 17 (non-blocking) |

**Key change from pre-onboarding:** `audit_blocked` disappeared. The audit now passes with notes. Warnings are informational (missing optional files like `FINAL_GATE.md`, `EVIDENCE_INDEX.md`).

## 4. Plans Created

| plan_id | task_id | status | risk | gates | blockers | budget | executable |
|---------|---------|--------|------|-------|----------|--------|------------|
| — | trial2-docs-audit | ready_for_review | medium | — | — | 5500 | false |
| — | trial2-approval-queue-review | ready_for_review | medium | — | — | 5500 | false |
| — | trial2-permission-boundary | needs_approval | high | execution_boundary_gate | — | 5500 | false |
| — | trial2-budget-pressure-docs | needs_approval | high | target_repo_mutation_gate | — | 5500 | false |
| — | trial2-budget-lower | needs_approval | high | target_repo_mutation_gate | — | 5500 | false |

**Key changes from pre-onboarding:**
- Plans are no longer `blocked` with `audit_blocked` blocker
- Status split: 2 `ready_for_review`, 3 `needs_approval` (correct gate assignment)
- Risk levels are now differentiated (medium vs high) instead of all critical
- Gate assignment correct: permission-boundary gets `execution_boundary_gate`, write-type plans get `target_repo_mutation_gate`

## 5. Review Guidance

| Plan | next_review_action | recommended_option | preview_only |
|------|-------------------|-------------------|--------------|
| trial2-permission-boundary | — | inspect_gates | true |
| trial2-budget-lower | — | reduce_budget | true |

**Finding:** Review guidance correctly differentiates plans. `preview_only: true` and `executable: false` held. Boundary notice present.

## 6. Triage

| Field | Value |
|-------|-------|
| Total plans | 5 |
| Non-executable | true |
| Boundary notice | "Portfolio triage is advisory only..." |
| Semantic ranking | boundary (91) > write (85) > review/audit (60) |

**Finding:** Triage now produces meaningful semantic ranking instead of all plans at equal priority. Permission-boundary ranks highest. Budget-write plans rank second. Docs/audit plans rank lowest. This is the expected behavior for a non-blocked audit.

## 7. Diagnostics

| Field | Value |
|-------|-------|
| Component count | 10 |
| Warnings | 0 |
| Blockers | 0 |
| Recent errors | `[]` |
| Storage registry | 1 record, ok |
| Storage plans | 5 records, ok |

**Finding:** Diagnostics correctly report 10 components all ok. No errors. Storage healthy.

## 8. Boundary Confirmation

| Boundary | Confirmed |
|----------|-----------|
| No target writes | Yes — hermes-gateway-lab unchanged (same pre-existing dirty state) |
| No provider/model calls | Yes — no API calls made |
| No sandbox/process/container/VM execution | Yes — only local harness app server |
| No autonomous workers | Yes — none spawned |
| No Stage 5 | Yes — none |
| No MVP9 | Yes — none |
| No plan execution | Yes — all plans non-executable |
| No CA-8 | Yes — none |

## 9. Final Verdict

**ACCEPTABLE_FOR_ONBOARDED_SECOND_PROJECT_TRIAL**

Rationale:
- Audit moved from `BLOCKED` to `PASS_WITH_NOTES` after onboarding — the onboarding worked
- All 5 plans created successfully with correct statuses, gates, and risk levels
- Review guidance correctly differentiates plans
- Triage produces meaningful semantic ranking
- Diagnostics report 10 components, no errors
- Target repo completely unchanged before/after replay
- No boundary violations

## 10. Cross-reference

- Original Trial 2 report: `docs/trials/TRIAL_2_REPORT.md`
- Onboarding followup: `docs/trials/TRIAL_2_ONBOARDING_FOLLOWUP.md`
- Onboarding plan: `docs/onboarding/TARGET_REPO_ONBOARDING_PLAN.md`
- Onboarding template: `docs/onboarding/TARGET_REPO_ONBOARDING_TEMPLATE.md`

## 11. Recommended Next Decision

**hermes-gateway-lab target onboarding is now COMPLETE.**

The onboarding branch was pushed, PR #1 was reviewed and merged (commit `77cf282`). Audit after merge remains `PASS_WITH_NOTES` with 0 blockers. The pre-existing dirty draft is preserved. hermes-gateway-lab is now harness-managed enough for audit and planning.

## 12. Final Closeout

| Event | Status |
|-------|--------|
| Target repo initial audit | BLOCKED (missing harness control files) |
| Onboarding protocol designed | Complete (`docs/onboarding/TARGET_REPO_ONBOARDING_PLAN.md`) |
| Onboarding applied to target | Complete (commit `05092f0`) |
| Onboarded replay | ACCEPTABLE_FOR_ONBOARDED_SECOND_PROJECT_TRIAL |
| Target PR reviewed | APPROVE TARGET PR FOR HUMAN MERGE |
| Target PR merged | 77cf282 |
| Audit after merge | PASS_WITH_NOTES, blockers [] |
| Dirty draft preserved | Yes |
| Target onboarding | **COMPLETE** |
