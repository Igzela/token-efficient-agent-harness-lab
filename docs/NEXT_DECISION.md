# Next Decision

## Active Track: Phase 8 — Final Completion / GA Seal

Phase 8 is the final planned phase. After Phase 8, no "core completion" phase remains. Future work is maintenance, bugfixes, pilots, or v2 proposals only.

**Current status:** Phase 8 IN PROGRESS. See `docs/PHASE8_FINAL_COMPLETION_PLAN.md` for gap inventory and seal criteria.

**Completed phases:** Phase 1-7 (including 6A, 6B-1/2/3, Gates 1-3), Operator Surface (PRs #49-#52), Dynamic Workflow Batches 1-7, Macro-Orchestrator Phases 1-5, Self-Hosted GA Readiness SG-1 through SG-5, HA Hardening HA-1 through HA-6, Dynamic Regulator MVP Phases 1-5.

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

- **Phase 8 completion:** PostgreSQL active trial, release hardening, safety boundary closure, docs consolidation, final seal
- Autonomous maintenance: repair stale docs, CI breakage, test drift, wire-codegen drift
- Regression hardening: add/repair tests for existing behavior
- Architecture/doc closeout: update records after accepted changes
- After Phase 8 DONE: maintenance, bugfix, pilot, v2 proposals only

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` only when status facts are unclear or the task updates status.
2. Read `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR/merge/CI work, docs cleanup, and real-world pilot tasks.
3. Confirm the proposed task is allowed under the safety gates above.
4. Keep the change commit-sized and run the relevant verification.
5. Run `uv run --no-project python scripts/check_agent_handoff.py`.
6. Update handoff docs before committing and pushing.
