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
- **Dispatch Kernel Phase 1-5**: Phase 1-4 stable, Phase 5 BETA (11 source modules, 9 test files, 1413 tests).
- **Phase 5 — Multi-Agent Orchestration**: BETA (11 orchestration modules, 134 tests, GPT review P0/P1 hardened, awaiting re-review).
- **Dispatch Kernel Phase 6-7**: Outlined in architecture book, not yet started.

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
- **2026-05-28**: Phase 5 hardening — all GPT review P0/P1 items fixed, 1 new test file (23 hardening tests, 1406 total). GPT re-review: mandatory dispatch_id fix applied (dispatch_id now required in create_workflow/decompose, no analysis_id fallback). 1413 tests. Awaiting GPT re-review.

## External Dependencies

- **Python stdlib only** — all phases through Phase 3 use `urllib.request` for HTTP, no third-party packages
- **No runtime LLM dependencies** in dispatch kernel itself (provider is pluggable)

## Test Strategy

- **Framework**: unittest (stdlib), no pytest
- **Run command**: `PYTHONPATH=src python3 -m unittest discover -s tests`
- **Current count**: 1413 tests, 0 failures (as of 2026-05-28)
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

Phase 5 — Multi-Agent Orchestration is BETA (1381 tests, 111 new). Awaiting GPT review for Stable verdict. Phase 6 — Observability is the next eligible path after Phase 5 Stable. See `docs/CURRENT_STATUS.md` and `docs/NEXT_DECISION.md`.

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
