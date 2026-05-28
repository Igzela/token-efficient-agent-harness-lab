# Project Instructions

## Product Scope

**What**: A local deterministic harness for studying token-efficient agent workflows. Dispatches tasks to model providers, evaluates responses, tracks cost-of-pass metrics.

**What NOT**: Not a production runtime. No real sandbox execution, no autonomous workers, no deployment, no production web UI.

**Target user**: Solo developer studying agent infrastructure patterns.

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

## Current State (as of 2026-05-28)

- **Original Stage 0-4 task-book**: Complete and sealed.
- **Harness App MVP0-MVP8**: Complete local operations console.
- **Trials 0-3**: Closed, with target repo onboarding and multi-repo generalization complete.
- **Dispatch Kernel Phase 1-6A**: All phases STABLE (Phase 6A: 5 source modules, 5 test files, 1596 tests).
- **Phase 5 — Multi-Agent Orchestration**: STABLE (11 orchestration modules, 1454 tests, GPT approved after 3 review rounds).
- **Phase 6A — Local Durable API/Storage**: STABLE (5 source modules, 5 test files, 1596 tests, GPT approved after 2 review rounds).
- **Phase 6B-1 — Per-server Route Isolation**: Implemented (http_server.py refactored, 1603 tests).
- **Phase 6B-2 — Local API Key + Tenant Boundary**: STABLE (auth.py, auth middleware, 1654 tests, GPT approved).
- **Phase 6B-3 + Phase 7**: STABLE (rate_limiter, backup_manager, plugin_system, sdk, doc_generator; 1918 tests, GPT approved).
- **Phase 6B-3 Gate 1**: IMPLEMENTED (scope checks, rate limiting, 403/429 responses).
- **Security hardening**: redaction logging, http_server body size limit + CORS, checkpoint path traversal fix, 42 new tests for coverage gaps.

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

- **2026-05-27**: Phase 3 STABLE after 4 rounds of GPT review (Alpha → Beta → RC → Stable). 1188 tests, 8 source modules, 8 test files. P0 fixes: provider gate bypass, budget_exhausted terminal, ProviderConfig.enabled enforcement. All commits on main (c0ec508→c631b4d).
- **2026-05-28**: Phase 4 STABLE after 2 rounds of GPT review (Beta → Stable). 1270 tests, 8 new routing modules, 8 new test files. P0 fixes: RoutingSelection dataclass for routing_mode metadata, task_group delimiter `/` to avoid underscore collision. P1 fixes: baseline sample check, routing_experiment_id propagation, duplicate PromotionVerdict removed. Commits ed2c762→66ebbc7.
- **2026-05-28**: Phase 5 initial implementation — 11 orchestration source modules, 9 test files, 111 new tests (1381 total).
- **2026-05-28**: Phase 5 STABLE after 3 rounds of GPT review (Beta → Re-review #1 → Re-review #2 → Stable). 1454 tests, 11 source modules, 12 test files. P0 fixes: dispatch gating, terminal semantics, approval reachability, budget enforcement, registry lifecycle, state unification, mandatory dispatch_id, terminal path cleanup. Commits ba7a01a→c5ff73f.
- **2026-05-28**: Phase 6A initial implementation — 5 source modules (observability, durable_store, storage_migrator, http_server, health_checker), 5 test files, 137 new tests (1591 total). Stdlib only: http.server, sqlite3, logging.
- **2026-05-28**: Phase 6A STABLE after 2 rounds of GPT review (Beta → Stable). 1596 tests. P0 fixes: JSONL parser tuple return, HTTP 500 generic error, DurableStore INSERT/upsert semantics, close() thread safety, HTTP query string stripping. Hardening commit 6d11c0f.
- **2026-05-28**: Phase 6B-1 per-server route isolation implemented. Refactored http_server.py: added ServerContext dataclass, moved routes/store/config from class-level globals to per-server instance. 1603 tests (7 new isolation tests). Design doc created at docs/dispatch/PHASE_6B_AUTH_TENANT_DESIGN.md.
- **2026-05-28**: Phase 6B-2 local API key + tenant boundary implemented. Created auth.py (APIKey, Tenant, TenantResolver, RequestContext, AuthDecision, salted SHA-256 hashing, hmac.compare_digest). Added auth middleware to http_server.py (_authenticate_request, 401 on denied). 1639 tests (36 new auth tests).
- **2026-05-28**: Phase 6B-2 STABLE after 2 rounds of GPT review (BLOCK → PASS). 1654 tests. P0 fix: RequestContext now flows into RouteMatch so handlers access tenant_id/scopes. Hardening: generic 401, token shape validation, scope subset constraint. Commit 6934b72.
- **2026-05-28**: Phase 6B-3 + Phase 7 fan-out — 7 new modules (rate_limiter, backup_manager, plugin_system, plugin_registry, sdk, doc_generator, cli extensions), 7 test files, 192 new tests (1846 total). Stdlib only: bisect, sqlite3, json, ast, argparse, threading. All code-reviewed (4 HIGH + 8 MEDIUM fixed), committed b6d5bc1.
- **2026-05-28**: Phase 6B-3 + Phase 7 STABLE after 1 round of GPT review. 1846 tests. No CRITICAL/HIGH findings. MEDIUM: TenantResolver lock deferred to 6B-3, empty scopes semantic documented, RouteMatch.path normalization deferred. LOW: hash_api_key delimiter, observability integration, auth audit — all 6B-3 scope.
- **2026-05-28**: Real provider integration — AnthropicProvider adapter for mimo (Anthropic-compatible API). Updated evaluation_stub for Phase 3+ provider execution. 1853 tests. Real API calls working (2/3 tasks completed, 1 blocked by approval gate correctly). Demo at demos/real_provider_demo.py. API key from env only.
- **2026-05-28**: Phase 6B-3 Gate 1 enforcement hardening — wired scope checks and rate limiting into HTTP request path. Added route_pattern to RouteMatch, required_scopes to register_route(), AuthorizationDecision in auth.py, _check_scopes() and _check_rate_limit() in http_server.py. Request pipeline: auth → route match → scope check (403) → rate check (429) → handler. 13 new enforcement tests + 7 anthropic_provider tests. 1866 tests total. Committed b404b8f.
- **2026-05-28**: Gate 1 hardening fix — fixed GPT BLOCK findings: (1) empty-scope API keys no longer bypass scope checks, (2) register_route clears stale scopes when re-registering without required_scopes. 4 new tests (1870 total). Awaiting re-review.
- Previous BLOCK findings (b6d5bc1): HIGH-1 rate limit not wired, HIGH-2 scope enforcement missing, HIGH-3 plugin locks unused
- Gate 1 addresses: HIGH-1 (rate limiter in ServerContext + _check_rate_limit), HIGH-2 (scope enforcement + AuthorizationDecision + 403/429)
- HIGH-3 (plugin locks) deferred to Gate 3
- Status: submitted for GPT audit

### GPT Gate Feedback 2026-05-28 (6B-3 + Phase 7)
- Target: Phase 6B-3 + Phase 7 checkpoint (commit b6d5bc1)
- Verdict: PASS (CRITICAL: 0, HIGH: 0, MEDIUM: 3, LOW: 3)
- MEDIUM items: TenantResolver config-time-only (defer to 6B-3), empty scopes = unlimited (documented), RouteMatch.path raw (defer to 6B-3)
- LOW items: hash_api_key delimiter, request_id observability, auth audit — all 6B-3 scope
- Status: passed — Phase 6B-3 + Phase 7 STABLE

### GPT Gate Feedback 2026-05-28 (6B-1)
- Target: Phase 6B-1 checkpoint (commit e4aecb3)
- Verdict: PASS_WITH_NOTES (P0: 0, P1: 3)
- P1 items: _last_context fallback marked legacy, add lock if threaded server, normalize RouteMatch.path before auth
- Status: passed, P1 items deferred to 6B-2

### GPT Gate Feedback 2026-05-28 (6B-2 round 1)
- Target: Phase 6B-2 initial checkpoint (commit 4899cea)
- Verdict: BLOCK (CRITICAL: 0, HIGH: 1, MEDIUM: 4, LOW: 3)
- HIGH-1: RequestContext generated but discarded — not passed to RouteMatch or handlers
- MEDIUM: generic 401 error, token format validation, thread safety docs, scope subset constraint
- Status: fixed, re-reviewed

### GPT Gate Feedback 2026-05-28 (6B-2 round 2)
- Target: Phase 6B-2 hardening checkpoint (commit 6934b72)
- Verdict: PASS (CRITICAL: 0, HIGH: 0, MEDIUM: 3, LOW: 3)
- MEDIUM items: TenantResolver config-time-only (acceptable), empty scopes = unlimited semantic (documented), RouteMatch.path raw (defer to 6B-3)
- Status: passed — Phase 6B-2 STABLE

### GPT Gate Feedback 2026-05-28 (Gate 1 round 1)
- Target: Phase 6B-3 Gate 1 enforcement hardening (commit b404b8f)
- Verdict: BLOCK (CRITICAL: 0, HIGH: 2, MEDIUM: 2, LOW: 0)
- HIGH-1: empty-scope API keys bypass scope checks (`request_context.scopes and` guard in _check_scopes) — FIXED
- HIGH-2: route re-registration with required_scopes=None leaves stale scopes — FIXED
- MEDIUM-1: tests call internal methods directly, no real HTTP 403/429 verification
- MEDIUM-2: rate_limit hardcoded to 60, ignoring Tenant.rate_limit — deferred to 6B-3 tenant config
- Status: HIGH items fixed, re-review pending in ChatGPT

## External Dependencies

- **Python stdlib only** — zero runtime dependencies, no third-party packages. Provider adapters use injectable transport (urllib used only in demo scripts).
- **No runtime LLM dependencies** in dispatch kernel itself (provider is pluggable)

## Test Strategy

- **Framework**: unittest (stdlib), no pytest
- **Run command**: `PYTHONPATH=src python3 -m unittest discover -s tests`
- **Current count**: 1918 tests, 0 failures (as of 2026-05-28)
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

Do not infer a new Stage 5, CA-8, production track, or provider-integration track from old phase names. The original Stage 0-4 work is complete; Dispatch Kernel Phase 4 is only an eligible future path with explicit human approval.

## Next Action

Phase 6B-3 Gate 1 is IMPLEMENTED. Gate 2 and Gate 3 enforcement are the next eligible path. See `docs/CURRENT_STATUS.md` and `docs/NEXT_DECISION.md`.

## Autonomous Advancement Protocol

For each autonomous session:

1. Inspect `git status --short --branch` and read the session bootstrap docs.
2. Choose the highest-value safe task from failing verification, documented phase work, concrete review findings, stale handoff docs, or narrowly scoped hardening.
3. Update or add tests before behavior changes.
4. Run the relevant verification command, plus `python3 scripts/check_agent_handoff.py`.
5. Update the handoff surface before commit: `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`, `README.md`, `CLAUDE.md`, and `AGENTS.md` when their facts changed.
6. Commit in English and push when the working tree only contains this session's intended changes.
7. Leave the next action, latest commit, verification, and residual risks in the final report.

This protocol authorizes the coding agent to advance the repository. It does not authorize adding runtime autonomous workers to the harness.

## Rules

1. **Always reference the architecture book** before making implementation decisions. If the book is ambiguous or too coarse, discuss with GPT (see below).
2. **Never deviate from schemas** defined in the architecture book without updating the book first.
3. **Phase boundaries are sacred**: Phase 1-2 MUST NOT call real providers, execute in sandbox, write to target repos, or start autonomous workers. Phase 3+ allows provider calls with gates.
4. **When blocked or facing coarse granularity**: Discuss with GPT in the same ChatGPT session used for architecture review. Iterate until both agree, then update the architecture book before implementing.
5. **Document maintenance**: Keep `docs/CURRENT_STATUS.md` updated as phases complete. Update the architecture book's Completeness Matrix (Section 0.7) when phase maturity changes.
6. **Autonomous closeout**: Run `python3 scripts/check_agent_handoff.py` before commit. A commit is incomplete if the handoff docs no longer tell the next session what changed, how it was verified, and what should happen next.

## Documentation Maintenance Rule

Before committing any change, update the handoff surface if the change affects status, scope, tests, commands, boundaries, modules, or next steps:

- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `README.md`
- `CLAUDE.md`
- `AGENTS.md`

If no document update is needed, say why in the completion report. New sessions must never have to reconstruct the current state from git log alone.

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
