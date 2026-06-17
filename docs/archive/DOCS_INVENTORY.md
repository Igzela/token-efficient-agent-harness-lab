# Docs Inventory

Last updated: 2026-06-17

This inventory keeps the active documentation set small. Historical phase plans, closeouts, trial reports, and long-form strategy notes should live under `docs/archive/` unless they are needed for daily maintenance.

## Active Docs

| File | Role | Read When |
|---|---|---|
| `docs/ARCHITECTURE_BOOK.md` | Current architecture baseline | Understanding runtime shape, data ownership, storage, execution modes, or safety boundaries |
| `docs/CURRENT_STATUS.md` | Current project state | Status facts are unclear or a change updates status/capability claims |
| `docs/NEXT_DECISION.md` | Single forward plan | Choosing or validating next work |
| `docs/MODULE_MAP.md` | Source/test ownership map | Changing code or deciding module ownership |
| `docs/DOCS_INVENTORY.md` | Documentation routing | Adding, moving, archiving, or deleting docs |
| `docs/REAL_WORLD_TESTING_PLAYBOOK.md` | Branch/PR/CI/maintenance workflow | Opening PRs, fixing CI, auto-merging, docs cleanup, or real-world pilot tasks |
| `docs/RUNBOOK.md` | Operator procedures | Running, backing up, restoring, checking health, or doing release/rollback drills |
| `docs/DATA_DIRECTORY.md` | App-owned data layout | Changing data directories, backups, artifacts, or export behavior |
| `docs/V1_SAFETY_BOUNDARIES.md` | Current safety boundary | Reviewing provider/CLI/dashboard/workspace/export/target-repo authority |
| `docs/SESSION_START_HERE.md` | Historical bootstrap note | Only when an older workflow specifically references it |
| `docs/security/THREAT_MODEL.md` | Security model | Security review, boundary expansion, auth/export/provider/CLI changes |
| `docs/security/SCOPE_TEMPLATES.md` | Security review helper | Preparing security scopes or review prompts |

Protected fixture:

| File | Rule |
|---|---|
| `docs/stage0/events.jsonl` | Do not modify without explicit approval |

## Archive

| Path | Contains | Use When |
|---|---|---|
| `docs/archive/phase-closeouts/` | Historical phase plans, audits, active-trial notes, and final completion evidence | Auditing why completed tracks were sealed |
| `docs/archive/dispatch/` | Legacy dispatch architecture and wire-contract documents | Investigating historical design decisions |
| `docs/archive/strategy/DYNAMIC_GLOBAL_REGULATOR_PLAN.md` | Long-form strategic regulator plan | Drafting a new v2 strategic proposal |
| `docs/archive/validation/LIVE_E2E_VALIDATION_REPORT.md` | Last live E2E validation report | Capability evidence or release audit |
| `docs/archive/security/` | Older security matrices/reviews | Historical security comparison |

## Maintenance Rules

- Prefer editing an active document over creating another one.
- Prefer archiving stale historical docs over keeping them in the daily reading path.
- Keep `README.md`, `CLAUDE.md`, `AGENTS.md`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/MODULE_MAP.md` consistent when facts change.
- Run `uv run --no-project python scripts/check_agent_handoff.py` after documentation moves.
