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

## Product Boundary Repair Track

The next recommended maintenance track is to close the gap between product wording, dashboard behavior, and practical usability. This is a maintenance/product-polish track, not a new runtime authority track.

Baseline requirement: land the documentation-pruning cleanup first so all follow-up branches start from the six-file active docs model.

Use separate branches and PRs:

| Slice | Branch | Goal | Scope |
|---|---|---|---|
| P0 | `codex/p0-boundary-lint` | Align dashboard boundary wording and checks | Use "local operations console with guarded app-owned controls"; enforce boundary lint across dashboard app/components/lib; update live E2E dashboard assertion |
| P1 | `codex/p1-runtime-gates` | Make local gates understandable | Add runtime-gate visibility and shortest local operator path; focus on provider/CLI/auth/workspace/export gates |
| P2 | `codex/p2-primary-workflow` | Add a clear dashboard main workflow | Surface create/select run, tick, inspect failure, retry/fix, approve, export as one guided path using existing APIs |
| P3 | `codex/p3-out-of-scope-docs` | Make non-goals explicit | Document cloud/SaaS, multi-tenant, hard sandbox, target writes/apply/merge/deploy, default-on provider, unattended workers, provider failover, and production worker concurrency as v2/out-of-scope |

Recommended merge order:

1. Baseline docs cleanup
2. P0 boundary lint
3. P3 out-of-scope docs
4. P1 runtime gates
5. P2 primary workflow

Hard constraints for all slices:

- Start from latest `main` on a new `codex/` branch; do not commit on `main`.
- Stop and report if the working tree is dirty before starting.
- Do not add sandbox/container/VM isolation, target-repository writes, deploy/apply/release controls, default-on provider/CLI execution, provider failover, hosted/cloud/multi-tenant behavior, or unattended autonomous workers.
- Do not add new docs; update only the six active docs when documentation changes.
- Keep each PR inside its slice. Do not opportunistically refactor adjacent code.
- Open PRs only; do not auto-merge.

Required verification for each slice:

```bash
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

Dashboard slices must also run the relevant dashboard lint/typecheck/build command from `dashboard/package.json`. Script changes must run Python syntax checks for touched Python files.

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` only when status facts are unclear or the task updates status.
2. Read `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR/merge/CI work, docs cleanup, and real-world pilot tasks.
3. Confirm the proposed task is allowed under the safety gates above.
4. Keep the change commit-sized and run the relevant verification.
5. Run `uv run --no-project python scripts/check_agent_handoff.py`.
6. Update handoff docs before committing and pushing.
