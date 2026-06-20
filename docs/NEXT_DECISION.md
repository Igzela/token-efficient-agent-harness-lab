# Next Decision

## Core Plan COMPLETE

**Phase 8 DONE. Core plan COMPLETE.** No future core-completion phase remains. Future work is maintenance, bugfixes, pilots, or v2 proposals only. Historical gap inventory and seal evidence are archived at `docs/archive/phase-closeouts/PHASE8_FINAL_COMPLETION_PLAN.md`.

**Completed phases:** Phase 1-7 (including 6A, 6B-1/2/3, Gates 1-3), Operator Surface (PRs #49-#52), Dynamic Workflow Batches 1-7, Macro-Orchestrator Phases 1-5, Self-Hosted GA Readiness SG-1 through SG-5, HA Hardening HA-1 through HA-6, Dynamic Regulator MVP Phases 1-5.

**Agent Autonomous Maintenance Mode is active.** Agents autonomously maintain repo health, docs hygiene, CI correctness, and low-risk PR flow. CI green is the merge/success standard. Documentation maintenance means update/prune/archive, not accumulate. See playbook section "Agent Autonomous Maintenance Mode" for the full loop and rules.

**Strategic background:** `docs/archive/strategy/DYNAMIC_GLOBAL_REGULATOR_PLAN.md` (read only when drafting a v2 strategic proposal, not at every session start).

## Architecture refactor (R-series)

**Architecture Refactor R-series**: **SEALED AT R7.** R8 is not approved. No further R-series file splitting is approved.

## V2 Real Production Output Track — AUTHORIZED

Human approval on 2026-06-17 authorizes a new V2 productization track: turn the local control plane into a system that can produce auditable patches or PR branches for real repositories.

This approval does **not** remove safety limits. It turns selected old limits into production guardrails that must land phase by phase, behind explicit gates, with audit evidence, tests, and rollback/kill paths. Until a V2 phase merges, current v1 behavior remains authoritative.

Target V2 user flow:

```text
connect real repo -> create task -> isolated app-owned workspace execution
-> code changes -> verification -> evidence/diff -> human approval
-> push PR branch or export patch
```

Current hard constraints for the V2 track:

- Provider API execution remains default-off. Installed local Claude/Codex CLIs are discovered by default for explicit workflow ticks; set `ACP_ENABLE_CLI_EXECUTION=0` to disable them.
- V2-3 target output is merged; target writes are allowed only through its controlled worktree plus `acp/*` branch push or patch export, never direct target working-tree or `main` writes.
- V2-1 may harden app-owned workspace isolation; process/container/VM sandboxing remains a separate approval item unless explicitly added to a future plan.
- V2-4 may add bounded supervised workers with lease/heartbeat/kill controls; unattended autonomous-agent loops remain disallowed.
- Hosted/cloud/multi-tenant SaaS, app-runtime release/deploy controls, provider failover, and default-on provider API execution remain out of scope for this track.
- Secrets must not appear in logs, diffs, artifacts, dashboard output, or PR bodies.

## Disallowed by Default

Outside explicitly merged V2 phases, the following remain disabled:

- Cloud SaaS, hosted/cloud deployment, and multi-tenant service.
- Process/container/VM sandbox isolation.
- Uncontrolled target working-tree writes, direct `main` writes, apply/merge/deploy authority, and release/tag controls.
- Default-on provider API execution.
- Unattended autonomous-agent loops.
- Provider failover.
- Production worker concurrency outside the V2-4 supervised lease/heartbeat model.

## Safety Gates

| Gate | Rule |
|---|---|
| No secrets committed | `scripts/acp_secret_scan.py` enforced |
| No merge on failing CI | all jobs must pass |
| No unlogged execution | ledger records all dispatches |
| Rollback path required | `git revert` sufficient for low-risk |
| Provider execution | env-gated (`ACP_ENABLE_PROVIDER_EXECUTION=1`) |
| CLI execution | local CLI discovery defaults on; `ACP_ENABLE_CLI_EXECUTION=0` disables it |
| Target repo output | V2 branch/worktree/PR flow only; no direct `main` writes |
| V2 real output | explicit phase gate, audit event, tests, rollback/kill path |
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
- V2 Real Production Output PRs that follow the phase plan below

## Real Output Closeout — IMPLEMENTED

Human approval on 2026-06-20 authorizes the final local-product closeout. This is not a new runtime kernel or unattended-agent track. It completes the existing V2 path in this order:

1. Preserve the workflow plan `raw_request` as the default CLI/provider node prompt.
2. Add a bounded supervised task loop in the existing workspace/executor/artifact path: one app-owned git worktree, explicit CLI executor, detected or supplied verification commands, at most two repair attempts, recorded verification evidence, and existing pause/kill/time/cost gates.
3. After an approved `acp/*` branch push, optionally create a GitHub pull request through an explicit GitHub token environment reference and repository/host allowlist. No merge authority is added.
4. Align release workflow, installer, and README artifact naming, then publish the first verified release.
5. Validate the flow against three independent disposable real git repositories and record compact evidence in the existing status/runbook surfaces.
6. Make the dashboard task-first: task prompt, repository, executor, verification, diff/evidence, approval, and PR result are primary; operations/admin views remain available as secondary navigation.

Implementation status:

- Items 1-3, 5, and 6 are complete on `codex/real-output-closeout`.
- Release naming, package layout, installer behavior, and local 16-check release smoke are complete.
- Remaining external action: merge the closeout PR, tag `v0.1.0`, wait for the release workflow, then run the online installer against the published asset.

Acceptance:

- Chinese and English prompts reach the selected CLI/provider unchanged unless an explicit command override is supplied.
- Failed verification can trigger no more than two audited repair attempts; exhausted verification blocks approval-bound output.
- Verification output, exit status, command, attempt, and timestamp are bound to the captured artifact.
- GitHub PR creation is default-off, explicit, audited, and returns the real PR URL; direct `main`, merge, release, deploy, and apply authority remain unavailable.
- The local package/installer smoke passes; published-asset verification is the release closeout step after merge.
- Three pilots produce distinct verified `acp/*` branches or PRs while each target `main` remains unchanged.

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

## V2 Phase Plan

Use this as the single forward plan. Do not create new roadmap/status docs for V2. If a phase grows too large, split by vertical acceptance criteria while preserving the same phase order.

| Phase | Branch | Goal | Required acceptance |
|---|---|---|---|
| V2-0 | `codex/v2-real-production-output` | Authorize and document the track | Merged in PR #69 |
| V2-1 | `codex/v2-1-execution-safety-base` | Real execution safety base | Merged in PR #70 |
| V2-2 | `codex/v2-2-provider-cli-output` | Real provider/CLI output path | Merged in PR #71 |
| V2-3 | `codex/v2-3-target-repo-pr-flow` | Target repo branch/PR output | Merged in PR #72 |
| V2-4 | `codex/v2-4-supervised-worker-queue` | Bounded production worker queue | Merged in PR #73: dual env gate, bounded worker count, atomic lease claim, worker heartbeat, stale recovery audit, pause/resume/kill API, auth scope, kill switch, SDK/tests |
| V2-5 | `codex/v2-5-product-output-ux` | Product-grade main workflow | Merged in PR #75: Mission Control path for task/run creation, tick, workspace, capture, approval binding, export/target output, scheduler control, visible gates, responsive layout |

V2 implementation routing:

- V2-1 starts in `engine/src/storage/local_product_store/supervised_patch.rs`, `engine/src/http_server/handlers/supervised_patch.rs`, `engine/src/node_executor.rs`, and focused storage/API tests.
- V2-2 starts in `engine/src/provider/`, `engine/src/cli/`, `engine/src/executor/`, `engine/src/dispatch_engine.rs`, and provider/CLI tests.
- V2-3 is owned by `engine/src/target_repo_output.rs`, supervised patch storage/API, and matching SDK/dashboard API contracts. Runtime gate: `ACP_ENABLE_TARGET_REPO_OUTPUT=1`; emergency kill: `ACP_TARGET_REPO_OUTPUT_KILL_SWITCH=1`.
- V2-4 starts in `engine/src/scheduler.rs`, `engine/src/workflow/run_queue.rs`, `engine/src/executor_pool.rs`, and `engine/src/storage/local_product_store/heartbeat.rs`.
- V2-5 starts in `dashboard/src/components/MissionControl.tsx`, `SupervisedPatch.tsx`, `RuntimeGates.tsx`, and `dashboard/src/lib/api-client.ts`.

Every V2 PR must list: completed phase, intentionally unfinished phases, verification, residual risk, rollback path, and next PR.

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` only when status facts are unclear or the task updates status.
2. Read `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR/merge/CI work, docs cleanup, and real-world pilot tasks.
3. Confirm the proposed task is allowed under the safety gates above.
4. Keep the change commit-sized and run the relevant verification.
5. Run `uv run --no-project python scripts/check_agent_handoff.py`.
6. Update handoff docs before committing and pushing.
