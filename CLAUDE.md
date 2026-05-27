# Project Instructions

## Architecture-Driven Development

The master architecture document is `docs/dispatch/DISPATCHER_KERNEL_V0_ARCHITECTURE.md`. ALL implementation work must follow this document. It is the single source of truth for:

- Phase definitions, goals, success criteria, and promotion gates
- Schema definitions and field-level contracts
- Component responsibilities and interfaces
- Testing strategy and pass/fail thresholds
- Cross-phase architecture decisions and rationale

## Current State (as of 2026-05-27)

- **Phase 0**: Complete (control plane, 914 tests passing)
- **Phase 1**: Architecture approved, implementation-ready — **NEXT STEP**
- **Phase 2-7**: Actionable outlines, not yet implementation-ready

## Next Action

Start Phase 1 implementation: `src/harness_core/dispatch/` subpackage.

Components to build in order:
1. `task_analyzer.py` — RuleBasedTaskAnalyzer
2. `model_selector.py` — ModelSelector + shadow dual-track
3. `budget_manager.py` — BudgetManager
4. `dispatch_decision.py` — DispatchDecision + BudgetReservation + ExecutionGate schemas
5. `dispatch_engine.py` — DispatchEngine orchestrator
6. `executor_adapter.py` — NoopExecutor, MockExecutor, ManualExecutor
7. `evaluation_stub.py` — EvaluationStub
8. `dispatch_ledger.py` — DispatchRecord, DispatchLedger

Fixtures: 20 golden requests covering all task_domain × task_intent combinations.

## Rules

1. **Always reference the architecture book** before making implementation decisions. If the book is ambiguous or too coarse, discuss with GPT (see below).
2. **Never deviate from schemas** defined in the architecture book without updating the book first.
3. **Phase boundaries are sacred**: Phase 1 MUST NOT call real providers, execute in sandbox, write to target repos, or start autonomous workers.
4. **When blocked or facing coarse granularity**: Discuss with GPT in the same ChatGPT session used for architecture review. Iterate until both agree, then update the architecture book before implementing.
5. **Test-first**: Write tests alongside implementation. Follow the test strategy in Section 4.25 of the architecture book.
6. **Document maintenance**: Keep `docs/CURRENT_STATUS.md` updated as phases complete. Update the architecture book's Completeness Matrix (Section 0.7) when phase maturity changes.

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
- Rule-based logic (no LLM calls in Phase 1)
- Deterministic, testable, auditable
- No comments unless WHY is non-obvious
- Commit messages: English, concise, focus on why
