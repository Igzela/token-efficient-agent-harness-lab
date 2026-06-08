# Next Decision

## Default Recommendation

All prior tracks complete (Dispatch Kernel 1-7, Dynamic Workflow 1-7, Macro-Orchestrator 1-5, Self-Hosted GA SG-1–SG-5, HA-1–HA-6, Dormant Adaptation, Productization 1-7, PostgreSQL backend). Autonomously maintain repo health and fix CI/docs/test drift. No new track is approved without user direction.

This is standing authorization for the external coding agent maintaining this repository. It is not authorization to implement real autonomous workers inside the harness runtime.

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

## Allowed Next Paths

| Path | Status |
|---|---|
| Autonomous maintenance | Repair stale docs, CI breakage, test drift, wire-codegen drift. |
| Regression hardening | Add/repair tests for existing behavior when risk is found. |
| Dashboard Onboarding UX | **ACTIVE** — ON-1 through ON-5. |
| Architecture/doc closeout | Update records after accepted changes. |
| Demo/docs polish | Refine when gaps identified. |
| CLI executor routing | Opt-in via `ACP_ENABLE_CLI_EXECUTION=1`. Maintenance only. |
| Dynamic workflow | All 7 batches complete. Scheduler dynamic mode wired. Maintenance. |
| Macro-orchestrator | All 5 phases complete. Maintenance. |
| Language migration | Rust+TS cutover complete. Python = REST SDK only. |
| HA hardening | All 6 phases complete. 1378 tests. |
| PostgreSQL backend | Complete. Optional via `ACP_DATABASE_URL`. |

## Disallowed by Default

The following require explicit human approval and a new implementation plan:

- MVP9 — no scope defined
- Provider/model productionization — no broadening beyond existing env-gated local beta
- Sandbox/process/container/VM execution — no expansion beyond CLI executor path
- Runtime autonomous workers — no concurrent worker processes
- Target repo writes — no mutation of registered repositories
- Approval/run/execute/deploy/merge controls — no execution mechanisms
- Cloud productionization — no hosted/SaaS/multi-tenant deployment

Planning-only metadata does not approve execution. Supervised execution primitives in app-owned workspaces do not approve sandbox, target mutation, or hosted deployment.

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` to confirm latest state.
2. Confirm the proposed track is not in the disallowed list.
3. Confirm the work has an architecture-book, test, issue, review finding, or documentation-drift basis.
4. Keep the change commit-sized and run the relevant verification.
5. Run `uv run --no-project python scripts/check_agent_handoff.py` (includes `scripts/check_wire_codegen_drift.sh`).
6. Update handoff docs before committing and pushing.
