# Docs Inventory

Last updated: 2026-06-11 (Phase 5 active trial playbook)

## Classification Key

- **authoritative** — defines current state and direction; read only when required by the agent read-order rules below
- **operational** — used during execution; referenced by scripts or CI
- **strategic-reference** — long-term planning; read when strategic context needed
- **archive-candidate** — no longer referenced; historical value only

## Inventory

| File | Lines | Classification | Referenced by | Recommendation |
|---|---|---|---|---|
| `docs/CURRENT_STATUS.md` | 217 | authoritative | SESSION_START, NEXT_DECISION | keep |
| `docs/NEXT_DECISION.md` | 62 | authoritative | SESSION_START, CURRENT_STATUS | keep |
| `docs/SESSION_START_HERE.md` | 94 | authoritative | CURRENT_STATUS | keep |
| `docs/REAL_WORLD_TESTING_PLAYBOOK.md` | 276 | operational | NEXT_DECISION, SESSION_START | keep |
| `docs/DYNAMIC_GLOBAL_REGULATOR_PLAN.md` | 616 | strategic-reference | NEXT_DECISION | keep |
| `docs/DYNAMIC_REGULATOR_PHASE_0_5_COMPLETION_MATRIX.md` | 421 | operational | NEXT_DECISION | keep |
| `docs/PHASE5_AUTO_ADJUSTMENT_AUDIT.md` | 130 | operational | NEXT_DECISION | keep |
| `docs/PHASE5_ACTIVE_TRIAL_PLAYBOOK.md` | 396 | operational | CURRENT_STATUS, NEXT_DECISION, DYNAMIC_REGULATOR_PHASE_0_5_COMPLETION_MATRIX | keep |
| `docs/MODULE_MAP.md` | 176 | operational | README, SESSION_START, CURRENT_STATUS | keep |
| `docs/RUNBOOK.md` | 360 | operational | CURRENT_STATUS | keep |
| `docs/DATA_DIRECTORY.md` | 190 | operational | CURRENT_STATUS | keep |
| `docs/security/THREAT_MODEL.md` | 274 | operational | CURRENT_STATUS | keep |
| `docs/security/SCOPE_TEMPLATES.md` | 46 | operational | CURRENT_STATUS | keep |
| `docs/dispatch/DISPATCHER_KERNEL_V0_ARCHITECTURE.md` | 1975 | strategic-reference | CURRENT_STATUS | keep |

## Summary

- **Keep:** 14 files (authoritative + operational + strategic-reference)
- **Archive:** 0 files
- **Delete:** 0 (conservative; archive first, delete later if unused)

## Agent Read Order

**Default entrypoints (read one):**
- `CLAUDE.md` — Claude Code default entrypoint
- `AGENTS.md` — generic agent default entrypoint

**Conditional reads (read based on task type):**
- `docs/NEXT_DECISION.md` — choosing or validating next work
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` — PRs, auto-merge, CI fix, docs cleanup, pilot tasks
- `docs/MODULE_MAP.md` — code changes, module ownership decisions
- `docs/CURRENT_STATUS.md` — status audit or update
- `docs/DOCS_INVENTORY.md` — adding, moving, archiving, or deleting docs
- `docs/DYNAMIC_GLOBAL_REGULATOR_PLAN.md` — strategic architecture planning only
