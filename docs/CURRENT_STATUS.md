# Current Status

Last recorded full verification: 2026-06-12. Documentation surface pruned: 2026-06-17.

## Summary

The core plan is complete. The repository is in maintenance mode for a local/small-team self-hosted macro-orchestrator control plane. Future work is maintenance, bugfixes, pilots, or explicitly approved v2 proposals only.

The system is useful as an operations/control-plane lab for deterministic dispatch, workflow state, app-owned execution metadata, guarded local controls, SDKs, and audit evidence. It is not a polished SaaS product, cloud service, coding-agent runtime, or unattended autonomous-worker runtime.

## Current Product Boundary

- Rust `engine/` is the sole runtime/API/storage implementation.
- `dashboard/` is the local operator console. It has observability views and guarded app-owned controls; it must not write target repositories or perform release/deploy/apply actions.
- TypeScript and Python SDKs cover REST access to dispatch, workflow, config, team, cost, audit, backup/export, and supervised patch metadata.
- Provider execution is off unless `ACP_ENABLE_PROVIDER_EXECUTION=1`.
- CLI execution is off unless `ACP_ENABLE_CLI_EXECUTION=1`.
- App runtime does not write registered target repositories.
- No hard process/container/VM sandbox is implemented.
- No hosted/cloud/multi-tenant deployment is implemented.
- No unattended autonomous worker loop is approved.

## Last Recorded Verification

- Branch: `main`.
- Tests: **1534 Rust tests pass**, 0 failures, recorded 2026-06-12.
- CI: latest `tests` workflow on `main` was green as of 2026-06-12.
- PostgreSQL integration tests are gated behind `cargo test -p engine --features pg-tests` with `ACP_TEST_DATABASE_URL`.
- Live E2E validation evidence is archived at `docs/archive/validation/LIVE_E2E_VALIDATION_REPORT.md` with 48 PASS, 0 FAIL, 1 SKIP on 2026-06-12.

Handoff guard facts:

- Phase 4 is complete and historical as part of the sealed dispatch-kernel sequence.
- Architecture Refactor R-series Seal: **SEALED AT R7**. R8 is not approved.
- Post-R7 Wire/Type Governance Hardening: `scripts/check_wire_codegen_drift.sh`.

For current verification commands, use:

```bash
bash scripts/verify_rust_typescript_stack.sh
uv run --no-project python scripts/check_agent_handoff.py
```

## Complete Tracks

| Track | Status |
|---|---|
| Dispatch Kernel Phases 1-7, including 6A and 6B Gates 1-3 | Stable |
| Language migration to Rust runtime | Complete |
| Dynamic Workflow Batches 1-7 plus scheduler dynamic mode | Complete |
| Macro-Orchestrator Phases 1-5 repair batch | Complete |
| Self-Hosted GA Readiness SG-1 through SG-5 | Complete |
| HA Hardening HA-1 through HA-6 | Complete |
| HybridExecutor with `ACP_EXECUTION_MODE` | Complete |
| Dynamic Regulator MVP Phases 1-5 | Complete |
| Phase 8 final GA seal | Complete; archived at `docs/archive/phase-closeouts/PHASE8_FINAL_COMPLETION_PLAN.md` |

Historical phase plans, closeouts, and long-form validation reports are retained under `docs/archive/`.

## Active Capability

- Deterministic dispatch pipeline: task analysis, model tier selection, budget reservation, executor selection, evaluation, and ledger persistence.
- Workflow runtime: persisted workflow runs, nodes, edges, events, approvals, queue/backpressure state, executor-pool binding, and opt-in dynamic graph mutation.
- Supervised execution primitives: app-owned workspace lifecycle, `NodeExecutor` trait, allowlisted `CommandNodeExecutor`, workflow tick endpoint, artifact capture, secret scan, integrity validation, approval binding, and export gate.
- Local storage: SQLite default with PostgreSQL optional via `ACP_DATABASE_URL`; schema version is documented in `docs/ARCHITECTURE_BOOK.md`.
- Operations: health, metrics, backups, restore smoke, circuit breaker state, audit log, and release-readiness checks.
- Dashboard: mission-control style local UI for workflow runs, scheduler state, proposals, patches, config/team/costs, and guarded app-owned actions.

## Current Gaps

- Product fit is stronger for local operations/research than for public-facing production UX.
- UI is functional and operator-oriented, but it is not yet a polished commercial product interface.
- Security posture is suitable for local/small-team self-hosting only; hosted/multi-tenant use would require a new threat model and approved implementation plan.
- Sandbox isolation, provider failover, default-on provider execution, target-repo write/apply controls, release/deploy controls, and unattended worker behavior remain out of scope.

## Documentation Discipline

Active documentation is intentionally small:

- `docs/ARCHITECTURE_BOOK.md` — current architecture baseline
- `docs/CURRENT_STATUS.md` — current status and limits
- `docs/NEXT_DECISION.md` — single forward plan
- `docs/MODULE_MAP.md` — source/test ownership
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` — branch/PR/CI/maintenance workflow
- `docs/RUNBOOK.md` — operator procedures

All other Markdown under `docs/` is historical or low-frequency reference material in `docs/archive/`.

Do not add new roadmap, next-step, closeout, status, or productization documents unless the user explicitly asks for a new artifact. Prefer editing, shortening, or archiving existing docs.
