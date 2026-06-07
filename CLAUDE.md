# Project Instructions

## Product Scope

**What**: Local deterministic harness and self-hosted macro-orchestrator control plane for studying token-efficient agent workflows. Provides deterministic dispatch planning, local API/dashboard access, app-owned SQLite state, dynamic workflow state, executor coordination primitives, and cost-of-pass metrics.

**What NOT**: Not a cloud production SaaS, coding-agent runtime, or autonomous-agent runtime. No real model-provider calls by default, no sandbox/process/container/VM isolation, no autonomous workers, no target-repo writes. CLI executor is explicit opt-in via `ACP_ENABLE_CLI_EXECUTION=1`.

**Target user**: Solo developer or small local team studying deterministic agent infrastructure on one machine or LAN.

## Architecture Summary

Deterministic, rule-based pipeline:
```
Request → TaskAnalyzer → ModelSelector → BudgetManager → DispatchDecision → Executor → Evaluation → Ledger
```

Key principles:
- Rule-based only (no LLM calls in dispatch kernel)
- Dataclass schemas, no pydantic
- Phase boundaries enforce safety
- Event-sourced ledger for auditability

Master document: `docs/dispatch/DISPATCHER_KERNEL_V0_ARCHITECTURE.md`

## Current State (2026-06-07)

**Stable Components:**
- Dispatch Kernel Phases 1-7 (including 6A, 6B-1/2/3, Gates 1-3): STABLE
- Language migration: COMPLETE (Rust engine is sole runtime)
- Dashboard UX Polish + Production-like Local Ops Hardening: IMPLEMENTED
- Supervised Autonomous Beta Planning Batch 3-7: IMPLEMENTED
- Dormant Module Adaptation Track: 4 phases COMPLETE
- Dynamic Workflow Batch 1 (Persisted Graph Mutation Runtime): COMPLETE (1125 Rust tests)
- Dynamic Workflow Batch 2 (DynamicWorkflowController): COMPLETE (1149 Rust tests)
- Dynamic Workflow Batch 3 (Feedback-Driven Routing): COMPLETE (1159 Rust tests)
- Dynamic Workflow Batch 4 (Dynamic Decomposition): COMPLETE (1205 Rust tests)
- Dynamic Workflow Batch 5 (Agent Profiles): COMPLETE (1193 Rust tests)
- Dynamic Workflow Batch 6 (Tool Registry): COMPLETE (1205 Rust tests)
- Dynamic Workflow Batch 7 + scheduler dynamic-mode recovery: COMPLETE (1208 Rust tests)

**Key Milestones:**
- 1243 Rust tests pass, 0 failures
- TypeScript strict + readonly lint + build + static export pass
- `cargo fmt`, `cargo clippy`, handoff guard all pass

## Known Technical Debt

See `docs/CURRENT_STATUS.md` for full details. Key items:
- Compound "or" negations only match first phrase
- Evidence spans use placeholder positions
- Budget pressure is diagnostic only
- Token estimates are rough char/4

## New Session Bootstrap

Start every session by reading:
1. `docs/SESSION_START_HERE.md`
2. `docs/CURRENT_STATUS.md`
3. `docs/NEXT_DECISION.md`
4. `docs/MODULE_MAP.md`

## Autonomous Advancement Protocol

For each autonomous session:

1. Inspect `git status --short --branch` and read the session bootstrap docs.
2. Choose the highest-value safe task from failing verification, CI/docs/test drift, concrete review findings, or narrowly scoped hardening.
3. **Use Workflow tool for multi-step implementation tasks.** Write a workflow script to `.claude/workflows/` with parallel agents for independent subtasks (e.g., Rust module + API endpoint + SDK + Dashboard). Use `model: 'opus'` for implementation agents and `model: 'sonnet'` for verification. Do NOT implement multi-file features sequentially by hand when Workflow can orchestrate.
4. Update or add tests before behavior changes.
5. Run the full verification suite: `cargo test -p engine`, `cargo fmt --check`, `cargo clippy -p engine --all-targets -- -D warnings`, TypeScript build/test, dashboard build, `uv run --no-project python scripts/check_agent_handoff.py`, `bash scripts/check_wire_codegen_drift.sh`.
6. **CI must be green before starting the next batch.** After pushing, use `gh run list --limit 3` to check CI status. If CI fails, fix and re-push. A green CI is required before the next session's work is considered safe to build on.
7. Update the smallest necessary handoff surface before commit.
8. Commit in English and push when the working tree only contains this session's intended changes.
9. Leave the next action, latest commit, verification, and residual risks in the final report.

## Architecture Refactor R-series

**Architecture Refactor R-series**: **SEALED AT R7**. R8 is not approved. The `checkpoint.rs` split and `dispatch_decision.rs` split are deferred. No further R-series file splitting is approved.

## Post-R7 Wire/Type Governance Hardening

**Post-R7 Wire/Type Governance Hardening**: IMPLEMENTED (`app_layer` dormant-reference annotation, Rust typed round-trip guardrail, active execution-result schema enums, generated/manual TypeScript split, schema-driven enum codegen with drift enforcement via `scripts/check_wire_codegen_drift.sh`, localized dashboard union reuse).

## Rules

1. **Reference architecture book** before implementation decisions
2. **Never deviate from schemas** without updating the book first
3. **Phase boundaries are sacred**: Follow safety constraints strictly
4. **When blocked**: Discuss with GPT, iterate until agreement, then update architecture book
5. **Document maintenance**: Keep handoff surface current and small
6. **Autonomous closeout**: Run `uv run --no-project python scripts/check_agent_handoff.py` before commit
7. **Single forward plan**: `docs/NEXT_DECISION.md` is the only roadmap surface
8. **Ultracode mode requires Workflow tool**: When `/effort ultracode` is active, all multi-step, multi-file, or cross-module tasks must use Workflow tool for dynamic multi-agent orchestration. Never use direct `Agent` tool calls or sequential `await agent(...)` for tasks that should be parallelized or orchestrated.

## Documentation Maintenance

Before committing, update smallest necessary handoff surface if change affects:
- Status, scope, tests, commands, boundaries, modules, or next steps

Authoritative surfaces:
- `docs/CURRENT_STATUS.md` — current state, verification, test counts
- `docs/NEXT_DECISION.md` — single forward plan
- `docs/MODULE_MAP.md` — source/test ownership
- `README.md`, `CLAUDE.md`, `AGENTS.md` — quickstart, agent workflow, boundaries

## GPT Collaboration Protocol

ChatGPT session: https://chatgpt.com/c/69fc96b0-2e48-839f-a031-557e9e2317ca

When you encounter schema ambiguity, interface questions, phase boundary edge cases, or cross-phase integration uncertainties:
1. Get GPT's analysis
2. Independently audit suggestions
3. Share perspectives with user if needed
4. Update architecture book with agreed changes
5. Push to GitHub for GPT reference

## Code Style

- Python 3.10+, dataclasses for schemas, no pydantic
- Rule-based logic (no LLM calls in dispatch kernel)
- Deterministic, testable, auditable
- No comments unless WHY is non-obvious
- Commit messages: English, concise, focus on why

## Test Strategy

- **Framework**: Rust `cargo test` for engine; Python `unittest` for SDK
- **Run commands**: `cargo test -p engine` (primary); SDK tests in `sdk/python/`
- **Current count**: 1226 Rust tests, 0 failures
- **CI**: GitHub Actions on push/PR to main
- **Test-first**: Write tests alongside implementation

## External Dependencies

- Python stdlib only — zero runtime dependencies
- No runtime LLM dependencies in dispatch kernel
