# Next Decision

## Default Recommendation

**Autonomously maintain and advance safe repository work.** The completed Stage 0–4 task-book scope, CA-7 sealed baseline, Harness App MVP0–MVP8, Trials 0–3, Reliability Hardening 1, and Dispatch Kernel Phase 1–7 (including 6A, 6B-1/2/3, Gates 1–3, and all Phase 7 modules: sdk, doc_generator, community_profiles, tool_adapter, dashboard, benchmark) are complete. The responsible coding agent should keep the repo healthy, fix verification/documentation drift, and advance documented dispatch-kernel work that stays inside the hard boundaries.

This is standing authorization for the external coding agent maintaining this repository. It is not authorization to implement real autonomous workers inside the harness runtime.

## Allowed Next Paths

The responsible coding agent may choose any of the following without asking for a new instruction each time, provided the work is small enough to verify and all hard boundaries remain intact.

| Path | Description |
|---|---|
| Autonomous maintenance loop | Repair stale docs, branch/test count drift, CI breakage, security baseline failures, and handoff gaps. |
| Focused regression hardening | Add or repair tests for existing behavior when review findings, failing tests, or code inspection identify a concrete risk. |
| Dispatch-kernel phase work | Plan and implement the next architecture-book-defined phase only when it can remain deterministic, local, test-first, and free of real providers, real sandbox/process execution, target writes, deployment, and real worker processes. |
| Architecture/documentation closeout | Update architecture records, module maps, closeout reports, and handoff docs after accepted changes. |
| Demo/docs polish | Refine demo docs when verification or user feedback identifies a concrete gap. |
| Language migration | Agent-control-plane migration phases 0-8 are implemented and recorded in `docs/AGENT_CONTROL_PLANE_MIGRATION_CLOSEOUT.md`. Rust engine/API parity is implemented through the local axum health/ready/openapi/dispatch router (422 Rust tests, 35 modules, 31 test files). Phase 5 codegen plus TypeScript/Python REST SDK packages, Phase 6 read-only Next.js dashboard, and Phase 7 local Docker deploy are implemented and smoke-tested. No further migration implementation slice is known inside the approved scope. Providers remain stub/off; no real workers, target writes, executable dashboard controls, SDK publishing, or production deployment. |

## Disallowed by Default

The following are **not** allowed without explicit human approval and a new implementation plan:

- **MVP9** — no MVP9 scope has been defined.
- **CA-8** — CA-7 is sealed. No CA-8 exists.
- **Original task-book Stage 5** — no Stage 5 implementation has been started.
- **Provider/model integration** — real API calls to model providers.
- **Sandbox/process/container/VM execution** — real isolation beyond logical file claims.
- **Runtime autonomous workers** — real concurrent worker processes.
- **Target repo writes** — any mutation of registered target repositories.
- **Approval/run/execute/deploy/merge controls** — any execution or deployment mechanism.
- **Productionization** — hosted service, production UI, deployment, auth/multitenancy, or user-facing release.

The language migration target does not by itself approve productionization, real provider calls, real sandbox/process execution, deployment, target-repo writes, or executable UI controls.

Python reference implementation remains in `src/harness_core/` until an explicit future removal or relocation decision is approved.

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` to confirm the latest state.
2. Confirm the proposed track is not in the disallowed list above.
3. Confirm the work has an architecture-book, test, issue, review finding, or documentation-drift basis.
4. Keep the change commit-sized and run the relevant verification.
5. Run `python3 scripts/check_agent_handoff.py`.
6. Update handoff docs before committing and pushing.
