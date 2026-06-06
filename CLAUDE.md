# Project Instructions

## Product Scope

**What**: A local deterministic harness and self-hosted agent-control-plane for studying token-efficient agent workflows. It provides deterministic dispatch planning, local API/dashboard access, app-owned SQLite history/config/team state, and cost-of-pass metrics.

**What NOT**: Not a cloud production SaaS or autonomous-agent runtime. No real model-provider calls by default, no sandbox/process/container/VM isolation runtime, no autonomous workers, no target-repo writes, and no hosted production deployment. Existing local CLI executor subprocess invocation is a separate, explicit opt-in exception via `ACP_ENABLE_CLI_EXECUTION=1`.

**Target user**: Solo developer or small local team studying and operating deterministic agent infrastructure patterns on one machine or a LAN.

## Architecture Summary

The dispatch kernel uses a **deterministic, rule-based pipeline**:

Request → TaskAnalyzer → ModelSelector → BudgetManager → DispatchDecision → Executor → Evaluation → Ledger

Key design principles:
- Rule-based only (no LLM calls in dispatch kernel)
- Dataclass schemas, no pydantic
- Phase boundaries enforce safety (Phase 1-2: no real providers, Phase 3+: provider calls allowed with gates)
- Event-sourced ledger for auditability

Master architecture document: `docs/dispatch/DISPATCHER_KERNEL_V0_ARCHITECTURE.md`. ALL implementation work must follow this document. It is the single source of truth for:

- Phase definitions, goals, success criteria, and promotion gates
- Schema definitions and field-level contracts
- Component responsibilities and interfaces
- Testing strategy and pass/fail thresholds
- Cross-phase architecture decisions and rationale

## Current State (as of 2026-06-05)

- **Original Stage 0-4 task-book**: Complete and sealed.
- **Harness App MVP0-MVP8**: Complete local operations console.
- **Trials 0-5**: Closed, with target repo onboarding, multi-repo generalization, real-use pilot, and CLI execution beta complete.
- **Python legacy reference**: **RETIRED** — `src/harness_core/`, root `tests/`, `demos/`, and root `pyproject.toml` removed (commit c3a23f5). Python retained only as REST SDK (`sdk/python/`) and utility scripts (`scripts/`, `tools/`, `codegen/`). Rust `engine/` is the sole runtime implementation.
- **Dispatch Kernel Phase 1-6A**: All phases STABLE (Phase 6A: 5 source modules, 5 test files, 1596 tests).
- **Phase 5 — Multi-Agent Orchestration**: STABLE (11 orchestration modules, 1454 tests, GPT approved after 3 review rounds).
- **Phase 6A — Local Durable API/Storage**: STABLE (5 source modules, 5 test files, 1596 tests, GPT approved after 2 review rounds).
- **Phase 6B-1 — Per-server Route Isolation**: Implemented (http_server.py refactored, 1603 tests).
- **Phase 6B-2 — Local API Key + Tenant Boundary**: STABLE (auth.py, auth middleware, 1654 tests, GPT approved).
- **Phase 6B-3 + Phase 7**: STABLE (rate_limiter, backup_manager, plugin_system, plugin_registry, sdk, doc_generator; GPT approved).
- **Phase 7 P7-T3 CommunityProfileRegistry + P7-T4 ToolAdapterManager**: IMPLEMENTED (community_profiles.py, tool_adapter.py; 58 new tests).
- **Phase 7 P7-T5 Dashboard + P7-T8 Benchmark**: IMPLEMENTED (dashboard.py, benchmark.py; 99 new tests).
- **Phase 6B-3 Gate 3 — Plugin Thread Safety**: STABLE (RLock in PluginSystem, locks in PluginRegistry; 2089 total).
- **Language migration Phase 0**: IMPLEMENTED (wire_contract/v1 JSON schemas, Python golden fixtures, stdlib parity runner; 2089 Python tests).
- **Language migration Phase 1**: IMPLEMENTED (Rust `engine` crate with deterministic runtime, event schema, task analyzer, dispatch decision parity).
- **Language migration Phase 2**: IMPLEMENTED (Rust selector, budget manager, noop executor, evaluation stub, ledger, dispatch engine).
- **Language migration Phase 3**: IMPLEMENTED (routing/ 7 modules + orchestration/ 10 modules; 385 Rust tests total).
- **Language migration Phase 4**: IMPLEMENTED (infrastructure/: observability, auth, rate_limiter, plugin_system, plugin_registry).
- **Language migration Phase 5**: IMPLEMENTED (ecosystem/: community_profiles, tool_adapter, dashboard, benchmark).
- **Language migration Phase 6**: IMPLEMENTED (storage/: durable_store via rusqlite, health_checker, backup_manager).
- **Language migration Phase 7**: IMPLEMENTED (sdk, storage_migrator).
- **Language migration Rust engine/API parity**: IMPLEMENTED (`http_server` local axum router for health/ready/openapi/dispatch, disabled-by-default `provider` trait boundary, `doc_generator`).
- **Agent-Control-Plane Phase 5 SDK + Codegen**: IMPLEMENTED (`codegen/generate_wire_types.py`, generated Rust/TypeScript/Python wire types, TypeScript REST SDK with Node test coverage, Python REST SDK with 8 tests). Full nested types from all 6 wire_contract schemas for SDK surfaces; Rust includes generated boundary value types.
- **Agent-Control-Plane Phase 6 Dashboard**: IMPLEMENTED (`dashboard/` Next.js App Router, read-only views, static export support, no executable controls).
- **Agent-Control-Plane Phase 7 Docker**: IMPLEMENTED (`deploy/Dockerfile.engine`, `deploy/Dockerfile.dashboard`, `docker-compose.yml`, optional local API + dashboard smoke).
- **Agent-Control-Plane Phase 8 Closeout**: IMPLEMENTED (`docs/AGENT_CONTROL_PLANE_MIGRATION_CLOSEOUT.md`).
- **Agent-Control-Plane Native Local Runtime**: IMPLEMENTED (`ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine` serves API + dashboard from one Rust process; Docker optional).
- **Agent-Control-Plane Local Small-Team Productization**: IMPLEMENTED (`engine/src/storage/local_product_store/`, live dashboard API state, SQLite dispatch history/config/team/API-key metadata/audit/cost/plan/workflow-run/supervised-patch-metadata state, export, admin-auth-confirmed local backup, operations metrics, backup verify/restore dry-run, audit redaction, provider pricing visibility, local ops/restore smoke scripts, SDK local-state methods). Still no cloud SaaS, target writes, real workers, or sandbox/process isolation runtime.
- **Phase 6B-3 Gate 1**: IMPLEMENTED (scope checks, rate limiting, 403/429 responses).
- **Security hardening**: redaction logging, http_server body size limit + CORS, checkpoint path traversal fix, 42 new tests for coverage gaps.
- **Productization Phase 2 — Permission Governance**: IMPLEMENTED (API key create/revoke/rotate/delete/scopes, team member create/update-role/delete, last_used_at tracking, expires_at support, revoked_at enforcement, admin audit events, team:admin scope gating, SDK CRUD methods, dashboard management UI).
- **Productization Phase 3 — Cost Governance**: IMPLEMENTED (cost_summary v2: reserved vs estimated, token usage, utilization ratio, daily trend; dispatch_cost_details endpoint; dashboard enhanced Costs view; typed SDK cost responses; 15 new Rust tests, 1056 total).
- **Productization Phase 4 — Data Operations**: IMPLEMENTED (versioned SQLite migrations via PRAGMA user_version; check_integrity() with PRAGMA integrity_check and per-table row counts; import_snapshot() for idempotent import from export JSON; GET /api/v1/storage/integrity and POST /api/v1/import and POST /api/v1/backups/:id/restore endpoints; backup restore hardened with restore_backup_with_verify(); data-directory documentation; 19 new Rust tests, 1075 total).
- **Productization Phase 5 — Native Packaging**: IMPLEMENTED (.env.example with all 16 env vars; install.sh/upgrade.sh scripts; package-release.sh builds release binary + static dashboard tarball; smoke_release.sh verifies extracted artifact; 4 MB release tarball).
- **Rust + TypeScript Cutover**: COMPLETE (`engine/` is the primary runtime/API/storage/provider-gated control plane; `dashboard/` and `sdk/typescript/` are the primary TypeScript surfaces; `scripts/verify_rust_typescript_stack.sh` is the primary cutover verification. Python retained as REST SDK and utility scripts only).
- **Architecture Refactor R1**: IMPLEMENTED (`engine/src/http_server/` module directory: mod.rs, state.rs, middleware.rs, routes.rs, server_context.rs, handlers/{health,dispatch,team,keys,costs,backups,audit,provider,dashboard,data_ops}). 1140 Rust tests pass.
- **Architecture Refactor R2**: IMPLEMENTED (`engine/src/storage/local_product_store/` module directory: mod.rs, dispatch.rs, config.rs, team.rs, keys.rs, audit.rs, provider_audit.rs, costs.rs, migrations.rs, integrity.rs, export_import.rs, boundaries.rs). 1140 Rust tests pass.
- **Architecture Refactor R3**: IMPLEMENTED (`engine/src/task_analyzer/` module directory: mod.rs, rules.rs, classify.rs, risk.rs, scoring.rs). 1144 Rust tests pass.
- **Architecture Refactor R4**: IMPLEMENTED (`engine/src/workflow/dag_manager/` module directory: mod.rs, types.rs, helpers.rs, mutations.rs, compensate.rs). 1144 Rust tests pass.
- **Architecture Refactor R5**: IMPLEMENTED (`engine/src/workflow/context_pack/` module directory: mod.rs, rules.rs, types.rs, validation.rs, budget.rs). 1144 Rust tests pass.
- **Architecture Refactor R6**: IMPLEMENTED (`engine/src/harness/model_profiles/` module directory: mod.rs, constants.rs, types.rs, validation.rs, shadow.rs). 1144 Rust tests pass.
- **Architecture Refactor R7**: IMPLEMENTED (`engine/src/workflow/concurrency/` module directory: mod.rs, dag_types.rs, types.rs, controller.rs, helpers.rs). 1144 Rust tests pass.
- **Architecture Refactor R-series**: **SEALED AT R7**. R8 is not approved. `checkpoint.rs` split and `dispatch_decision.rs` split deferred. No further R-series file splitting is approved.
- **Post-R7 Wire/Type Governance Hardening**: IMPLEMENTED (`app_layer` dormant-reference annotation, 20-fixture Rust typed round-trip guardrail, active CLI/provider execution-result schema enums, generated/manual TypeScript split behind compatibility re-export, schema-driven practical enum extraction, `--check` codegen mode, CI/autonomous-closeout `scripts/check_wire_codegen_drift.sh` guard, localized dashboard union reuse). Post-R7 real-use, developer-experience audit fixes, Trial 4 duplicate-dispatch regression coverage, Trial 5 malformed-CLI-output regressions, read-only planner persistence, inert workflow-run persistence, recommendation-only plan advisory metadata, and Batch 7 Slice A/B/C/D supervised patch metadata/design are stable in the current 1204-test Rust suite.
- **Dashboard UX Polish + Production-like Local Ops Hardening**: IMPLEMENTED (ARIA tab roles + keyboard navigation, modal focus traps with Escape, keyboard-accessible dispatch rows, form labels, aria-label on icon buttons/search, CSS spinner loading states, utility classes, structured dispatch detail view replacing JSON dumps, Operations tab, backup verify/restore dry-run, audit redaction, provider pricing visibility, read-only advisory risk-gate repair, scope templates, local ops/restore smoke scripts). 1204 Rust tests pass, TypeScript strict + readonly lint + build + static export pass.
- **Supervised Autonomous Beta Planning Batch 3**: IMPLEMENTED planning-only. `/api/v1/plans` creates/lists/reads non-executable app-owned plans; `engine/src/read_only_planner.rs` generates canonical `WorkflowGraph` plans using rule-based analysis and deterministic decomposition; `workflow_plans` persists plan state in local SQLite; TypeScript/Python SDKs expose plan methods. No workers, target writes, sandbox/process/container/VM execution, provider calls, approve/run/deploy/merge controls, or default-on execution were added.
- **Supervised Autonomous Beta Planning Batch 4**: IMPLEMENTED inert state only. `/api/v1/workflow-runs` creates/lists/reads workflow run metadata from plans, persists run/node/edge/event/approval rows, and records resume/cancel intent as metadata only; TypeScript/Python SDKs expose workflow-run methods. No `WorkflowEngine` execution path, workers, target writes, sandbox/process/container/VM execution, provider calls, approve/run/deploy/merge controls, or default-on execution were added.
- **Supervised Autonomous Beta Planning Batch 5**: IMPLEMENTED recommendation-only. Read-only plans include advisory metadata for quality/routing/retry/observability decisions only. No provider invocation, retry execution, live workers, target writes, sandbox/process/container/VM execution, or approve/run/deploy/merge controls were added.
- **Supervised Autonomous Beta Planning Batch 6**: DESIGN GATE DOCUMENTED. ADR-0002 and `docs/security/THREAT_MODEL.md` define future sandbox/workspace/approval-broker/rollback/artifact-capture contracts and execution-phase risks as planning artifacts only. No implementation authority, runtime worker, target write, sandbox/process/container/VM execution, provider call, or approve/run/deploy/merge control was added.
- **Supervised Autonomous Beta Planning Batch 7 Slice A**: IMPLEMENTED storage-only metadata. `LocalProductStore` schema v3 stores app-owned `supervised_patch_workspaces` and `supervised_patch_artifacts`, with path-boundary validation outside registered target repositories, normalized changed-file validation, import-bypass validation, export/import/integrity/stats coverage, and 7 new Rust tests.
- **Supervised Autonomous Beta Planning Batch 7 Slice B**: IMPLEMENTED read-only HTTP visibility. `/api/v1/supervised-patch/workspaces`, `/api/v1/supervised-patch/workspaces/{workspace_id}`, `/api/v1/supervised-patch/artifacts`, and `/api/v1/supervised-patch/artifacts/{artifact_id}` expose app-owned metadata through GET-only routes requiring `dispatch:read`, with OpenAPI docs and 4 Rust tests.
- **Supervised Autonomous Beta Planning Batch 7 Slice C**: IMPLEMENTED read-only SDK visibility. TypeScript/Python SDKs expose list/detail GET wrappers for supervised patch workspace/artifact metadata, with TS response types and URL-encoding tests. No POST/PUT/DELETE methods, dashboard UI, workspace creation, patch generation, approval/export gate, rollback engine, runtime controls, execution code, target writes, workers, sandbox/process/container/VM execution, providers, or push/merge/deploy/apply controls are approved.
- **Supervised Autonomous Beta Planning Batch 7 Slice D**: DOCUMENTED approval-binding contract only. ADR-0002 defines `supervised_patch_approval_binding.v1`, evidence-binding fields, validation rules, state transitions, export-eligibility checks, and blocking conditions for future patch-review/export approval. No code, tables, routes, SDK methods, dashboard UI, approval broker wiring, export runtime, execution code, target writes, workers, sandbox/process/container/VM execution, providers, or push/merge/deploy/apply controls are approved.
- **Production-Grade Track**: User-approved (2026-06-06). Extending supervised autonomous beta infrastructure to production-grade hosted/self-hosted deployment with real CLI workers, persistent scheduler, dashboard controls, SDK productization, security isolation, and ops hardening. Starting from commit 0309d0d (1222 Rust tests). Must not create parallel runtime/DAG/scheduler kernels. See `docs/NEXT_DECISION.md` for phase details.

See `docs/CURRENT_STATUS.md` for detailed phase closeout records.

## Known Technical Debt

**Phase 1 carry-over:**
- Compound "or" negations only match first phrase
- Evidence spans use placeholder (0, 0) positions
- Budget pressure is diagnostic only, doesn't change model selection
- fallback_tier conflates fallback/escalation semantics

**Phase 2 carry-over:**
- Pasteback stores raw_output inline (no redaction)
- ManualSessionStore lacks strict transition validation
- Boundary compliance is heuristic, not authoritative
- Token estimates are rough char/4

**Phase 3 carry-over:**
- Only env credential backend active (file/keyring/vault are schema-reserved)
- Audit recorder is in-memory (no persistent store)
- OpenAI-compatible path only; Anthropic/local are future adapters
- Cost depends on configured pricing and provider-reported usage

**Phase 4 carry-over:**
- routing_experiment_id is supported but usually None until richer experiment tracking exists
- History and observation stores are in-memory only
- Promotion logic is deterministic threshold-based, not statistical
- Adaptive routing depends on quality/cost observations supplied by upstream evaluators

See `docs/CURRENT_STATUS.md` for full details.

## Session Log

- **2026-05-27→05-28**: Dispatch Kernel Phases 1-5, 6A, 6B-1/2/3, Gates 1-3, Phase 7 all STABLE through iterative GPT review. 2089 Python tests. Language migration Phases 0-7 complete. Rust engine/API parity implemented. SDK + codegen + dashboard + Docker deployed. See `docs/CURRENT_STATUS.md` for full track history.
- **2026-05-29**: Provider infrastructure (audit/redaction/RetryFallbackManager), Rust + TypeScript cutover, Productization Phases 1-6 (Provider Safety Gate, Permission Governance, Cost Governance, Data Operations, Native Packaging, Dashboard Controls). 1086 Rust tests, 13 TS SDK tests, 17 Python SDK tests. All verification passing.
- **2026-05-30**: Long-Run Hardening (part 1) — SQLite contention tests (6 tests for concurrent writes, reads-during-writes, audit events, deadlock prevention, data integrity) and provider failure matrix tests (21 tests covering retry exhaustion, fallback routing, budget-exhausted mid-retry, non-retryable errors, disabled provider, cost gate blocks, audit trail, governance blocks, backoff strategies). 27 new Rust tests (1113 total).
- **2026-05-30**: Long-Run Hardening (part 2) — Audit integrity tests (7 tests: mutation audit correctness, ordering monotonicity, persistence across reopen, concurrent writes, integrity report row count). Enhanced smoke_release.sh: tarball structure, install smoke, data preservation, port retry, integrity endpoint. 7 new Rust tests (1120 total).
- **2026-05-30**: CLI Executor Routing — Complexity-based dispatch to Claude Code CLI / Codex CLI. New `engine/src/cli/` module with `ClaudeCodeCliExecutor`, `CodexCliExecutor`, `MultiExecutor`, `CliConfig`. Existing local subprocess exception is explicit opt-in via `ACP_ENABLE_CLI_EXECUTION=1`. Complexity threshold 0.7 escalates to CLI tiers when enabled. 10 new Rust tests (1130 total).
- **2026-05-30**: P1 Local-Beta Follow-Up — 7 items: GET /api/v1/keys metadata-only key list endpoint, search/filter/pagination for dispatches and audit, bookmarkable dashboard tabs via URL hash, 60-second auto-refresh with visibility-aware pausing, Docker volume persistence for SQLite, key reveal modal replacing alert(), dashboard split from 1358-line monolith into 12 focused components. 4 new Rust tests (1140 total).
- **2026-05-30**: P2 Local-Beta Polish & Type Hardening — CSS design token cleanup (#c0392b → var(--risk), utility classes), TypeScript SDK type hardening (22 new focused response interfaces, 21 methods typed, ExecutorType/ExecutionStatus extended for CLI executors), dashboard component quality (usePaginatedSearch hook, SearchBar, Pagination components), Next.js app polish (loading.tsx, error.tsx, metadata, favicon). 0 new Rust tests (1140 total), 16 SDK tests pass.
- **2026-05-30**: Toolchain Consolidation & Drift Guard — Standardized all authoritative docs to `uv run --no-project python` (8 stale bare python3 references fixed across 9 files). README toolchain table added. verify_rust_typescript_stack.sh preflight extended (bun/cargo/uv). `scripts/check_toolchain_drift.sh` drift guard added for stale JS/Python toolchain references. Integrated into autonomous closeout workflow. CI already aligned. 0 new Rust tests (1140 total). `uv.lock` intentionally not added.
- **2026-05-30**: Python Legacy Reference Retirement — Removed `src/harness_core/` (58 files), root `tests/` (121 files), `demos/` (2 files), legacy tools (2 files), root `pyproject.toml`. Relocated `test_security_baseline.py` and `test_dashboard_static.py` to `tools/`. Updated CI workflow, handoff script, and all living docs. Python now means SDK + utility scripts only. 0 new Rust tests (1140 total).
- **2026-05-30**: Architecture Refactor R1 — http_server split. Replaced 2077-line `engine/src/http_server.rs` monolith with `engine/src/http_server/` module directory (16 files): mod.rs, state.rs, middleware.rs, routes.rs, server_context.rs, handlers/{health,dispatch,team,keys,costs,backups,audit,provider,dashboard,data_ops}. Public API unchanged. 1140 Rust tests pass. Commit `f2c5ac3`.
- **2026-05-30**: Architecture Refactor R2 — local_product_store split. Replaced 1365-line `engine/src/storage/local_product_store.rs` monolith with `engine/src/storage/local_product_store/` module directory (12 files): mod.rs, dispatch.rs, config.rs, team.rs, keys.rs, audit.rs, provider_audit.rs, costs.rs, migrations.rs, integrity.rs, export_import.rs, boundaries.rs. Public API unchanged. 1140 Rust tests pass. Commit `3c9439b`. GPT PASS.
- **2026-05-30**: Architecture Refactor R3 — task_analyzer split. Replaced 1117-line `engine/src/task_analyzer.rs` monolith with `engine/src/task_analyzer/` module directory (5 files): mod.rs, rules.rs, classify.rs, risk.rs, scoring.rs. Public API unchanged. 1140 Rust tests pass. Commit `8813a4d`. GPT PASS.
- **2026-05-30**: Architecture Refactor R4 — dag_manager split. Replaced 1186-line `engine/src/workflow/dag_manager.rs` monolith with `engine/src/workflow/dag_manager/` module directory (5 files): mod.rs, types.rs, helpers.rs, mutations.rs, compensate.rs. Public API unchanged. 1144 Rust tests pass. Commit `7b9aac1`. GPT PASS (requires_approval re-export fix applied, docs patched). CLOSED.
- **2026-05-30**: Architecture Refactor R5 — context_pack split. Replaced 1003-line `engine/src/workflow/context_pack.rs` monolith with `engine/src/workflow/context_pack/` module directory (5 files): mod.rs, rules.rs, types.rs, validation.rs, budget.rs. Public API unchanged. 1144 Rust tests pass.
- **2026-05-30**: Architecture Refactor R6 — model_profiles split. Replaced 840-line `engine/src/harness/model_profiles.rs` monolith with `engine/src/harness/model_profiles/` module directory (5 files): mod.rs, constants.rs, types.rs, validation.rs, shadow.rs. Public API unchanged. 1144 Rust tests pass.
- **2026-05-30**: Architecture Refactor R7 — concurrency split. Replaced 674-line `engine/src/workflow/concurrency.rs` monolith with `engine/src/workflow/concurrency/` module directory (5 files): mod.rs, dag_types.rs, types.rs, controller.rs, helpers.rs. Public API unchanged. 1144 Rust tests pass.
- **2026-05-30**: Post-R7 closeout + wire/type governance hardening — sealed R-series at R7; deferred R8, checkpoint.rs split, and dispatch_decision.rs split; annotated dormant app_layer; added typed fixture round-trips and active enum checks; aligned execution-result schemas; split generated/manual TypeScript types; extracted practical schema enums in codegen; added `--check` drift enforcement in CI and handoff checks; reused generated enum aliases in dashboard types. 2 new Rust tests (1146 total).
- **2026-06-04**: Production-like Local Ops Hardening — guarded `.env.production-like.local.example` and startup script, `/api/v1/metrics`, dashboard Operations tab, backup verify and restore dry-run API/UI/script smoke, local env secret scan, audit redaction query, and least-privilege scope templates. 3 new Rust tests (1166 total). TypeScript strict + readonly lint pass; temporary protected engine ops/restore smoke passes. Branch `feat/dashboard-ux-polish`.
- **2026-06-04**: Production-like Provider Trial Repair — explicit provider price env (`ACP_PROVIDER_INPUT_COST_PER_1K_USD`, `ACP_PROVIDER_OUTPUT_COST_PER_1K_USD`) now controls estimated-cost availability across API/dashboard/SDK; missing rates surface as `pricing_configured=false` instead of a real zero-cost estimate. Read-only advisory requests about production/secret boundaries can execute through the env-gated provider path while preserving human review; deploy, secret disclosure, target write, and bypass requests remain blocked. Temporary CN OpenAI-compatible real-provider smoke passed: advisory provider execution completed with nonzero estimated cost, dangerous request stayed `execution_not_authorized`. 12 new Rust tests (1178 total).
- **2026-06-05**: Supervised Autonomous Beta Planning Batch 3 — read-only planner API implemented. Added deterministic `ReadOnlyPlanner`, `/api/v1/plans` POST/GET/detail endpoints, app-owned SQLite `workflow_plans` persistence, export/import/integrity coverage, and TypeScript/Python SDK plan methods. Plans use canonical `WorkflowGraph` and never execute, call providers, spawn workers, write targets, run sandbox/process/container/VM isolation, or expose approve/run/deploy/merge controls. 6 new Rust tests (1184 total), TS SDK 24 tests, Python SDK 28 tests.
- **2026-06-05**: Supervised Autonomous Beta Planning Batch 4 — inert durable workflow state implemented. Added `workflow_runs`, `workflow_run_nodes`, `workflow_run_edges`, `workflow_run_events`, and `workflow_run_approvals` SQLite state, `/api/v1/workflow-runs` metadata endpoints, export/import/integrity coverage, and TypeScript/Python SDK workflow-run methods. Resume/cancel endpoints record metadata and status only; no workers, execution, providers, target writes, sandbox/process/container/VM execution, or approve/run/deploy/merge controls. 6 new Rust tests (1190 total), TS SDK 25 tests, Python SDK 29 tests.
- **2026-06-05**: Supervised Autonomous Beta Planning Batch 5 — recommendation-only advisory metadata implemented. Read-only plans now include `advisory` with quality preflight status, cold-start routing recommendation, retry-policy metadata, observability hints, blockers, and recommendations. No providers, retry execution, live workers, target writes, sandbox/process/container/VM execution, or approve/run/deploy/merge controls. 3 new Rust tests (1193 total), TS SDK 25 tests, Python SDK 29 tests.
- **2026-06-05**: Supervised Autonomous Beta Planning Batch 6 — design gate documented. ADR-0002 now records future sandbox/process/container/VM, target workspace, approval broker, rollback, and artifact-capture contracts plus Batch 7 prerequisites; `docs/security/THREAT_MODEL.md` records T-009 through T-012 execution-phase design risks. Docs-only; no new Rust tests (1193 total).
- **2026-06-05**: Supervised Autonomous Beta Planning Batch 7 readiness audit — NO-GO for implementation. Missing implementation prerequisites recorded in ADR-0002 and `docs/NEXT_DECISION.md`; provider default-off and no-push/no-merge/no-deploy/no-target-mutation boundaries remain intact. Docs-only; no new Rust tests (1193 total).
- **2026-06-05**: Supervised Autonomous Beta Planning Batch 7 implementation-plan artifact — docs-only plan recorded in ADR-0002. The first planned primitive is app-owned detached patch workspace/snapshot outside registered target repositories; registered-target `git worktree add` is rejected because it mutates target `.git/worktrees` metadata. Future implementation must test workspace evidence, patch-review approval binding, rollback/quarantine, artifact capture/redaction/access, and unchanged target metadata before any supervised beta code is accepted. Docs-only; no new Rust tests (1193 total).
- **2026-06-05**: Supervised Autonomous Beta Planning Batch 7 Slice A — storage-only metadata implemented. Added `supervised_patch_workspaces` and `supervised_patch_artifacts` to app-owned SQLite schema v3, `LocalProductStore` metadata methods, stats/integrity/export/import coverage, path-boundary validation outside registered target repositories, normalized changed-file validation, import-bypass validation, and 7 Rust tests. 1200 Rust tests pass.
- **2026-06-05**: Supervised Autonomous Beta Planning Batch 7 Slice B — read-only HTTP visibility implemented. Added GET-only supervised patch workspace/artifact metadata routes with `dispatch:read` auth and OpenAPI docs. No route mutates state or grants execution/export/apply authority. 4 new Rust tests (1204 total).
- **2026-06-05**: Supervised Autonomous Beta Planning Batch 7 Slice C — read-only SDK visibility implemented. Added TypeScript/Python SDK list/detail methods for supervised patch workspace/artifact metadata GET routes plus TS response types and URL-encoding tests. Rust tests remain 1204; TS SDK 26 tests pass; Python SDK 30 tests pass.
- **2026-06-05**: Supervised Autonomous Beta Planning Batch 7 Slice D — docs-only approval-binding contract recorded. ADR-0002 now defines `supervised_patch_approval_binding.v1`; threat model T-011 records future binding controls. No code or tests added; Rust count remains 1204.
- **2026-06-06**: CI fix + pilot hardening + real CLI pilot — Fixed CI failures (security baseline allowlist for pilot scripts, `cargo fmt`). Fixed pilot script `--base-url` propagation (ApiClient class replaces bare function) and added hard-fail assertions (executor must create files, patch must contain changes). Fixed CLI executor wiring: tick API now passes `command` to `CliNodeExecutor` via `tick_with_executor_and_command` (was falling back to `echo noop`). Added `--allowedTools Edit,Write,Bash` to claude CLI invocation. New `scripts/pilot_cli_e2e.py` verified end-to-end with real Claude Code CLI process: plan → run → workspace → CLI tick (creates src/greeting.rs + modifies src/lib.rs) → capture (patch: +src/greeting.rs, ~src/lib.rs) → approval → export → cleanup. Hard assertions verify file content and patch coverage. 1256 Rust tests pass. Commits `06fcfb4`, `10f4bdc`, `ae151c4`.
- **2026-06-06**: GA-3 Scheduler Stability — 10 new tests proving lease anti-concurrency, fail-executor error counting, cancelled-run skip, stale-lease recovery with re-execution, retry exhaustion, max_concurrent limiting, and active_runs status field. 3 HTTP endpoint tests for `/api/v1/scheduler/status` (enabled/disabled/active_runs). Scheduler status API now includes `active_runs` count from live store. 1276 Rust tests pass, clippy clean, fmt clean.
- **2026-06-06**: GA-4 Observability/Audit — 3 new tests (metrics enrichment fields, node_tick audit event, approval audit event). `/api/v1/metrics` now includes `artifact_count`, `approval_count`, `executor_latency_avg_ms`, `scheduler_active_runs`. `workflow_run.node_tick` audit event emitted for every node execution (completed, failed, retry_scheduled) with executor_type, latency, status, attempt, error_domain. Existing audit events for capture, export, cleanup, quarantine, approval_record confirmed. 1286 Rust tests pass, clippy clean, fmt clean.
- **2026-06-06**: GA-5 Review UI — Dashboard WorkflowRuns component (run list with search, run detail with node table/event timeline/approval list/tick+cancel controls, executor selector, summary tiles) and SchedulerStatus component (running state, config, tick/error/retry counts, active runs). 8 new API client functions. TypeScript types for WorkflowRun, WorkflowRunNode, WorkflowRunEdge, WorkflowRunEvent, WorkflowRunApproval, WorkflowPlan, SchedulerStatus. Runs and Scheduler tabs added to navigation. ConfirmDialog extended with tickRun/cancelRun. 0 new Rust tests (1286 total); TypeScript strict + readonly lint + static build all pass. Verified: `cargo test -p engine`, `npx tsc --noEmit`, `node scripts/lint-readonly.mjs`, `node scripts/build-static.mjs`, `cargo fmt --check`, `cargo clippy`, `uv run --no-project python scripts/check_agent_handoff.py`.
- Previous BLOCK findings (b6d5bc1): HIGH-1 rate limit not wired, HIGH-2 scope enforcement missing, HIGH-3 plugin locks unused
- Gate 1 addresses: HIGH-1 (rate limiter in ServerContext + _check_rate_limit), HIGH-2 (scope enforcement + AuthorizationDecision + 403/429)
- Gate 2 addresses: atomic restore, WAL safety, failure-mode coverage
- HIGH-3 (plugin locks) deferred to Gate 3
- Status: Gate 1 STABLE, Gate 2 STABLE, Gate 3 STABLE

### GPT Gate Feedback Summary (2026-05-28)
All dispatch kernel review gates (6B-1, 6B-2, 6B-3 Gates 1-3, Phase 7 hardening) passed after iterative fixes. Key P0 fixes: empty-scope bypass, stale scope re-registration, WAL checkpoint before sidecar removal, atomic restore with checksum verification. Full details archived in git history.

## External Dependencies

- **Python stdlib only** — zero runtime dependencies, no third-party packages. Provider adapters use injectable transport (urllib used only in demo scripts).
- **No runtime LLM dependencies** in dispatch kernel itself (provider is pluggable)

## Test Strategy

- **Framework**: Rust `cargo test` for engine; Python `unittest` for SDK
- **Run command**: `cargo test -p engine` (primary); `cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests` (SDK)
- **Current count**: 1286 Rust tests pass, 0 failures (as of 2026-06-06)
- **Coverage**: Phase boundary contracts, schema validation, golden fixtures
- **CI**: GitHub Actions on push/PR to main — runs security baseline + Rust/TS/SDK tests
- **Test-first**: Write tests alongside implementation. Follow the test strategy in Section 4.25 of the architecture book.

## New Session Bootstrap

Start every Claude Code, Codex, or other coding-agent session by reading:

1. `docs/SESSION_START_HERE.md`
2. `docs/CURRENT_STATUS.md`
3. `docs/NEXT_DECISION.md`
4. `docs/MODULE_MAP.md`

Do not infer a new Stage 5, CA-8, production track, or provider-integration track from old phase names. The original Stage 0-4 work is complete; Dispatch Kernel Phases 1-7 (including 6A, 6B-1/2/3, Gates 1-3) are all complete and stable.

## Next Action

Use `docs/NEXT_DECISION.md` for the current forward plan. Do not rely on older phase logs as next-action authority.

## Autonomous Advancement Protocol

For each autonomous session:

1. Inspect `git status --short --branch` and read the session bootstrap docs.
2. Choose the highest-value safe task from failing verification, CI/docs/test drift, wire-governance drift, concrete review findings, or narrowly scoped hardening.
3. Update or add tests before behavior changes.
4. Run the relevant verification command, plus `uv run --no-project python scripts/check_agent_handoff.py` (includes toolchain and `scripts/check_wire_codegen_drift.sh` guards).
5. Update the smallest necessary handoff surface before commit: `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`, `README.md`, `CLAUDE.md`, and `AGENTS.md` when their facts changed.
6. Commit in English and push when the working tree only contains this session's intended changes.
7. Leave the next action, latest commit, verification, and residual risks in the final report.

This protocol authorizes the coding agent to advance the repository. It does not authorize adding runtime autonomous workers to the harness.

## Rules

1. **Always reference the architecture book** before making implementation decisions. If the book is ambiguous or too coarse, discuss with GPT (see below).
2. **Never deviate from schemas** defined in the architecture book without updating the book first.
3. **Phase boundaries are sacred**: Phase 1-2 MUST NOT call real providers, add sandbox isolation, expand subprocess execution beyond the existing CLI executor path, write to target repos, or start autonomous workers. Current provider execution is an explicit env-gated local beta path and must stay default-off unless a future approved plan changes that boundary.
4. **When blocked or facing coarse granularity**: Discuss with GPT in the same ChatGPT session used for architecture review. Iterate until both agree, then update the architecture book before implementing.
5. **Document maintenance**: Keep the authoritative handoff surface current and small. Prefer shortening existing docs or deleting stale planning docs over adding files.
6. **Autonomous closeout**: Run `uv run --no-project python scripts/check_agent_handoff.py` before commit. A commit is incomplete if the handoff docs no longer tell the next session what changed, how it was verified, and what should happen next.
7. **Single forward plan**: `docs/NEXT_DECISION.md` is the only roadmap / next-steps / productization-plan surface. Do not create parallel planning documents. If the phase order, goals, or done-when criteria change, update `docs/NEXT_DECISION.md` and only the directly affected handoff docs.

## Documentation Maintenance Rule

Before committing, update the smallest necessary handoff surface if the change affects status, scope, tests, commands, boundaries, modules, or next steps.

Authoritative surfaces:

- `docs/CURRENT_STATUS.md` — current state, verification, test counts, stable tracks, limitations
- `docs/NEXT_DECISION.md` — single forward plan, local productization phases, allowed/disallowed paths
- `docs/MODULE_MAP.md` — source/test ownership
- `README.md`, `CLAUDE.md`, `AGENTS.md` — quickstart, agent workflow, hard boundaries

Do not add new roadmap, next-steps, closeout, status, or productization documents unless the user explicitly asks for a new artifact. Prefer shortening or deleting stale documents. If no document update is needed, say why in the completion report.

## GPT Collaboration Protocol

ChatGPT session: https://chatgpt.com/c/69fc96b0-2e48-839f-a031-557e9e2317ca

When you encounter:
- Schema ambiguity or missing fields
- Component interface questions
- Phase boundary edge cases
- Cross-phase integration uncertainties

Discuss with GPT, then:
1. Get GPT's analysis
2. Independently audit GPT's suggestions (don't accept blindly)
3. Share both perspectives with the user if needed
4. Update the architecture book with agreed changes
5. Push to GitHub so GPT can reference the latest version

## Code Style

- Python 3.10+, dataclasses for schemas, no pydantic
- Rule-based logic (no LLM calls in dispatch kernel)
- Deterministic, testable, auditable
- No comments unless WHY is non-obvious
- Commit messages: English, concise, focus on why
