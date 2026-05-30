# Project Instructions

## Product Scope

**What**: A local deterministic harness and self-hosted agent-control-plane for studying token-efficient agent workflows. It provides deterministic dispatch planning, local API/dashboard access, app-owned SQLite history/config/team state, and cost-of-pass metrics.

**What NOT**: Not a cloud production SaaS or autonomous-agent runtime. No real model-provider calls by default, no real sandbox/process/container/VM execution, no autonomous workers, no target-repo writes, and no hosted production deployment.

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

## Current State (as of 2026-05-29)

- **Original Stage 0-4 task-book**: Complete and sealed.
- **Harness App MVP0-MVP8**: Complete local operations console.
- **Trials 0-3**: Closed, with target repo onboarding and multi-repo generalization complete.
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
- **Agent-Control-Plane Phase 8 Closeout**: IMPLEMENTED (`docs/AGENT_CONTROL_PLANE_MIGRATION_CLOSEOUT.md`). Python reference remains in `src/harness_core/` pending explicit future removal decision.
- **Agent-Control-Plane Native Local Runtime**: IMPLEMENTED (`ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine` serves API + dashboard from one Rust process; Docker optional).
- **Agent-Control-Plane Local Small-Team Productization**: IMPLEMENTED (`engine/src/storage/local_product_store.rs`, live dashboard API state, SQLite dispatch history/config/team/API-key metadata/audit/cost state, export, admin-auth-confirmed local backup, SDK local-state methods). Still no cloud SaaS, target writes, real workers, or real sandbox/process execution.
- **Phase 6B-3 Gate 1**: IMPLEMENTED (scope checks, rate limiting, 403/429 responses).
- **Security hardening**: redaction logging, http_server body size limit + CORS, checkpoint path traversal fix, 42 new tests for coverage gaps.
- **Productization Phase 2 — Permission Governance**: IMPLEMENTED (API key create/revoke/rotate/delete/scopes, team member create/update-role/delete, last_used_at tracking, expires_at support, revoked_at enforcement, admin audit events, team:admin scope gating, SDK CRUD methods, dashboard management UI).
- **Productization Phase 3 — Cost Governance**: IMPLEMENTED (cost_summary v2: reserved vs estimated, token usage, utilization ratio, daily trend; dispatch_cost_details endpoint; dashboard enhanced Costs view; typed SDK cost responses; 15 new Rust tests, 1056 total).
- **Productization Phase 4 — Data Operations**: IMPLEMENTED (versioned SQLite migrations via PRAGMA user_version; check_integrity() with PRAGMA integrity_check and per-table row counts; import_snapshot() for idempotent import from export JSON; GET /api/v1/storage/integrity and POST /api/v1/import and POST /api/v1/backups/:id/restore endpoints; backup restore hardened with restore_backup_with_verify(); data-directory documentation; 19 new Rust tests, 1075 total).
- **Productization Phase 5 — Native Packaging**: IMPLEMENTED (.env.example with all 16 env vars; install.sh/upgrade.sh scripts; package-release.sh builds release binary + static dashboard tarball; smoke_release.sh verifies extracted artifact; 4 MB release tarball).
- **Rust + TypeScript Cutover**: COMPLETE (`engine/` is the primary runtime/API/storage/provider-gated control plane; `dashboard/` and `sdk/typescript/` are the primary TypeScript surfaces; `scripts/verify_rust_typescript_stack.sh` is the primary cutover verification. Python remains legacy reference plus retained Python SDK compatibility).

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

- **Framework**: unittest (stdlib), no pytest
- **Run command**: `PYTHONPATH=src python3 -m unittest discover -s tests`
- **Current count**: 2089 Python tests + 1086 Rust tests pass, 0 failures (as of 2026-05-29)
- **Coverage**: Phase boundary contracts, schema validation, golden fixtures
- **CI**: GitHub Actions on push/PR to main — runs security baseline + all tests
- **Test naming**: `tests/test_<module>.py`, one test file per source module
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
2. Choose the highest-value safe task from failing verification, documented phase work, concrete review findings, stale handoff docs, or narrowly scoped hardening.
3. Update or add tests before behavior changes.
4. Run the relevant verification command, plus `python3 scripts/check_agent_handoff.py`.
5. Update the smallest necessary handoff surface before commit: `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`, `README.md`, `CLAUDE.md`, and `AGENTS.md` when their facts changed.
6. Commit in English and push when the working tree only contains this session's intended changes.
7. Leave the next action, latest commit, verification, and residual risks in the final report.

This protocol authorizes the coding agent to advance the repository. It does not authorize adding runtime autonomous workers to the harness.

## Rules

1. **Always reference the architecture book** before making implementation decisions. If the book is ambiguous or too coarse, discuss with GPT (see below).
2. **Never deviate from schemas** defined in the architecture book without updating the book first.
3. **Phase boundaries are sacred**: Phase 1-2 MUST NOT call real providers, execute in sandbox, write to target repos, or start autonomous workers. Current provider execution is an explicit env-gated local beta path and must stay default-off unless a future approved plan changes that boundary.
4. **When blocked or facing coarse granularity**: Discuss with GPT in the same ChatGPT session used for architecture review. Iterate until both agree, then update the architecture book before implementing.
5. **Document maintenance**: Keep the authoritative handoff surface current and small. Prefer shortening existing docs or deleting stale planning docs over adding files.
6. **Autonomous closeout**: Run `python3 scripts/check_agent_handoff.py` before commit. A commit is incomplete if the handoff docs no longer tell the next session what changed, how it was verified, and what should happen next.
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
