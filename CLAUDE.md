# Project Instructions

## Scope and Boundaries

This is a local deterministic harness and self-hosted workflow control plane, not a cloud SaaS or unbounded deployment tool. Rust `engine/` owns runtime, API, scheduler, policy, state transitions, and persistence; `LocalProductStore` owns application data. TypeScript is interaction/projection; Python is a bounded adapter/evaluation/research layer.

Target worktrees and `main` remain protected by approval, worktree, branch, patch, audit, budget, kill, and rollback owners. Do not create parallel runtimes, schedulers, stores, state models, or authority systems.

## Current State

PE-1 through PE-6 and the **Post-R7 Wire/Type Governance Hardening**: IMPLEMENTED are accepted. Product Golden Path is default-off and its managed-executor acceptance remains the active gate; later order and packet states are in `docs/NEXT_DECISION.md`. Vader/Issue #208 are stopped, Issue #254 is parked, real OpenCode admission is deferred, and the active Harness is immutable.

## Authority and Safety

Full Agent Autonomy Mode permits repository-scoped, testable, observable, reviewable, verification-gated, and rollbackable work. Record material architecture, authority, schema, security, evaluator, release, or recovery decisions in an authoritative document. Preserve safety, audit, approval, budget, evaluator, compatibility, compensation, and rollback boundaries.

Hard stops: secrets or unredacted sensitive content, falsified evidence, hidden failures, bypassed approval, weakened guards, removed rollback, sealed-evaluator mutation, unbounded external effects, or target-default-branch writes.

## Model Selection

Model and reasoning effort are user/tool settings. Do not edit model configuration merely to satisfy repository instructions; model choice never reduces required tests, review, CI, audit, compatibility, compensation, or rollback.

## Reading Model

Read `AGENTS.md`, then relevant sections of `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/MODULE_MAP.md`. Add `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR/CI/merge work, `docs/ARCHITECTURE_BOOK.md` for architecture/authority/security/recovery, and `docs/RUNBOOK.md` only for proven procedures. Current code, merged history, tests, and CI are authoritative.

## Architecture

```text
Request → TaskAnalyzer → ModelSelector → BudgetManager → DispatchDecision → Executor → Evaluation → Ledger
```

The architecture baseline is `docs/ARCHITECTURE_BOOK.md`; wire/type governance is checked by `scripts/check_wire_codegen_drift.sh`.

## Autonomous Advancement Protocol

1. Refresh branch, worktree, `main`, PRs/issues, CI, controls, and active documents.
2. Choose the earliest eligible packet; audit existing code, tests, owners, and overlap.
3. Define scope, non-goals, authority, acceptance, compatibility, risk, and rollback.
4. Add focused tests where practical; implement one coherent slice.
5. Run focused and applicable full checks; repair root causes without weakening guards.
6. Review correctness, authority, security, evaluator integrity, compatibility, audit, recovery, and cost.
7. Update minimal active docs and run `uv run --no-project python scripts/check_agent_handoff.py`.
8. Commit in English, push a focused PR, wait for exact-head green CI and complete-diff review, manually merge only when eligible, refresh `main`, and report evidence.

During long CI or compilation, continue safe read-only review or prerequisite work without starting a later packet or changing authority.

## Rules

- Keep `docs/NEXT_DECISION.md` as the single forward plan and update schemas/contracts with their authoritative documentation.
- Use one focused branch/PR per coherent change; any new head invalidates old CI/review evidence.
- Do not invent evidence or claim implementation/CI success before the underlying check exists.
- Preserve provider-free CI, default-off product gates, target-main protection, manual merge, and rollback.
- Prefer concise comments and English commit messages.

## Documentation Maintenance

Keep these surfaces small and non-duplicative: `docs/ARCHITECTURE_BOOK.md`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`, `docs/REAL_WORLD_TESTING_PLAYBOOK.md`, `docs/RUNBOOK.md`, `README.md`, `CLAUDE.md`, and `AGENTS.md`. Prune stale text before adding prose.

## Test Strategy

Rust `cargo test` covers engine behavior; Python `unittest` covers SDK/tools; Bun covers SDK/Dashboard; PostgreSQL checks cover persistence parity; GitHub Actions validates PRs and `main`. Add fault, recovery, migration, concurrency, browser, or evaluator-integrity checks when touched.
