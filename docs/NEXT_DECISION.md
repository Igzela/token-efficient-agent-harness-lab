# Next Decision

## Active Track: Real-World Testing Mode

The project is in **Real-World Testing Mode**. The Dynamic Global Regulator is validated through real tasks, real branches, real commits, real PRs, real CI, and gated auto-merge.

**Next implementation priority:** Review Phase 5 dry-run auto-adjustment evidence. Phase 1 (ContextBridge, ContextBudgetAllocator, tick integration) is DONE; Phase 2 (RunTraceRecorder, OutcomeAttributor, PatternDetector, feedback patterns endpoint) is DONE; Phase 3 (ShadowRouter, PolicySimulator, delta metrics, policy-delta endpoint, dashboard delta display, SDK methods) is DONE; Phase 4 (PolicyProposer, ProposalValidator, ProposalSerializer, generated proposals endpoint) is DONE; Phase 5 is PARTIAL with dry-run infrastructure only (`AutoAdjustmentPolicy`, `AutoAdjustmentGuard`, `PolicySnapshotPreview`, `GET /api/v1/auto-adjustments`). Active automatic adjustment, POST apply, and rollback endpoints require separate explicit human approval. Audit details live in `docs/PHASE5_AUTO_ADJUSTMENT_AUDIT.md`; phase matrix details live in `docs/DYNAMIC_REGULATOR_PHASE_0_5_COMPLETION_MATRIX.md`. See `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for the operational execution guide.

**Agent Autonomous Maintenance Mode is active.** Agents autonomously maintain repo health, docs hygiene, CI correctness, and low-risk PR flow. CI green is the merge/success standard. Documentation maintenance means update/prune/archive, not accumulate. See playbook section "Agent Autonomous Maintenance Mode" for the full loop and rules.

**Strategic background:** `docs/DYNAMIC_GLOBAL_REGULATOR_PLAN.md` (read when strategic context needed, not at every session start).

## Architecture refactor (R-series)

**Architecture Refactor R-series**: **SEALED AT R7.** R8 is not approved. No further R-series file splitting is approved.

## Disallowed by Default

The following require explicit human approval and a new implementation plan:

## Safety Gates

| Gate | Rule |
|---|---|
| No secrets committed | `scripts/acp_secret_scan.py` enforced |
| No merge on failing CI | all jobs must pass |
| No unlogged execution | ledger records all dispatches |
| Rollback path required | `git revert` sufficient for low-risk |
| Provider execution | env-gated (`ACP_ENABLE_PROVIDER_EXECUTION=1`) |
| CLI execution | env-gated (`ACP_ENABLE_CLI_EXECUTION=1`) |
| No auto release/tag/deploy | explicit approval required |
| High-risk changes | auth, security, provider, deploy, DB — explicit approval |
| YAML/rubric/policy mutation | explicit approval |
| Destructive operations | explicit approval |

## Auto-Merge Policy

Auto-merge eligible: docs-only, tests-only, CI fix, small low-risk code fix (< 50 lines), all CI green, handoff guard pass, `git revert` rollback.

Not auto-merge eligible: auth/security/provider/deploy/DB changes, release/tag/deploy, policy mutation, failing CI, unclear rollback. PR #31 is not auto-merge eligible (DB schema v12 migration + active policy override routing path).

Full classifier: `docs/REAL_WORLD_TESTING_PLAYBOOK.md`

## Allowed Next Paths

Autonomously maintain repo health and fix CI/docs/test drift. The following paths are allowed:

- Autonomous maintenance: repair stale docs, CI breakage, test drift, wire-codegen drift
- Regression hardening: add/repair tests for existing behavior
- Real-world pilot matrix: execute first 10 tasks from playbook
- Dynamic regulator hardening: safety gate HTTP tests complete (PR #32); Phase 1 DONE (PR #33); Phase 2 DONE (PR #34); Phase 3 DONE; Phase 4 DONE (PR #35 — PolicyProposer, ProposalValidator, ProposalSerializer, generated proposals endpoint); Phase 5 dry-run infrastructure PARTIAL; remaining: review dry-run evidence, then decide whether to approve active apply + rollback
- Proposal CRUD lifecycle validation: safety gate tests pass; remaining: verify end-to-end proposal→dispatch integration with real tier override
- Architecture/doc closeout: update records after accepted changes

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` only when status facts are unclear or the task updates status.
2. Read `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR/merge/CI work, docs cleanup, and real-world pilot tasks.
3. Confirm the proposed task is allowed under the safety gates above.
4. Keep the change commit-sized and run the relevant verification.
5. Run `uv run --no-project python scripts/check_agent_handoff.py`.
6. Update handoff docs before committing and pushing.
