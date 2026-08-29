# Claude Code Adapter

Read `START_HERE.md` first, then `AGENTS.md`. Those files define repository navigation, implementation authority, quality priorities, hard stops, and the working protocol.

Before acting, generate a fresh context view when possible:

```bash
uv run --no-project python scripts/project_context.py
```

The command is on-demand for this session; canonical CI also publishes a short-lived artifact. Verify the capsule against remote `main`, its bounded live GitHub observations, `docs/ARCHITECTURE.md`, `docs/AUTONOMY.md`, and `docs/ROADMAP.md`. Do not treat stale chat history, an old branch, aggregate unbound approval state, or a blocked downstream PR as accepted truth.

Use the role route in `START_HERE.md` and targeted reads from `docs/ARCHITECTURE.md`, `docs/AUTONOMY.md`, `docs/ROADMAP.md`, and `docs/RUNBOOK.md`. Do not restate their contracts here.

Claude-specific behavior does not expand authority: preserve provider-free CI, fail-closed execution, exact-head review, guarded merge, secrets boundaries, compatibility, recovery, and rollback. Claude managed runtime remains governed by the current accepted repository state, not this adapter.
