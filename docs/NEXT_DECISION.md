# Next Decision

## Core Plan COMPLETE

**Phase 8 DONE. Core plan COMPLETE.** No future core-completion phase remains. Future work is maintenance, bugfixes, pilots, or v2 proposals only. Historical gap inventory and seal evidence are archived at `docs/archive/phase-closeouts/PHASE8_FINAL_COMPLETION_PLAN.md`.

**Completed phases:** Phase 1-7 (including 6A, 6B-1/2/3, Gates 1-3), Operator Surface (PRs #49-#52), Dynamic Workflow Batches 1-7, Macro-Orchestrator Phases 1-5, Self-Hosted GA Readiness SG-1 through SG-5, HA Hardening HA-1 through HA-6, Dynamic Regulator MVP Phases 1-5.

**Agent Autonomous Maintenance Mode is active.** Agents autonomously maintain repo health, docs hygiene, CI correctness, and low-risk PR flow. CI green is the merge/success standard. Documentation maintenance means update/prune/archive, not accumulate. See playbook section "Agent Autonomous Maintenance Mode" for the full loop and rules.

**Strategic background:** `docs/archive/strategy/DYNAMIC_GLOBAL_REGULATOR_PLAN.md` (read only when drafting a v2 strategic proposal, not at every session start).

## Architecture refactor (R-series)

**Architecture Refactor R-series**: **SEALED AT R7.** R8 is not approved. No further R-series file splitting is approved.

## Disallowed by Default

The following require explicit human approval and a new implementation plan:

- Cloud SaaS, hosted/cloud deployment, and multi-tenant service.
- Hard process/container/VM sandbox isolation.
- Target-repository writes/apply/merge/deploy authority.
- Default-on provider execution or default-on CLI execution.
- Unattended workers or unattended autonomous-agent loops.
- Provider failover.
- Production worker concurrency.

These are v2/out-of-scope product boundaries, not current bugs. Moving any item into scope requires a new plan, threat-model update, focused tests, and explicit human approval.

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

Autonomously maintain repo health and fix CI/docs/test drift. No future core-completion phase remains. The following paths are allowed:

- Autonomous maintenance: repair stale docs, CI breakage, test drift, wire-codegen drift
- Regression hardening: add/repair tests for existing behavior
- Docs/CI/test drift repair
- Pilots: real-world task validation
- v2 proposals: new features, boundary expansions, or architectural changes

## Product Boundary Repair Track — COMPLETE

The product-boundary repair track closed the gap between product wording, dashboard behavior, and practical usability. This was a maintenance/product-polish track, not a new runtime authority track.

Completed PRs:

| Slice | Branch | Goal | Scope |
|---|---|---|---|
| P0 | `codex/p0-boundary-lint` / PR #64 | Align dashboard boundary wording and checks | Replaced read-only dashboard lint with boundary lint across dashboard app/components/lib; updated live E2E dashboard assertion |
| P3 | `codex/p3-out-of-scope-docs` / PR #65 | Make non-goals explicit | Documented cloud/SaaS, multi-tenant, hard sandbox, target writes/apply/merge/deploy, default-on provider, unattended workers, provider failover, and production worker concurrency as v2/out-of-scope |
| P1 | `codex/p1-runtime-gates` / PR #66 | Make local gates understandable | Added runtime-gate visibility and shortest local operator path for provider/CLI/auth/workspace/export gates |
| P2 | `codex/p2-primary-workflow` / PR #67 | Add a clear dashboard main workflow | Surfaced create/select run, tick, inspect failure/status, retry/fix, approve, and export readiness as a guided path using existing APIs |

Latest `main` CI after P0-P3 is green. No further Product Boundary Repair slices are planned.

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` only when status facts are unclear or the task updates status.
2. Read `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR/merge/CI work, docs cleanup, and real-world pilot tasks.
3. Confirm the proposed task is allowed under the safety gates above.
4. Keep the change commit-sized and run the relevant verification.
5. Run `uv run --no-project python scripts/check_agent_handoff.py`.
6. Update handoff docs before committing and pushing.
