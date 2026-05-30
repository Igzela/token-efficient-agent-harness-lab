# Current Status

Last verified: 2026-05-30.

## Repository State

- Branch: `main` with the Rust + TypeScript agent-control-plane cutover complete. Rust `engine/` is the primary runtime/API/storage/provider-gated control plane; `dashboard/` and `sdk/typescript/` are the primary TypeScript surfaces. Python retained as REST SDK (`sdk/python/`) and utility scripts only.
- Tests: **1140 Rust pass**, 0 failures. Python SDK tests run separately under `sdk/python/`.
- Security baseline: ALL CHECKS PASSED.

## New Session / Documentation Discipline

New Codex, Claude Code, or other coding-agent sessions must start with `docs/SESSION_START_HERE.md`, then this file, then `docs/NEXT_DECISION.md`.

The responsible coding agent has standing authority to autonomously advance repository-safe work: documentation repair, focused regression fixes, CI/security/test hardening, and architecture-book-defined dispatch-kernel phase work that remains deterministic, local, test-first, and does not broaden real provider behavior beyond the existing explicit env-gated beta paths, real sandbox/process execution, target repo writes, deployment, or real runtime workers.

After every commit-sized change, update only the authoritative handoff docs whose facts changed. `docs/NEXT_DECISION.md` is the single forward-plan surface; do not add parallel roadmap, next-steps, status, closeout, or productization documents unless the user explicitly asks for a new artifact.

Run `uv run --no-project python scripts/check_agent_handoff.py` before committing so the handoff surface remains self-consistent.

## Completed Tracks

| Track | Status |
|---|---|
| Stage 0 — Foundation | Complete |
| Stage 1 — Deterministic Harness Core | Complete |
| Stage 2 — Quality Runtime | Complete |
| Stage 3 — Controlled Intelligence Stubs | Complete |
| Stage 4 — Advanced Runtime Abstractions | Complete |
| CA-7 Sealed Baseline | Complete — policy baseline sealed |
| Post-closeout hardening/design | Complete |
| Harness App MVP0–MVP8 | Complete |
| Trial 0 — Real target acceptance | Closed — `PASS` |
| Trial 1 — Multi-task budget validation | Closed — `ACCEPTABLE_FOR_MULTI_TASK_TRIAL_AFTER_HARDENING` |
| Reliability Hardening 1 — Negated risk and triage | Complete |
| Demo packaging | Complete |
| Demo verification | Complete — all docs accurate and runnable |
| Trial 2 candidate selection | Complete — hermes-gateway-lab onboarded |
| Trial 2 execution | Closed — `ACCEPTABLE_WITH_NOTES` (audit BLOCKED on target, generalization finding) |
| Target repo onboarding plan | Complete — plan and templates ready, awaiting user approval for target writes |
| Target repo onboarding (hermes-gateway-lab) | Complete — PR #1 merged (commit `77cf282`), onboarding files on target main, audit PASS_WITH_NOTES / blockers [] |
| Trial 2 onboarded replay | Closed — `ACCEPTABLE_FOR_ONBOARDED_SECOND_PROJECT_TRIAL` (audit PASS_WITH_NOTES, 0 blockers, 5 plans created) |
| Trial 2 final verification | Closed — `TRIAL_2_FINAL_VERIFICATION_PASS` (audit PASS_WITH_NOTES from target main, 5 plans, all boundary confirmed) |
| Trial 3 multi-repo generalization | Closed — `TRIAL_3_MULTI_REPO_GENERALIZATION_PASS` (3 repos: API/CLI/infra, all BLOCKED→PASS_WITH_NOTES, 6 plans, triage working) |
| Trial 3 target merge | Closed — all 3 target PRs merged, audit PASS_WITH_NOTES, blockers [] |
| Global Architecture Book v1 | Approved — 3-round Claude+GPT collaborative review, Phase 1 implementation-ready |
| Phase 1 — Dispatch Kernel | **STABLE** — 8 source files, 20 fixtures, 1074 total tests, commits `a4227e9`→`aed213b`→`592803f` |
| Phase 2 — Manual Execution Bridge | **STABLE** — 6 source modules, 6 test files, 1131 total tests, commits `afbba23`→`19c8a17`→`8f683ad` |
| Phase 3 — Provider Adapter Boundary | **STABLE** — 8 source modules, 8 test files, 1188 total tests, commits `c0ec508`→`e34ad8e`→`29fd12b`→`0092a1c`→`c631b4d`→`ef52704` |
| Phase 4 — Adaptive Routing | **STABLE** — 8 source modules, 8 test files, 1270 total tests, commits `ed2c762`→`66ebbc7` |
| Phase 5 — Multi-Agent Orchestration | **STABLE** — 11 source modules, 12 test files, 1454 total tests, GPT approved |
| Phase 6A — Local Durable API/Storage | **STABLE** — 5 source modules, 5 test files, 1596 total tests, GPT approved |
| Phase 6B-1 — Per-Server Route Isolation | **STABLE** — ServerContext pattern, backward-compatible, 4 isolation tests |
| Phase 6B-2 — Local API Key + Tenant Boundary | **STABLE** — 1 source module (auth.py), 1 test file (test_auth.py), auth middleware in http_server.py, RequestContext flow into RouteMatch, 1654 total tests, GPT approved |
| Phase 6B-3 — Rate Limiting + Backup + Plugins | **STABLE** — 5 source modules (rate_limiter, backup_manager, plugin_system, plugin_registry, cli extensions), 5 test files, 192 new tests (1846 total), GPT approved |
| Phase 6B-3 Gate 1 — Enforcement Hardening | **STABLE** — scope checks, rate limiting, 403/429 in HTTP path, 17 enforcement tests + 7 provider tests (1870 total), committed b404b8f→e26439f, GPT PASS (2 rounds) |
| Phase 6B-3 Gate 2 — BackupManager Atomic Restore | **STABLE** — atomic restore (prepare/checksum before live target, WAL checkpoint, try/finally), 4 failure-mode tests (37 backup_manager tests, 1919 total), committed ee0cd97→c124c57, GPT PASS (3 rounds) |
| Phase 7 — SDK + Documentation + Plugin System | **STABLE** — 2 source modules (sdk.py, doc_generator.py), 2 test files, 64 new tests, GPT approved |
| Phase 7 (P7-T3) — CommunityProfileRegistry | **IMPLEMENTED** — 1 source module (community_profiles.py), 1 test file, 31 tests |
| Phase 7 (P7-T4) — ToolAdapterManager | **IMPLEMENTED** — 1 source module (tool_adapter.py), 1 test file, 27 tests |
| Phase 7 (P7-T5) — Dispatch Dashboard | **IMPLEMENTED** — 1 source module (dashboard.py), 1 test file, 46 tests |
| Phase 7 (P7-T8) — BenchmarkSuite | **IMPLEMENTED** — 1 source module (benchmark.py), 1 test file, 53 tests |
| Phase 6B-3 Gate 3 — Plugin Thread Safety | **STABLE** — RLock in PluginSystem, locks in PluginRegistry, all public methods guarded, 0 new tests (verified existing), committed 785fe61 |
| Language Migration Phase 0 — Wire Schemas + Python Golden Parity | **IMPLEMENTED** — `wire_contract/v1` JSON schemas, 20 normalized Python golden fixtures, stdlib parity runner, 6 contract tests. |
| Language Migration Phase 1 — Rust Parity Kernel | **IMPLEMENTED** — Rust workspace + `engine` crate with deterministic fixture runtime, event schema validation/hash helpers, task analyzer, dispatch decision bundle builder, and 20-fixture Python golden parity test. |
| Language Migration Phase 2 — Rust Dispatch Engine | **IMPLEMENTED** — Rust selector, budget reservation manager, noop executor abstraction, evaluation stub, dispatch ledger, and dispatch engine. |
| Language Migration Phase 3 — Routing + Orchestration Parity | **IMPLEMENTED** — 17 routing/orchestration modules (routing: schemas, history_store, cost_of_pass_router, promotion_gate, auto_policies, feedback_integrator, dynamic_tier_selector; orchestration: schemas, agent_role_registry, task_decomposer, dependency_resolver, work_queue, workflow_engine, conflict_resolver, result_aggregator, human_approval_gate, multi_agent_budget). 173 new tests. Commit `31c105a`. |
| Language Migration Phase 4 — Infrastructure Parity | **IMPLEMENTED** — 5 infrastructure modules (observability, auth, rate_limiter, plugin_system, plugin_registry). 64 new tests. Commit `f877e81`. |
| Language Migration Phase 5 — Ecosystem Parity | **IMPLEMENTED** — 4 ecosystem modules (community_profiles, tool_adapter, dashboard, benchmark). 48 new tests. Commit `098eda9`. |
| Language Migration Phase 6 — Storage Parity | **IMPLEMENTED** — 3 storage modules (durable_store via rusqlite, health_checker, backup_manager). 32 new tests. Commit `965a2cd`. |
| Language Migration Phase 7 — SDK + Migrator | **IMPLEMENTED** — 2 modules (sdk, storage_migrator). 16 new tests. Commit `ea11dfb`. |
| Provider Infrastructure — audit + redaction | **IMPLEMENTED** — Rust `engine/src/provider/` expanded with `audit.rs` (ProviderAuditEvent, ProviderAuditRecorder with thread-safe in-memory store, monotonic event IDs, extra-field merge) and `redaction.rs` (redact_secrets, redact_audit_fields with recursive sensitive-key redaction). Module re-exports added to `mod.rs`. 28 new inline tests. |
| Language Migration Rust Engine/API Parity — http_server + doc_generator + component tests | **IMPLEMENTED** — `http_server` now includes a local axum router for `/api/v1/health`, `/api/v1/ready`, `/api/v1/openapi.json`, and deterministic `/api/v1/dispatch`; `doc_generator` includes module/schema registry and markdown generation; `provider` exposes a disabled-by-default provider trait boundary. 425 total Rust tests, 36 source modules, 32 test files. Real providers, target writes, sandbox/process/container/VM execution, runtime workers, executable dashboard controls, SDK publishing, and production deployment remain out of scope. |
| Agent-Control-Plane Phase 5 — SDK + Codegen | **IMPLEMENTED** — `codegen/generate_wire_types.py`, generated Rust/TypeScript/Python wire types, `sdk/typescript` REST SDK package, and `sdk/python` REST SDK package. `cd sdk/typescript && bun run build && npm pack --dry-run`, Python unittest, and `cd sdk/python && python -m build` pass. SDKs call REST endpoints and do not bind private Rust internals. Security baseline allows only the Python SDK's scoped stdlib `urllib` transport exception for this local REST client. |
| Agent-Control-Plane Phase 6 — Read-only Dashboard | **IMPLEMENTED** — Next.js App Router dashboard at `dashboard/` with dispatch, routing, agents/workflows, costs, settings, and health views. Verified with `cd dashboard && bun run lint && bun run typecheck && bun run build`; static export is verified with `cd dashboard && bun run build:static`. Dashboard does not import or call dispatch POST and exposes no approve/run/deploy/execute/merge controls. |
| Agent-Control-Plane Phase 7 — Local Docker Deploy | **IMPLEMENTED** — Optional local compose stack builds Rust axum API and Next.js dashboard with `deploy/Dockerfile.engine`, `deploy/Dockerfile.dashboard`, and root `docker-compose.yml`. `docker compose build` and default `docker compose up --build -d` pass; `/api/v1/health`, `/api/v1/dispatch`, and dashboard HTTP all returned 200-class responses. No production credentials, provider calls, target writes, sandbox/process execution, or runtime workers are enabled. |
| Agent-Control-Plane Phase 8 — Closeout | **IMPLEMENTED** — Closeout recorded in `docs/AGENT_CONTROL_PLANE_MIGRATION_CLOSEOUT.md`. Rust `engine/` includes a disabled-by-default provider trait boundary in `engine/src/provider/`; later provider stack work added explicit env-gated beta adapters. |
| Agent-Control-Plane Native Local Runtime | **IMPLEMENTED** — `engine` can serve API plus exported static dashboard from one local Rust process via `ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine`. `scripts/smoke_native_runtime.py` verifies health, readiness, dispatch, and dashboard root without Docker. |
| Agent-Control-Plane Local Small-Team Productization | **IMPLEMENTED** — Rust engine now defaults to app-owned SQLite state at `.agent-control-plane/local-team.db` (overridable by `ACP_DB_PATH`), persists dispatch history/config/team/API-key metadata/audit log/cost summary, exposes dashboard/history/config/team/cost/export/audit/admin-auth-confirmed-backup API endpoints, and serves a dashboard that reads real local API state instead of fixtures. TypeScript/Python SDKs include local state and backup methods. No cloud SaaS, target writes, real workers, or sandbox/process execution were added. |
| Rust Provider Stack — Stage 1 | **IMPLEMENTED** — 11 provider modules (config, credential, audit, redaction, transport, openai, anthropic, stub, executor, retry, mod), RetryFallbackManager with budget-checked retry and fallback routing, provider health endpoint (`GET /api/v1/provider/health`), env-based wiring via `ACP_PROVIDER_TYPE`/`ACP_API_KEY`/`ACP_MODEL`/`ACP_BASE_URL`. Provider execution is default-off and explicit env-gated; CI tests use stub/mock paths and do not call real provider APIs. |
| Rust Provider Stack — Stage 2 audit/usage bridge | **IMPLEMENTED** — provider audit events persist to local SQLite, dispatch history stores executor type, token usage, estimated provider cost, and latency columns, SDKs expose provider health/audit readers, and `/api/v1/provider/audit` reads persisted provider audit state. |
| Productization Phase 1 — Provider Safety Gate | **IMPLEMENTED** — `ACP_ENABLE_PROVIDER_EXECUTION=1` required for real provider types (stub remains safe without it), `ACP_REQUIRE_AUTH=1` enforced when provider is active, `dispatch:execute` scope required for provider dispatches, per-dispatch and daily cost caps via `ACP_COST_PER_DISPATCH_USD`/`ACP_COST_DAILY_USD`, dynamic dashboard boundaries reflect real provider state, structured startup summary log. 10 new Rust tests (1041 total). |
| Productization Phase 2 — Permission Governance | **IMPLEMENTED** — API key create/revoke/rotate/delete/update-scopes via HTTP (POST /keys, POST /keys/:id/revoke, POST /keys/:id/rotate, DELETE /keys/:id, POST /keys/:id/scopes). Team member create/update-role/delete via HTTP (POST /team, PUT /team/:id, DELETE /team/:id). `last_used_at` tracking on auth, `expires_at` support, `revoked_at` enforcement in TenantResolver. Admin audit events for all mutations. All mutation endpoints require `team:admin` scope. TypeScript + Python SDK CRUD methods. Dashboard Team tab with management UI. 8 new Rust integration tests (606 total Rust tests). |
| Productization Phase 3 — Cost Governance | **IMPLEMENTED** — Enriched `cost_summary()` to v2 schema with `total_estimated_cost_usd`, `total_input_tokens`, `total_output_tokens`, `cost_utilization` ratio, per-tier estimated/tokens breakdown, and daily cost trend. New `dispatch_cost_details()` method and `GET /api/v1/costs/dispatches` endpoint for per-dispatch cost rows. Dashboard Costs component enhanced with reserved vs estimated comparison, utilization metric, token usage totals, and daily trend bars. TypeScript + Python SDKs typed for `LocalCostSummary` and `LocalDispatchCostDetail`. 15 new Rust tests (1056 total). |
| Productization Phase 4 — Data Operations | **IMPLEMENTED** — Versioned SQLite migrations via `PRAGMA user_version` (v1: adds `last_used_at`/`expires_at` columns). `check_integrity()` method with `PRAGMA integrity_check` and per-table row counts. `import_snapshot()` for idempotent import from export JSON. `GET /api/v1/storage/integrity` endpoint. `POST /api/v1/import` endpoint (requires `confirm_import=true`). Backup restore hardened: `restore_backup_with_verify()` with post-restore integrity check and row count. `POST /api/v1/backups/:id/restore` endpoint (requires `confirm_restore=true`). Data directory documentation at `docs/DATA_DIRECTORY.md`. 19 new Rust tests (1075 total). |
| Productization Phase 5 — Native Packaging | **IMPLEMENTED** — `.env.example` with all 16 env vars documented. `scripts/install.sh` installs engine binary + dashboard to `~/.agent-control-plane/`. `scripts/upgrade.sh` swaps binary with permission preservation. `scripts/package-release.sh` builds release binary + static dashboard + assembles tarball. `scripts/smoke_release.sh` extracts tarball, installs, starts engine, verifies health/readiness/dispatch/dashboard. Release artifact: `dist/agent-control-plane-v0.1.0-linux-x86_64.tar.gz` (4 MB). |
| Productization Phase 6 — Dashboard Controls | **IMPLEMENTED** — Dispatch detail drill-down (click row to see full bundle with analysis/decision/execution/evaluation sections). Backups tab with list/create/restore/delete and confirmation dialogs. Audit log tab with collapsible details. Team tab confirmation dialogs for all destructive actions. Settings tab enhanced with provider health status. New Rust endpoints: `GET /api/v1/dispatches/:dispatch_id`, `GET /api/v1/backups`, `DELETE /api/v1/backups/:backup_id`. `get_dispatch()` on LocalProductStore. 11 new Rust tests (1086 total). 6 new TypeScript SDK methods + 6 tests (13 total). 6 new Python SDK methods + 6 tests (17 total). |
| Rust + TypeScript Cutover | **COMPLETE** — primary verification is `bash scripts/verify_rust_typescript_stack.sh`, covering Rust fmt/clippy/tests, TypeScript SDK test/build, dashboard lint/typecheck/build/static export, native Rust API + dashboard smoke, and deterministic dispatch smoke. Python is retained as REST SDK and utility scripts only; legacy reference implementation retired. |
| Productization Phase 7 — Long-Run Hardening (part 1) | **IMPLEMENTED** — SQLite contention tests (6 tests: concurrent dispatch writes, concurrent reads during writes, concurrent audit events, no-deadlock contention, data integrity after concurrent writes, concurrent dispatch read-by-id). Provider failure matrix tests (21 tests: retry exhaustion, fallback routing, budget-exhausted mid-retry, non-retryable errors, disabled provider, cost gate blocks, audit trail on success/failure, governance blocks, backoff strategies, concurrent provider invocations). 27 new Rust tests (1113 total). |
| Productization Phase 7 — Long-Run Hardening (part 2) | **IMPLEMENTED** — Audit integrity tests (7 tests: dispatch/config/api-key mutation audit correctness, audit log ordering monotonicity, audit persistence across store reopen, concurrent audit writes non-corruption, integrity report audit_log row count). Enhanced smoke_release.sh: tarball structure verification (6 file/dir checks), install script smoke, data preservation across upgrade, port conflict retry (3 attempts), integrity endpoint smoke. 7 new Rust tests (1120 total). |
| CLI Executor Routing — Complexity-Based Dispatch | **IMPLEMENTED** — `engine/src/cli/` module with `ClaudeCodeCliExecutor` (spawns `claude -p --output-format json`), `CodexCliExecutor` (spawns `codex exec`), `MultiExecutor` (tier-based routing), and `CliConfig` (env-var config, binary detection). Complexity-based escalation: score >= 0.7 escalates cheap/balanced/strong tiers to `claude_code_cli`. Tier map: `code_generate`→`codex_cli`, `code_refactor`→`codex_cli`, `code_debug`/`architecture_plan`/`architecture_design`→`claude_code_cli` (via policy or complexity escalation). Env vars: `ACP_ENABLE_CLI_EXECUTION`, `ACP_CLAUDE_CODE_BIN`, `ACP_CODEX_BIN`, `ACP_CLI_TIMEOUT_MS`, `ACP_CLI_COMPLEXITY_THRESHOLD`. 10 new Rust tests (1130 total). |
| Product-Readiness Repair Pass | **IMPLEMENTED** — P0 fixes: smoke_release.sh integrity endpoint drift (fixed path + OpenAPI route guard test), hardcoded LOCAL_NOW replaced with injectable clock in LocalProductStore (chrono::Utc::now() + new_with_clock for tests), CLI executor dispatch wrapped in spawn_blocking for async HTTP safety, CLI timeout enforced via spawn_with_timeout (kills child on deadline), dashboard ApiError type with status code awareness and visible error states for all tabs, dashboard protected-mode auth flow (token input panel, localStorage, Authorization header, auth status states), createBackup function and Create Backup button, threat model rewritten for current state. 6 new Rust tests (1136 total). |
| P1 Local-Beta Follow-Up | **IMPLEMENTED** — (1) GET /api/v1/keys metadata-only key list endpoint + SDK methods + 3 tests. (2) Search/filter/pagination for dispatches and audit (server-side ?limit=&offset=, client-side text search, 25-item pagination). (3) Bookmarkable tabs via URL hash with browser back/forward. (4) 60-second auto-refresh with visibility-aware pausing and last-updated timestamp. (5) Docker named volume persistence for SQLite data. (6) Key reveal modal replacing alert() with copy button. (7) Dashboard split from 1358-line monolith into 12 focused components under components/. 4 new Rust tests (1140 total). |
| P2 Local-Beta Polish & Type Hardening | **IMPLEMENTED** — (1) CSS design token cleanup: hardcoded `#c0392b` replaced with `var(--risk)` across 7 locations, new `.risk-action`, `.flex-row`, `.flex-between`, `.flex-end`, `.search-input` utility classes. (2) TypeScript SDK type hardening: 22 new focused response interfaces in `wire-types.ts`, all 21 broad `Record<string, unknown>` method return types replaced with focused types (only `openapi()` remains loose); `ExecutorType` extended with `"claude_code_cli"` and `"codex_cli"`, `ExecutionStatus` extended with `"cli_completed"` and `"provider_completed"`. (3) Dashboard component quality: extracted `usePaginatedSearch` hook, `SearchBar` component, `Pagination` component; AuditLog and Dispatches now use shared search/pagination. (4) Next.js app polish: enhanced metadata with description, `loading.tsx` and `error.tsx` surfaces, SVG favicon. 0 new Rust tests (1140 total), 16 SDK tests pass. |
| Toolchain Consolidation & Drift Guard | **IMPLEMENTED** — Standardized all authoritative docs to `uv run --no-project python` (fixed 8 stale bare `python3` references across 9 authoritative files). Added toolchain table to README.md. Extended `verify_rust_typescript_stack.sh` preflight to check `bun`, `cargo`, and `uv` with actionable install hints. Added `scripts/check_toolchain_drift.sh` drift guard that fails when stale JS/Python toolchain references reappear in authoritative docs/scripts, with allowlist for historical references. Integrated drift guard into autonomous closeout workflow. CI already aligned: Bun via `oven-sh/setup-bun@v2`, uv via `astral-sh/setup-uv@v5`, Rust via `dtolnay/rust-toolchain@stable`. 0 new Rust tests (1140 total). `uv.lock` intentionally not added (pure stdlib, no deps). |
| Architecture Refactor R1 — http_server split | **IMPLEMENTED** — Replaced 2077-line `engine/src/http_server.rs` monolith with `engine/src/http_server/` module directory (16 files, 2292 lines). Split into: `mod.rs` (re-exports, request types, openapi_document), `state.rs` (AxumApiState, ServerConfig), `middleware.rs` (auth, rate-limit, CORS, helpers), `routes.rs` (router construction, dashboard serving), `server_context.rs` (ServerContext, RouteMatch, match_path + tests), and 10 handler modules (health, dispatch, team, keys, costs, backups, audit, provider, dashboard, data_ops). Public API unchanged — `build_axum_router`, `build_axum_router_with_dashboard`, `AxumApiState` re-exported from `crate::http_server`. 1140 Rust tests pass. Commit `f2c5ac3`. |
| Architecture Refactor R2 — local_product_store split | **IMPLEMENTED** — Replaced 1365-line `engine/src/storage/local_product_store.rs` monolith with `engine/src/storage/local_product_store/` module directory (12 files). Split into: `mod.rs` (struct, constructors, shared helpers, DDL, re-exports), `dispatch.rs`, `config.rs`, `team.rs`, `keys.rs`, `audit.rs`, `provider_audit.rs`, `costs.rs`, `migrations.rs`, `integrity.rs`, `export_import.rs`, `boundaries.rs`. Public API unchanged — `LocalProductStore`, `local_boundaries`, `IntegrityReport`, `ImportResult`, `ImportCounts`, schema version constants all re-exported from `crate::storage::local_product_store`. Single connection owner preserved. 1140 Rust tests pass. Commit `3c9439b`. GPT PASS (connection model, import/export, audit ordering, cost summary all verified). |
| Architecture Refactor R3 — task_analyzer split | **IMPLEMENTED** — Replaced 1117-line `engine/src/task_analyzer.rs` monolith with `engine/src/task_analyzer/` module directory (5 files). Split into: `mod.rs` (TaskAnalysis struct, RuleBasedTaskAnalyzer, analyze(), round4()), `rules.rs` (static keyword/phrase/multiplier maps), `classify.rs` (classify_domain, classify_intent), `risk.rs` (detect_risk_flags, positive_risk_text, is_negated_occurrence), `scoring.rs` (compute_complexity, estimate_budgets, assess_confidence, derive_risk_level, derive_quality_requirement, determine_safe_default, determine_escalation, detect_capabilities, detect_features). Public API unchanged — `TaskAnalysis`, `RuleBasedTaskAnalyzer`, `TASK_ANALYSIS_SCHEMA_VERSION`, `analyze` all re-exported from `crate::task_analyzer`. Golden fixture outputs preserved. 1140 Rust tests pass. Commit `8813a4d`. GPT PASS. |

Trial 2 complete evidence chain: [`docs/trials/TRIAL_2_FINAL_STATE_INDEX.md`](trials/TRIAL_2_FINAL_STATE_INDEX.md).
Trial 3 report: [`docs/trials/TRIAL_3_REPORT.md`](trials/TRIAL_3_REPORT.md).
Trial 3 target merge closeout: [`docs/trials/TRIAL_3_TARGET_MERGE_CLOSEOUT.md`](trials/TRIAL_3_TARGET_MERGE_CLOSEOUT.md).

## Phase Closeout Summary

All dispatch kernel phases (1–7, including 6A, 6B-1/2/3, Gates 1–3) are STABLE and GPT-approved. Detailed closeout records for each phase are preserved in git history. Key boundaries maintained throughout: no real provider calls in CI, no sandbox execution, no target repo writes, no autonomous workers. Accepted limitations (heuristic boundaries, in-memory stores, rule-based decomposition) are documented in `docs/dispatch/DISPATCHER_KERNEL_V0_ARCHITECTURE.md`.

## Current App Capability

The local Harness App (MVP0–MVP8) provides:

- **Repo registry** — register local or remote target repositories.
- **Local target audit** — read-only inspection of harness control files in a target repo.
- **Non-executable planning** — deterministic resource plans with steps, budgets, approval gates, and blockers. Plans are never executed.
- **App-owned plan store** — plans persist in a local JSON file owned by the app.
- **Plan review workbench** — plan history, summary, comparison, and advisory review actions.
- **Review guidance** — non-persistent advisory guidance derived from stored plans.
- **Portfolio triage** — read-only ranking of stored plans by risk, budget, and bottleneck.
- **Operations diagnostics** — component health, data flow, storage status, recent errors.

The local Agent Control Plane runtime now also provides:

- **SQLite local team state** — app-owned dispatch history, config, team/API-key metadata, audit log, and cost summary.
- **Live local dashboard state** — dashboard reads `/api/v1/dashboard` from the Rust engine instead of fixture rows.
- **Local role boundary** — optional `ACP_REQUIRE_AUTH=1` plus `ACP_ADMIN_API_KEY` enables scoped local API keys; admin-only backup requires `backup:admin`.
- **Export and backup** — `/api/v1/export` exports app-owned state; `/api/v1/backups` creates local SQLite backups only with local auth enabled, `backup:admin`, `confirm_local_backup=true`, and an audit event.
- **SDK access** — TypeScript and Python REST SDKs cover dashboard, dispatch history, config, team, costs, export, audit, dispatch, and confirmed backup.

## State Boundary

| State | Owner | Writable | Description |
|---|---|---|---|
| Target repositories | User | No (read-only by app) | The app never writes to target repos. |
| App registry | App | Yes | Stores registered repo metadata. |
| Plan store | App | Yes | Stores non-executable resource plans. |
| Diagnostics | Derived | No | Computed on each request from app state. |
| Review guidance | Derived | No | Computed from plan store. Not persisted. |
| Portfolio triage | Derived | No | Computed from plan store. Not persisted. |
| Agent Control Plane SQLite | App | Yes | Stores local dispatch history, config, team/API-key metadata, cost state, and audit log. |
| Local backups/export | App | Yes | Backup/export operates only on app-owned SQLite state and requires explicit confirmation for backup. |

No app output constitutes execution authority. The human operator remains the final decision-maker.
