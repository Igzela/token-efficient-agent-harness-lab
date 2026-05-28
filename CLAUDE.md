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
- **Dispatch Kernel Phase 1-4**: Complete and stable.
- **Phase 4 — Adaptive Routing**: STABLE (8 source modules, 8 test files, 1270 tests, stable commit 66ebbc7).
- **Dispatch Kernel Phase 5-7**: Outlined in architecture book, not yet started.

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

## External Dependencies

- **Python stdlib only** — all phases through Phase 3 use `urllib.request` for HTTP, no third-party packages
- **No runtime LLM dependencies** in dispatch kernel itself (provider is pluggable)

## Test Strategy

- **Framework**: unittest (stdlib), no pytest
- **Run command**: `PYTHONPATH=src python3 -m unittest discover -s tests`
- **Current count**: 1270 tests, 0 failures (as of 2026-05-28)
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

Phase 4 is STABLE. Phase 5 — Multi-Agent Orchestration is the next eligible path per the architecture book. GPT recommends deciding whether multi-agent orchestration is the right next step before diving in. See `docs/CURRENT_STATUS.md` and `docs/NEXT_DECISION.md`.

## Rules

1. **Always reference the architecture book** before making implementation decisions. If the book is ambiguous or too coarse, discuss with GPT (see below).
2. **Never deviate from schemas** defined in the architecture book without updating the book first.
3. **Phase boundaries are sacred**: Phase 1-2 MUST NOT call real providers, execute in sandbox, write to target repos, or start autonomous workers. Phase 3+ allows provider calls with gates.
4. **When blocked or facing coarse granularity**: Discuss with GPT in the same ChatGPT session used for architecture review. Iterate until both agree, then update the architecture book before implementing.
5. **Document maintenance**: Keep `docs/CURRENT_STATUS.md` updated as phases complete. Update the architecture book's Completeness Matrix (Section 0.7) when phase maturity changes.

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
