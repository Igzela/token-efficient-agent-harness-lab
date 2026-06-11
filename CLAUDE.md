# Project Instructions

## Product Scope

**What**: Local deterministic harness and self-hosted macro-orchestrator control plane for studying token-efficient agent workflows. Provides deterministic dispatch planning, local API/dashboard access, app-owned SQLite state, dynamic workflow state, executor coordination primitives, and cost-of-pass metrics.

**What NOT**: Not a cloud production SaaS, coding-agent runtime, or autonomous-agent runtime. No real model-provider calls by default, no sandbox/process/container/VM isolation, no autonomous workers, no target-repo writes by app runtime. CLI executor is explicit opt-in via `ACP_ENABLE_CLI_EXECUTION=1`.

**Target user**: Solo developer or small local team studying deterministic agent infrastructure on one machine or LAN.

## Current State (2026-06-11)

**Active tracks:**
- Real-World Testing Mode — validated through real tasks, branches, commits, PRs, CI, gated auto-merge
- Agent Autonomous Maintenance Mode — agents maintain docs/CI/tests/low-risk PR flow under playbook gates

**Complete tracks:**
- Dispatch Kernel Phases 1–7 (including 6A, 6B-1/2/3, Gates 1–3): STABLE
- Language migration: COMPLETE (Rust engine is sole runtime)
- Dynamic Workflow Batches 1–7 + scheduler dynamic-mode: COMPLETE
- Macro-Orchestrator Phases 1–5 repair batch: COMPLETE
- Self-Hosted GA Readiness Track SG-1 through SG-5: COMPLETE
- HA Hardening Track HA-1 through HA-6: COMPLETE (scheduler resilience, automated backup, deep health, circuit breaker, TLS, SQLite encryption)
- HybridExecutor with `ACP_EXECUTION_MODE`: COMPLETE
- Dashboard Onboarding UX ON-1 through ON-5: COMPLETE

**Key facts:**
- 1390 Rust tests pass, 0 failures
- TypeScript strict + readonly lint + build + static export pass
- `cargo fmt`, `cargo clippy`, handoff guard all pass
- **Architecture Refactor R-series**: **SEALED AT R7**. R8 is not approved.
- **Post-R7 Wire/Type Governance Hardening**: IMPLEMENTED

## App Runtime vs Agent Maintenance Boundary

**App/runtime** does not write target repos by default. Target repositories remain protected from direct app writes.

**Agent maintenance** may create branches, commits, PRs, and low-risk merges only through branch+PR workflow under `docs/REAL_WORLD_TESTING_PLAYBOOK.md` gates. This is a repository workflow mode, not an app-runtime feature.

**Requires explicit human approval:** Provider/CLI execution boundary expansion, auth/security boundary changes, DB migrations, release/tag/deploy, active YAML/rubric/policy mutation, destructive operations.

## Minimal Agent Reading Model

**Default:** Read `CLAUDE.md` only.

**Read conditionally:**
- `docs/NEXT_DECISION.md` — when choosing or validating next work
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` — when opening PRs, auto-merging, fixing CI, cleaning docs, or running real-world pilot tasks
- `docs/MODULE_MAP.md` — when changing code or deciding module ownership
- `docs/CURRENT_STATUS.md` — when status facts are unclear or when updating status
- `docs/DOCS_INVENTORY.md` — when adding, moving, archiving, or deleting docs
- `docs/DYNAMIC_GLOBAL_REGULATOR_PLAN.md` — only for strategic architecture planning

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

## Autonomous Advancement Protocol

For each autonomous session:

1. Inspect `git status --short --branch` and read this file.
2. Conditionally read `docs/NEXT_DECISION.md` and `docs/REAL_WORLD_TESTING_PLAYBOOK.md` based on task type.
3. Choose the highest-value safe task from failing verification, CI/docs/test drift, concrete review findings, or narrowly scoped hardening.
4. **All sessions must use Workflow tool for implementation.** Write a workflow script to `.claude/workflows/` with `parallel()` for independent subtasks and `pipeline()` for sequential dependencies. Use `model: 'opus'` for implementation agents and `model: 'sonnet'` for verification. The only exception is trivial single-line edits (typo, doc wording, env var). Anything touching 2+ files goes through Workflow.
5. Update or add tests before behavior changes.
6. Run the full verification suite: `cargo test -p engine`, `cargo fmt --check`, `cargo clippy -p engine --all-targets -- -D warnings`, TypeScript build/test, dashboard build, `uv run --no-project python scripts/check_agent_handoff.py`, `bash scripts/check_wire_codegen_drift.sh`.
7. **CI must be green before starting the next batch.** After pushing, use `gh run list --limit 3` to check CI status. If CI fails, fix and re-push. A green CI is required before the next session's work is considered safe to build on.
8. Update the smallest necessary handoff surface before commit.
9. Commit in English and push when the working tree only contains this session's intended changes.
10. Leave the next action, latest commit, verification, and residual risks in the final report.

## Rules

1. **Reference architecture book** before implementation decisions
2. **Never deviate from schemas** without updating the book first
3. **Phase boundaries are sacred**: Follow safety constraints strictly
4. **When blocked**: Discuss with GPT, iterate until agreement, then update architecture book
5. **Document maintenance**: Keep handoff surface current and small
6. **Autonomous closeout**: Run `uv run --no-project python scripts/check_agent_handoff.py` before commit
7. **Single forward plan**: `docs/NEXT_DECISION.md` is the only roadmap surface
8. **Workflow tool is the default for all implementation**: All sessions use Workflow tool for any task touching 2+ files. Write script to `.claude/workflows/`, use `parallel()`/`pipeline()`, launch with `Workflow({scriptPath})`. Only trivial single-line edits bypass Workflow.

## Documentation Maintenance

Before committing, update smallest necessary handoff surface if change affects:
- Status, scope, tests, commands, boundaries, modules, or next steps

Authoritative surfaces:
- `docs/CURRENT_STATUS.md` — current state, verification, test counts
- `docs/NEXT_DECISION.md` — single forward plan
- `docs/MODULE_MAP.md` — source/test ownership
- `README.md`, `CLAUDE.md`, `AGENTS.md` — quickstart, agent workflow, boundaries

Prefer prune/archive/link over adding more prose.

## Code Style

- Python 3.10+, dataclasses for schemas, no pydantic
- Rule-based logic (no LLM calls in dispatch kernel)
- Deterministic, testable, auditable
- No comments unless WHY is non-obvious
- Commit messages: English, concise, focus on why

## Test Strategy

- **Framework**: Rust `cargo test` for engine; Python `unittest` for SDK
- **Run commands**: `cargo test -p engine` (primary); SDK tests in `sdk/python/`; PostgreSQL integration: `cargo test -p engine --features pg-tests` (requires `ACP_TEST_DATABASE_URL`)
- **Current count**: 1390 Rust tests, 0 failures
- **CI**: GitHub Actions on push/PR to main
- **Test-first**: Write tests alongside implementation

## GPT Collaboration Protocol

ChatGPT session: https://chatgpt.com/c/69fc96b0-2e48-839f-a031-557e9e2317ca

When you encounter schema ambiguity, interface questions, phase boundary edge cases, or cross-phase integration uncertainties:
1. Get GPT's analysis
2. Independently audit suggestions
3. Share perspectives with user if needed
4. Update architecture book with agreed changes
5. Push to GitHub for GPT reference

## External Dependencies

- Python stdlib only — zero runtime dependencies
- No runtime LLM dependencies in dispatch kernel
