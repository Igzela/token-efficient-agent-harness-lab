# Next Decision

## Default Recommendation

**Autonomously maintain and advance safe repository work.** The completed Stage 0–4 task-book scope, CA-7 sealed baseline, Harness App MVP0–MVP8, Trials 0–3, Reliability Hardening 1, and Dispatch Kernel Phase 1–7 (including 6A, 6B-1/2/3, Gates 1–3, and all Phase 7 modules: sdk, doc_generator, community_profiles, tool_adapter, dashboard, benchmark) are complete. The responsible coding agent should keep the repo healthy, fix verification/documentation drift, and advance documented dispatch-kernel work that stays inside the hard boundaries.

This is standing authorization for the external coding agent maintaining this repository. It is not authorization to implement real autonomous workers inside the harness runtime.

This file is the single forward-plan surface. Do not add parallel roadmap, next-steps, or productization-plan documents; update this file and prune stale planning text instead.

## Allowed Next Paths

The responsible coding agent may choose any of the following without asking for a new instruction each time, provided the work is small enough to verify and all hard boundaries remain intact.

| Path | Description |
|---|---|
| Autonomous maintenance loop | Repair stale docs, branch/test count drift, CI breakage, security baseline failures, and handoff gaps. |
| Focused regression hardening | Add or repair tests for existing behavior when review findings, failing tests, or code inspection identify a concrete risk. |
| Dispatch-kernel phase work | Plan and implement the next architecture-book-defined phase only when it can remain deterministic, local, test-first, and free of broader real-provider behavior, real sandbox/process execution, target writes, deployment, and real worker processes. Existing provider adapters are explicit env-gated beta paths and must remain default-off. |
| Architecture/documentation closeout | Update architecture records, module maps, closeout reports, and handoff docs after accepted changes. |
| Demo/docs polish | Refine demo docs when verification or user feedback identifies a concrete gap. |
| Language migration | Agent-control-plane migration phases 0-8 are implemented and recorded in `docs/AGENT_CONTROL_PLANE_MIGRATION_CLOSEOUT.md`. Rust + TypeScript cutover is complete: Rust `engine/` is the primary runtime/API/storage/provider-gated control plane, and `dashboard/` plus `sdk/typescript/` are the primary TypeScript surfaces. Python remains only as legacy reference plus retained Python SDK compatibility. No real workers, target writes, SDK publishing, or cloud production deployment. |
| Local small-team hardening | Productization Phases 1-7 complete (Provider Safety Gate, Permission Governance, Cost Governance, Data Operations, Native Packaging, Dashboard Controls, Long-Run Hardening). All planned phases done. Keep provider execution default-off and explicit; keep target writes, sandbox/process execution, real workers, and cloud SaaS out of scope. |
| CLI executor routing | Complexity-based dispatch to Claude Code CLI / Codex CLI implemented. Can extend: interactive session persistence, additional CLI tools, adaptive routing feedback for CLI tiers, CLI-specific execution gates. |

## Local Productization Plan

Current level: local self-hosted MVP / internal beta.

Implemented:

- one Rust engine process serves API plus static dashboard without Docker
- local SQLite persists dispatch history, config, team/API-key metadata, audit, costs, provider audit, and provider usage columns
- dashboard reads live local state
- TypeScript and Python SDKs cover local API, state, provider health/audit, export, and backup
- provider execution is explicit, env-gated, auth-gated, execute-scope-gated, audited, and budget-capped
- team/API-key create, revoke, rotate, delete, scope update, role update, last-used tracking, expiry, and admin audit events are implemented
- cost governance: reserved vs estimated cost separation, per-tier and daily cost breakdown, utilization ratio, token usage totals, per-dispatch cost detail endpoint, typed SDK cost responses
- data operations: versioned SQLite migrations, integrity checks, import/export roundtrip, hardened backup restore with verification, data-directory documentation
- native packaging: `.env.example`, install/upgrade scripts, release tarball with engine binary + static dashboard + scripts, native smoke verification
- dashboard controls: dispatch detail drill-down, backups tab with create/restore/delete and confirmation dialogs, audit log tab, Team tab confirmation dialogs, provider health in Settings, dispatch detail/list-backups/delete-backup API endpoints, 6 new SDK methods per SDK
- product-readiness repair pass: smoke endpoint drift fixed with guard test, hardcoded timestamps replaced with injectable clock, CLI executor timeout enforced via spawn_with_timeout, dashboard protected-mode auth flow with token input panel, dashboard error visibility for all tabs, threat model rewritten for current state
- P1 local-beta follow-up: GET /api/v1/keys metadata-only key list, search/filter/pagination for dispatches and audit, bookmarkable tabs via URL hash, 60-second auto-refresh with visibility-aware pausing, Docker volume persistence, key reveal modal replacing alert(), dashboard split into 12 focused components
- P2 local-beta polish & type hardening: CSS design token cleanup (#c0392b → var(--risk), utility classes), TypeScript SDK type hardening (22 new focused response interfaces, 21 methods typed), dashboard component quality (usePaginatedSearch hook, SearchBar, Pagination components), Next.js app polish (loading.tsx, error.tsx, metadata, favicon)

Next productization phases:

| Order | Phase | Done When |
|---|---|---|
| 7 | Long-Run Hardening | **COMPLETE** — SQLite contention tests ✓, provider failure matrix ✓, audit integrity review ✓ (7 tests), upgrade smoke verification ✓ (tarball structure, install smoke, data preservation, port retry, integrity endpoint). LAN threat model exists at `docs/security/THREAT_MODEL.md`. GitHub Actions clean (Node 22, latest action versions). |

All planned productization phases (1–7) are complete. No Phase 8 is defined. The agent should maintain repo health (CI, docs, test drift, security baseline) until the user provides new direction or defines a new phase.

## Disallowed by Default

The following are **not** allowed without explicit human approval and a new implementation plan:

- **MVP9** — no MVP9 scope has been defined.
- **CA-8** — CA-7 is sealed. No CA-8 exists.
- **Original task-book Stage 5** — no Stage 5 implementation has been started.
- **Provider/model productionization** — broadening real API calls beyond the existing explicit env-gated local beta path, enabling providers by default, or adding unattended provider execution.
- **Sandbox/process/container/VM execution** — real isolation beyond logical file claims.
- **Runtime autonomous workers** — real concurrent worker processes.
- **Target repo writes** — any mutation of registered target repositories.
- **Approval/run/execute/deploy/merge controls** — any execution or deployment mechanism.
- **Cloud productionization** — hosted service, SaaS deployment, production multi-tenant service, or remote user-facing release.

The local small-team track does not approve cloud hosting, default-on provider calls, real sandbox/process execution, target-repo writes, hosted deployment, or real autonomous workers.

Python reference implementation remains in `src/harness_core/` as legacy reference. Do not expand it for new runtime features; new primary runtime work belongs in Rust and TypeScript unless a compatibility fix is explicitly needed.

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` to confirm the latest state.
2. Confirm the proposed track is not in the disallowed list above.
3. Confirm the work has an architecture-book, test, issue, review finding, or documentation-drift basis.
4. Keep the change commit-sized and run the relevant verification.
5. Run `python3 scripts/check_agent_handoff.py`.
6. Update handoff docs before committing and pushing.
