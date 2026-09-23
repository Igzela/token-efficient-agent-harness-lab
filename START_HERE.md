# Start Here

This is the repository's direct entrypoint. Read it, then use the user's latest request as the task. No generated assignment, session bootstrap, journal, or agent service is needed to begin ordinary development and maintenance.

## Choose and start the work

1. The user's latest explicit request is the objective. If there is no request, refresh `main`, open pull requests, issues, and CI; choose the next unblocked product-development or repository-maintenance item in `docs/ROADMAP.md`. Experiments are separate and are not a substitute for this mainline.
2. Check the current branch, head, and worktree. Make changes on a feature branch, never directly on protected `main`.
3. Read only the relevant owner documentation, source, and tests. `docs/ARCHITECTURE.md` owns architecture and module boundaries; `docs/AUTONOMY.md` owns verification, review, CI, merge, and recovery; `docs/ROADMAP.md` owns product-development priorities; `docs/RUNBOOK.md` owns proven operator procedures; `AGENTS.md` owns implementation permissions and stop boundaries.
4. Implement one coherent change. Run focused tests, applicable repository checks, and `git diff --check`; update the canonical owner document when behavior or policy changes.
5. Review the complete diff against the exact base. Keep changing pull requests Draft. A new head invalidates prior review and CI. Ready/merge only after independent exact-head review and all required canonical CI pass. Use the guarded merge workflow and verify the merged commit on `main`.

## Safety and evidence

- Keep one owner for runtime, persistence, policy, and external effects; see `docs/ARCHITECTURE.md`.
- Preserve existing user work and rollback paths. Do not write secrets, weaken security boundaries, or claim tests, reviews, CI, effects, or acceptance that were not observed.
- Do not make a provider call, target write, release, deployment, or protected-branch mutation without current authority. Stop on an unreconciled external outcome or a decision outside the accepted scope.
- GitHub aggregate approval is not exact-head review. Review and CI evidence applies only to the exact commit it names.

## Optional status summary

`scripts/project_context.py` can summarize accepted-main, checkout, pull-request, review, and CI observations. It is a transport view only: it neither chooses the task nor grants authority. Verify material facts against Git and GitHub.

```bash
uv run --no-project python scripts/project_context.py
uv run --no-project python scripts/project_context.py --offline
```

## Handoff

Report the accepted base, branch and exact head, changed behavior, tests and checks run, exact-head review and CI state, remaining blockers, rollback notes, and updated owner documents. Distinguish local verification from GitHub/merged evidence.

## Documentation boundaries

Keep the seven canonical governance documents concise and non-duplicative: `README.md`, `START_HERE.md`, `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/AUTONOMY.md`, `docs/ROADMAP.md`, and `docs/RUNBOOK.md`. Other entrypoints link to these owners rather than copying their full policy.
