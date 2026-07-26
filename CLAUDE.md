# Claude Code Adapter

Read `START_HERE.md` first, then `AGENTS.md`. Those files define repository navigation, implementation authority, quality priorities, hard stops, and the working protocol.

Before acting, generate a fresh context view when possible:

```bash
uv run --no-project python scripts/project_context.py
```

The command is currently on-demand; CI tests the generator but does not automatically inject its output into this Claude Code session. Verify the capsule against remote `main`, the current PR exact head, required CI, exact-head review evidence, `docs/CURRENT_STATUS.md`, and `docs/NEXT_DECISION.md`. Do not treat stale chat history, an old branch, aggregate unbound approval state, or a blocked downstream PR as accepted truth.

Use the role route in `START_HERE.md` and targeted reads from `docs/MODULE_MAP.md`, `docs/ARCHITECTURE_BOOK.md`, `docs/REAL_WORLD_TESTING_PLAYBOOK.md`, and `docs/RUNBOOK.md`. Do not restate their contracts here.

Claude-specific behavior does not expand authority: preserve provider-free CI, fail-closed execution, exact-head review, manual merge, secrets boundaries, compatibility, recovery, and rollback. Claude managed runtime remains governed by the current accepted repository state, not this adapter.
