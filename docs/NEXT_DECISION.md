# Next Decision

## Default Recommendation

**Autonomously maintain and advance safe repository work.** The completed Stage 0–4 task-book scope, CA-7 sealed baseline, Harness App MVP0–MVP8, Trials 0–3, Reliability Hardening 1, and Dispatch Kernel Phase 1–7 (including 6A, 6B-1/2/3, Gates 1–3, and all Phase 7 modules: sdk, doc_generator, community_profiles, tool_adapter, dashboard, benchmark) are complete. The responsible coding agent should keep the repo healthy and fix CI/docs/test drift plus wire-governance gaps. Do not continue R-series file splitting.

This is standing authorization for the external coding agent maintaining this repository. It is not authorization to implement real autonomous workers inside the harness runtime.

This file is the single forward-plan surface. Do not add parallel roadmap, next-steps, or productization-plan documents; update this file and prune stale planning text instead.

## Allowed Next Paths

The responsible coding agent may choose any of the following without asking for a new instruction each time, provided the work is small enough to verify and all hard boundaries remain intact.

| Path | Description |
|---|---|
| Autonomous maintenance loop | Repair stale docs, branch/test count drift, CI breakage, security baseline failures, handoff gaps, and wire-codegen guard drift. |
| Focused regression hardening | Add or repair tests for existing behavior when review findings, failing tests, or code inspection identify a concrete risk. |
| Dispatch-kernel phase work | Plan and implement the next architecture-book-defined phase only when it can remain deterministic, local, test-first, and free of broader real-provider behavior, sandbox isolation, subprocess expansion beyond the existing CLI executor path, target writes, deployment, and real worker processes. Existing provider adapters are explicit env-gated beta paths and must remain default-off. |
| Architecture/documentation closeout | Update architecture records, module maps, closeout reports, and handoff docs after accepted changes. |
| Demo/docs polish | Refine demo docs when verification or user feedback identifies a concrete gap. |
| Language migration | Agent-control-plane migration phases 0-8 are implemented and recorded in `docs/AGENT_CONTROL_PLANE_MIGRATION_CLOSEOUT.md`. Rust + TypeScript cutover is complete: Rust `engine/` is the primary runtime/API/storage/provider-gated control plane, and `dashboard/` plus `sdk/typescript/` are the primary TypeScript surfaces. Python retained as REST SDK and utility scripts only; legacy reference implementation retired. No real workers, target writes, SDK publishing, or cloud production deployment. |
| Local small-team hardening | Productization Phases 1-7 complete (Provider Safety Gate, Permission Governance, Cost Governance, Data Operations, Native Packaging, Dashboard Controls, Long-Run Hardening). All planned phases done. Keep provider execution default-off and explicit; keep target writes, sandbox/process execution, real workers, and cloud SaaS out of scope. |
| CLI executor routing | Complexity-based dispatch to Claude Code CLI / Codex CLI implemented as a pre-existing local subprocess exception. It is explicit opt-in via `ACP_ENABLE_CLI_EXECUTION=1`; unavailable or disabled CLI tiers fall back to noop. Trial 5 controlled beta validation is closed. Maintenance only. Any expansion requires an explicit new plan and approval. |
| Supervised autonomous beta planning | Planning-only track accepted in ADR-0002. Batch 0-6 governance/module/model/read-only-planner/durable-state/advisory/design-gate work is complete. Batch 7 Slice A storage-only metadata, Slice B read-only HTTP visibility, and Slice C read-only SDK visibility are implemented: app-owned SQLite records for detached patch workspace/artifact metadata, path-boundary tests outside registered target repositories, export/import/integrity/stats coverage, GET-only metadata routes, and TypeScript/Python SDK GET wrappers. Supervised execution runtime remains NO-GO: no workspace directory creation, patch generation, approval/export gate, workers, target writes, sandbox/process/container/VM execution, deploy/merge/apply controls, or default-on provider calls are approved. |
| Architecture refactor (R-series) | **SEALED AT R7.** R1–R7 are complete. R8 is not approved. The `checkpoint.rs` split and `dispatch_decision.rs` split are deferred. No further R-series file splitting is approved. |

## Supervised Autonomous Beta Planning

Current level: planning-only track, no execution authority.

Authoritative ADR: `docs/adr/0002-supervised-planning-track.md`.

Batch status:

| Batch | Scope | Status |
|---|---|---|
| 0 | Governance and boundary confirmation | Complete as documentation/audit scope. |
| 1 | Module reachability audit and classification | Complete as documentation/audit scope. |
| 2 | DAG/workflow canonical model decision | Complete as documentation/design scope; `WorkflowGraph` is canonical. |
| 3 | Read-only planner API plus app-owned SQLite plan state | Complete as planning-only implementation; `/api/v1/plans` creates/lists/reads non-executable app-owned `WorkflowGraph` plans. |
| 4 | Durable workflow run/node/edge/event/approval state | Complete as inert app-owned state; `/api/v1/workflow-runs` creates/lists/reads run metadata, records events/approvals, and records resume/cancel intent without execution authority. |
| 5 | Quality/routing/retry/observability recommendation path | Complete as recommendation-only plan advisory metadata; no provider invocation, live worker routing, retry execution, target writes, or execution authority. |
| 6 | Sandbox, target workspace, approval broker, rollback, artifact-capture design gate | Complete as documentation/design only in ADR-0002 and `docs/security/THREAT_MODEL.md`; no implementation. |
| 7 | Supervised execution beta | **Slice A/B/C implemented; runtime NO-GO**. Storage-only app-owned workspace/artifact metadata now exists in `LocalProductStore` with schema v3, path-boundary tests, and export/import/integrity/stats coverage. GET-only HTTP routes and TypeScript/Python SDK methods expose that metadata for read-only inspection. No workspace creation, patch generation, approval broker, artifact file capture, dashboard UI controls, create/update/delete routes, or execution runtime exists. |

Batch 2 selects `WorkflowGraph` as the canonical planning and persistence model. `DAGState` remains the graph-mutation model for versioned proposals/rollback, and scheduling-local `DagState` remains the concurrency view for file-overlap scheduling. Batch 3 implemented the read-only planner without R8, file splitting, target writes, worker runtime, provider calls, sandbox/process/container/VM execution, or execution controls. Batch 4 persists workflow run/node/edge/event/approval records only as app-owned state; resume/cancel endpoints record metadata and update stored status only, with no worker or execution authority. Batch 5 connects quality/routing/retry/observability into planning decisions only as `advisory` status/block/recommendation metadata. Batch 6 documents future sandbox/workspace/approval/rollback/artifact requirements and threat-model risks. Batch 7 Slice A implements storage-only metadata for app-owned detached patch workspace/artifact records; Slice B exposes those records through GET-only HTTP routes; Slice C exposes those same GET routes through TypeScript/Python SDK methods. These slices do not implement or authorize execution.

Batch 7 readiness audit outcome:

| Prerequisite | Current evidence | Status |
|---|---|---|
| Isolation primitive selected | ADR-0002 selects app-owned detached patch workspace/snapshot for the first patch artifact slice and rejects registered-target `git worktree add` because it mutates target `.git/worktrees`. Slice A records metadata/path evidence only. No process/container/VM execution primitive is selected. | Storage-only metadata implemented; execution primitive not selected |
| Target workspace contract | Slice A stores source revision evidence, target/workspace canonical paths, lifecycle status, and boundary JSON in app-owned SQLite. Slice B exposes this metadata through read-only GET routes, and Slice C adds read-only SDK wrappers. Path-boundary tests reject workspace paths inside registered target repositories. It does not create directories or copy files. | Metadata schema/storage/API/SDK visibility implemented; lifecycle runtime missing |
| Approval broker scope/gate | ADR-0002 defines future `workflow:patch_review`-style evidence binding for patch artifact export. Batch 4 approvals remain `metadata_only` and `execution_authority=disabled`; no gate is wired. | Design specified; implementation missing |
| Rollback strategy/tests | ADR-0002 defines app-owned workspace discard/quarantine and target `.git` unchanged checks. No workspace rollback or failure-mode tests exist. | Design specified; tests missing |
| Artifact capture schema/storage | Slice A stores `supervised_patch_artifact.v1` metadata in app-owned SQLite with patch hash, normalized changed files, redaction status, storage refs, export/import, integrity, and stats coverage. Slice B exposes metadata only through read-only GET routes, and Slice C adds read-only SDK wrappers. It does not create patch files, run redaction, or expose/export artifacts through an approval gate. | Metadata schema/storage/API/SDK visibility implemented; capture runtime missing |
| Provider default-off | Existing provider gate remains default-off. | Satisfied, must be preserved |
| No push/merge/deploy/target mutation | Existing boundaries block these behaviors. | Satisfied, must be preserved |

Next safe action: define and approve one narrow Batch 7 Slice D before code. Recommended next slice is evidence-bound patch-review approval/export design or dashboard read-only visibility, but only as non-executable metadata/design. Do not add command execution, workspace directory creation, patch generation, target writes, provider calls, registered-target `git worktree add`, push/merge/deploy/apply controls, create/update/delete supervised-patch routes, or runtime workers.

## Local Productization Plan

Current level: local self-hosted MVP / internal beta.

Implemented:

- one Rust engine process serves API plus static dashboard without Docker
- local SQLite persists dispatch history, read-only workflow plans, inert workflow run state, config, team/API-key metadata, audit, costs, provider audit, and provider usage columns
- dashboard reads live local state
- TypeScript and Python SDKs cover local API, state, provider health/audit, supervised patch metadata, export, and backup
- provider execution is explicit, env-gated, auth-gated, execute-scope-gated, audited, and budget-capped
- team/API-key create, revoke, rotate, delete, scope update, role update, last-used tracking, expiry, and admin audit events are implemented
- cost governance: reserved vs estimated cost separation, explicit provider price env for estimated-cost availability, per-tier and daily cost breakdown, utilization ratio, token usage totals, per-dispatch cost detail endpoint, typed SDK cost responses
- data operations: versioned SQLite migrations, integrity checks, import/export roundtrip, hardened backup restore with verification, data-directory documentation
- native packaging: `.env.example`, install/upgrade scripts, release tarball with engine binary + static dashboard + scripts, native smoke verification
- dashboard controls: dispatch detail drill-down, backups tab with create/restore/delete and confirmation dialogs, audit log tab, Team tab confirmation dialogs, provider health in Settings, dispatch detail/list-backups/delete-backup API endpoints, 6 new SDK methods per SDK
- product-readiness repair pass: smoke endpoint drift fixed with guard test, hardcoded timestamps replaced with injectable clock, CLI executor timeout enforced via spawn_with_timeout, dashboard protected-mode auth flow with token input panel, dashboard error visibility for all tabs, threat model rewritten for current state
- P1 local-beta follow-up: GET /api/v1/keys metadata-only key list, search/filter/pagination for dispatches and audit, bookmarkable tabs via URL hash, 60-second auto-refresh with visibility-aware pausing, Docker volume persistence, key reveal modal replacing alert(), dashboard split into 12 focused components
- P2 local-beta polish & type hardening: CSS design token cleanup (#c0392b → var(--risk), utility classes), TypeScript SDK type hardening (22 new focused response interfaces, 21 methods typed), dashboard component quality (usePaginatedSearch hook, SearchBar, Pagination components), Next.js app polish (loading.tsx, error.tsx, metadata, favicon)
- Dashboard UX polish: ARIA tab roles + keyboard navigation, modal focus traps with Escape key, keyboard-accessible dispatch table rows, form labels on Team inputs, aria-label on icon buttons and search input, CSS spinner animation replacing plain-text loading states, shared EmptyState/StateBanner/BoundaryBadges components, local setup checklist plus setup/auth helper scripts, permission-aware Backups/Audit/Provider states, structured API error codes, server-side dispatch/audit pagination/search, actionable empty states, consolidated visual utility classes, and readable dispatch/provider/audit summaries with raw JSON behind details
- Production-like local beta ops hardening: guarded `.env.production-like.local.example` and startup script, `/api/v1/metrics`, dashboard Operations tab, `acp_ops_check.py`, backup verify and restore dry-run API/UI/script smoke, local env secret scan, audit redaction query, provider pricing visibility, read-only advisory risk-gate repair, and least-privilege scope templates
- Supervised planning Batch 3: `/api/v1/plans` creates/lists/reads non-executable app-owned `WorkflowGraph` plans in SQLite; SDKs expose plan methods
- Supervised planning Batch 4: `/api/v1/workflow-runs` creates/lists/reads inert workflow run metadata from plans, stores nodes/edges/events/approvals, records resume/cancel intent as metadata only, and exposes SDK methods
- Supervised planning Batch 5: read-only plan records include recommendation-only quality/routing/retry/observability advisory metadata for status/block/recommendation decisions only
- Supervised planning Batch 6: ADR-0002 and `docs/security/THREAT_MODEL.md` document sandbox/workspace/approval-broker/rollback/artifact-capture contracts and execution-phase risks as planning-only gates
- Supervised planning Batch 7 Slice A/B/C: app-owned SQLite `supervised_patch_workspaces` and `supervised_patch_artifacts` metadata records, schema v3 migration, path-boundary validation outside registered target repositories, normalized changed-file validation, stats, integrity, export/import coverage, GET-only HTTP metadata visibility, and TypeScript/Python SDK GET wrappers. No dashboard UI, workspace creation, patch generation, approval/export gate, rollback engine, target writes, workers, provider calls, create/update/delete supervised-patch routes, or execution controls.

Next productization phases:

| Order | Phase | Done When |
|---|---|---|
| 7 | Long-Run Hardening | **COMPLETE** — SQLite contention tests ✓, provider failure matrix ✓, audit integrity review ✓ (7 tests), upgrade smoke verification ✓ (tarball structure, install smoke, data preservation, port retry, integrity endpoint). LAN threat model exists at `docs/security/THREAT_MODEL.md`. GitHub Actions clean (Node 22, latest action versions). |

All planned productization phases (1–7) are complete. No productization Phase 8 is defined. The completed migration Phase 8 closeout is historical, not a new work track. The agent should maintain repo health (CI, docs, test drift, wire governance, security baseline) until the user provides new direction or defines a new phase.

## Disallowed by Default

The following are **not** allowed without explicit human approval and a new implementation plan:

- **MVP9** — no MVP9 scope has been defined.
- **CA-8** — CA-7 is sealed. No CA-8 exists.
- **Original task-book Stage 5** — no Stage 5 implementation has been started.
- **Provider/model productionization** — broadening real API calls beyond the existing explicit env-gated local beta path, enabling providers by default, or adding unattended provider execution.
- **Sandbox/process/container/VM execution** — real isolation or subprocess expansion beyond the existing local CLI executor path.
- **Runtime autonomous workers** — real concurrent worker processes.
- **Target repo writes** — any mutation of registered target repositories.
- **Approval/run/execute/deploy/merge controls** — any execution or deployment mechanism.
- **Cloud productionization** — hosted service, SaaS deployment, production multi-tenant service, or remote user-facing release.

A planning-only module may store app-owned non-executable plans or approval metadata. That does not approve approval controls, execution controls, runtime workers, target writes, or sandbox execution.

Batch 6 design contracts, Batch 7 readiness audit, Batch 7 implementation-plan artifact, Batch 7 Slice A storage metadata, Batch 7 Slice B read-only HTTP metadata views, and Batch 7 Slice C read-only SDK wrappers do not approve any sandbox/process/container/VM implementation, target workspace writer, approval broker wiring, rollback engine, artifact-capture runtime, worker process, provider call, push, merge, deploy, apply, run, or execution control.

The local small-team track does not approve cloud hosting, default-on provider calls, sandbox isolation, subprocess expansion beyond the existing CLI executor path, target-repo writes, hosted deployment, or real autonomous workers.

Python legacy reference implementation has been retired. Python is retained only as the REST SDK (`sdk/python/`) and utility scripts. New primary runtime work belongs in Rust and TypeScript.

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` to confirm the latest state.
2. Confirm the proposed track is not in the disallowed list above.
3. Confirm the work has an architecture-book, test, issue, review finding, or documentation-drift basis.
4. Keep the change commit-sized and run the relevant verification.
5. Run `uv run --no-project python scripts/check_agent_handoff.py` (includes toolchain and `scripts/check_wire_codegen_drift.sh` guards).
6. Update handoff docs before committing and pushing.
