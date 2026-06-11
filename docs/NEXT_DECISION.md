# Next Decision

## Active Track: Real-World Testing Mode

The project has entered **Real-World Testing Mode**. The old posture of blanket restriction is replaced with controlled autonomy. Except for necessary safety gates and security components, the system is authorized to operate on real tasks, real branches, real commits, real PRs, real CI, and gated auto-merge.

**Dynamic Global Regulator** is the active strategic direction, validated through real-world testing. See `docs/DYNAMIC_GLOBAL_REGULATOR_PLAN.md`.

This file is the single forward-plan surface. Do not add parallel roadmap, next-steps, or productization-plan documents; update this file and prune stale planning text instead.

## Architecture refactor (R-series)

**Architecture Refactor R-series**: **SEALED AT R7.** R8 is not approved. No further R-series file splitting is approved.

## Active Track: Dashboard Onboarding UX

**COMPLETE — ON-1 through ON-5.** All phases implemented and verified. 1379 Rust tests pass, dashboard builds clean.

| Phase | Scope | Status |
|---|---|---|
| **ON-1** | Tab grouping (Monitor/System/Admin) + Welcome Panel | ✅ Done |
| **ON-2** | Empty state overhaul | ✅ Done |
| **ON-3** | Term tooltip system | ✅ Done |
| **ON-4** | Copy + boundary fixes | ✅ Done |
| **ON-5** | Polish | ✅ Done |

## Dynamic Global Regulator

`docs/DYNAMIC_GLOBAL_REGULATOR_PLAN.md` defines the 7-phase roadmap for the Dynamic Global Regulator. Phases are implemented incrementally through real-world testing. Each phase has explicit acceptance tests, safety gates, and rollback strategy.

## Allowed Next Paths

The following paths are allowed under Real-World Testing Mode. Autonomously maintain repo health and fix CI/docs/test drift.

## Allowed Actions (Real-World Testing Mode)

| Action | Gate |
|---|---|
| Branch creation | Any task that needs isolation |
| Target repo edits via branch + PR | CI must pass before merge |
| Commits | Must pass fmt/clippy |
| PR creation | Auto-created for non-trivial changes |
| CI triggering and CI repair | CI failures fixed before merge |
| Docs/tests/small code fixes | Low-risk, auto-merge eligible |
| Dynamic workflow fix/test node injection | Within existing bounds |
| Low-risk auto-merge after CI green | See Auto-Merge Policy in regulator plan |

## Disallowed by Default

The following require explicit human approval and a new implementation plan under the safety gate framework:

## Requires Safety Gate / Explicit Approval

| Action | Requirement |
|---|---|
| Provider/CLI execution boundary expansion | Explicit user approval |
| Auth/security boundary changes | Explicit user approval |
| Database migrations | Explicit user approval |
| Release/tag/deploy | Explicit user approval |
| Active YAML/rublic/policy mutation | Explicit user approval |
| Destructive or irreversible operations | Explicit user approval |
| Sandbox/process/container/VM expansion | Explicit user approval |

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` to confirm latest state.
2. Confirm the proposed track is not in the disallowed list.
3. Confirm the work has an architecture-book, test, issue, review finding, or documentation-drift basis.
4. Keep the change commit-sized and run the relevant verification.
5. Run `uv run --no-project python scripts/check_agent_handoff.py` (includes `scripts/check_wire_codegen_drift.sh`).
6. Update handoff docs before committing and pushing.
