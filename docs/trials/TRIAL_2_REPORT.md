# Trial 2 Report

Date: 2026-05-27

## 1. Metadata

| Field | Value |
|-------|-------|
| Date/time | 2026-05-27T17:38 UTC |
| Harness commit | `9ccba61` (main, after PR #15 merge) |
| Target repo commit | `e96c61d` |
| Target baseline status | 1 modified draft file (`drafts/hermes-local-execution-worker.service.draft`) — pre-existing |
| Registry path | `/tmp/harness-trial2-registry.json` |
| Plans path | `/tmp/harness-trial2-plans.json` |
| App URL | `http://127.0.0.1:8769/` |

## 2. Preflight

| Check | Result |
|-------|--------|
| Security baseline | ALL CHECKS PASSED |
| Tests | 914 OK |
| Node check | PASS |
| Diff check | PASS |

## 3. Audit Result

| Field | Result |
|-------|--------|
| Verdict | `BLOCKED` |
| Warnings | `["Missing optional recommended control files: docs/harness/FINAL_GATE.md, docs/harness/EVIDENCE_INDEX.md"]` |
| Blockers | 6 missing required harness control files: AGENTS.md, PROJECT_BOARD.md, TASK_QUEUE.md, QUALITY_GATES.md, DECISION_RECORD.md, RISK_REGISTER.md |

**Generalization finding:** The audit correctly identifies that hermes-gateway-lab lacks harness control files. This is expected — it is not a harness-managed repo. The app does not crash, does not produce misleading output, and does not attempt to auto-fix the target. The BLOCKED verdict is the correct response for a repo without harness infrastructure.

## 4. Plans Created

| plan_id | task_id | status | risk | gates | blockers | budget | executable |
|---------|---------|--------|------|-------|----------|--------|------------|
| `plan-5dc42782b5406cad` | trial2-docs-audit | blocked | critical | human_approval_required | audit_blocked | 5500 | false |
| `plan-8448b2cb62c20e0f` | trial2-approval-queue-review | blocked | critical | human_approval_required | audit_blocked | 5500 | false |
| `plan-8d72de280ad33a1b` | trial2-permission-boundary | blocked | critical | execution_boundary_gate, human_approval_required | audit_blocked | 5500 | false |
| `plan-848dc3a86b429ac1` | trial2-budget-pressure-docs | blocked | critical | human_approval_required, target_repo_mutation_gate | audit_blocked | 5500 | false |
| `plan-e6ab6aef33ae9655` | trial2-budget-lower | blocked | critical | human_approval_required, target_repo_mutation_gate | audit_blocked | 5500 | false |

**Finding:** All plans are blocked because the audit is BLOCKED. The planner correctly refuses to generate non-executable plans when the target repo lacks required harness control files. This is the expected behavior — the planner does not produce plans for repos it cannot audit.

**Boundary note:** The permission boundary plan correctly includes `execution_boundary_gate`. The write-type plans correctly include `target_repo_mutation_gate`. Gate assignment logic works on blocked plans too.

## 5. Review Guidance

| Plan | next_review_action | recommended_option | preview_only |
|------|-------------------|-------------------|--------------|
| trial2-docs-audit | review_audit_failure | inspect_audit_result | true |
| trial2-permission-boundary | review_audit_failure | inspect_audit_result | true |

**Finding:** Review guidance correctly points to the audit failure. The `inspect_audit_result` option is the appropriate next step. `preview_only: true` and `executable: false` held. Boundary notice present.

## 6. Triage

| Field | Value |
|-------|-------|
| Total plans | 5 |
| Non-executable | true |
| Boundary notice | "Portfolio triage is advisory only..." |
| Ranking | All at priority 90, all `audit_blocked` bucket |

**Finding:** Triage correctly ranks all plans at the same priority since they share the same blocker (audit_blocked). The triage is not stored-index-driven — it groups by semantic bucket. Ranking is less informative when all plans are blocked, but it is correct.

## 7. Diagnostics

| Field | Value |
|-------|-------|
| Component count | 10 |
| Warnings | 0 |
| Blockers | 0 |
| Recent errors | `[]` |
| Storage registry | `/tmp/harness-trial2-registry.json`, 1 record, ok |
| Storage plans | `/tmp/harness-trial2-plans.json`, 5 records, warning (all blocked) |

**Finding:** Diagnostics correctly report 10 components all ok. Storage shows the correct paths. The plan_store has a warning status because all plans are blocked, but this is informational, not an error.

## 8. Boundary Confirmation

| Boundary | Confirmed |
|----------|-----------|
| No target writes | Yes — hermes-gateway-lab unchanged |
| No provider/model calls | Yes — no API calls made |
| No sandbox/process/container/VM execution | Yes — only local harness app server |
| No autonomous workers | Yes — none spawned |
| No Stage 5 | Yes — none |
| No MVP9 | Yes — none |
| No plan execution | Yes — all plans non-executable |
| No CA-8 | Yes — none |

## 9. Final Verdict

**ACCEPTABLE_WITH_NOTES**

Rationale:
- The harness app generalizes correctly to a second, operationally different repo
- Audit correctly BLOCKED when target lacks harness control files (not a failure — a generalization finding)
- All 5 plans correctly blocked with `audit_blocked` blocker
- Review guidance correctly points to audit failure
- Triage correctly groups by semantic bucket
- Diagnostics correctly report 10 components, no errors
- Target repo completely unchanged before/after
- No boundary violations

Notes:
- The BLOCKED audit means Trial 2 cannot proceed to plan execution on hermes-gateway-lab without first adding harness control files to the target repo
- This is the expected behavior for a non-harness-managed repo
- The app's diagnostic value is limited when the audit is BLOCKED — it cannot generate plans, guidance, or triage that go beyond "audit failed"
- To make hermes-gateway-lab a viable Trial 2 target for full plan execution, it would need AGENTS.md and the 5 required docs/harness/ control files

## 10. Recommended Next Decision

**target onboarding plan**

Rationale: hermes-gateway-lab is a valid second target but needs harness control files before Trial 2 can proceed to plan execution. A target onboarding plan would document what control files are needed and how to add them without modifying the target repo in ways that violate its purpose.

Alternative: **persist Trial 2 report** — if the BLOCKED result is sufficient evidence that the app generalizes, no further action is needed on hermes-gateway-lab.
