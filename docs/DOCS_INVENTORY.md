# Docs Inventory

Last updated: 2026-06-11 (governance cleanup PR #28)

## Classification Key

- **authoritative** — must be read at session start; defines current state and direction
- **operational** — used during execution; referenced by scripts or CI
- **strategic-reference** — long-term planning; read when strategic context needed
- **archive-candidate** — no longer referenced; historical value only

## Inventory

| File | Lines | Classification | Referenced by | Recommendation |
|---|---|---|---|---|
| `docs/CURRENT_STATUS.md` | 214 | authoritative | SESSION_START, NEXT_DECISION | keep |
| `docs/NEXT_DECISION.md` | ~75 | authoritative | SESSION_START, CURRENT_STATUS | keep |
| `docs/SESSION_START_HERE.md` | 157 | authoritative | CURRENT_STATUS | keep |
| `docs/REAL_WORLD_TESTING_PLAYBOOK.md` | ~250 | operational | NEXT_DECISION, SESSION_START | keep (new) |
| `docs/DYNAMIC_GLOBAL_REGULATOR_PLAN.md` | 616 | strategic-reference | NEXT_DECISION | keep |
| `docs/MODULE_MAP.md` | 159 | operational | README, SESSION_START, CURRENT_STATUS | keep |
| `docs/RUNBOOK.md` | 360 | operational | CURRENT_STATUS | keep |
| `docs/DATA_DIRECTORY.md` | 190 | operational | CURRENT_STATUS | keep |
| `docs/security/THREAT_MODEL.md` | 274 | operational | CURRENT_STATUS | keep |
| `docs/security/SCOPE_TEMPLATES.md` | 46 | operational | CURRENT_STATUS | keep |
| `docs/dispatch/DISPATCHER_KERNEL_V0_ARCHITECTURE.md` | 1975 | strategic-reference | CURRENT_STATUS | keep |
| `docs/CI_VERIFICATION.md` | 68 | archive-candidate | none | archive |
| `docs/security/SECURITY_CONTROLS_MATRIX.md` | 16 | archive-candidate | none | archive |
| `docs/security/CA7_SECURITY_REVIEW.md` | 139 | archive-candidate | none | archive |
| `docs/security/README.md` | 47 | archive-candidate | none | archive |
| `docs/dispatch/DISPATCH_WIRE_CONTRACT_V1.md` | 238 | archive-candidate | none | archive |
| `docs/dispatch/PHASE_6B_AUTH_TENANT_DESIGN.md` | 118 | archive-candidate | none | archive |

## Summary

- **Keep:** 11 files (authoritative + operational + strategic-reference)
- **Archive:** 6 files (unreferenced by any authoritative doc)
- **Delete:** 0 (conservative; archive first, delete later if unused)

## Authoritative Read Order

1. `README.md`
2. `docs/CURRENT_STATUS.md`
3. `docs/NEXT_DECISION.md`
4. `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
5. `docs/DYNAMIC_GLOBAL_REGULATOR_PLAN.md` (when strategic context needed)
