# Phase 8 — Final Completion / GA Seal

Date: 2026-06-12
Status: **DONE — Core plan COMPLETE**

## Phase 8 PRs

| PR | Title | Branch | Status |
|---|---|---|---|
| #54 | Phase 8 Scope Lock | docs/phase8-final-completion-scope-lock | MERGED |
| #55 | PostgreSQL Active Trial Closure | phase8/postgres-active-trial-closure | MERGED |
| #56 | Release Hardening | phase8/release-ops-hardening | MERGED |
| #57 | Safety Boundary Closure | phase8/safety-boundary-closure | MERGED |
| #58 | Docs Consolidation | phase8/docs-runbook-release-gate | MERGED |
| #59 | Final Seal | docs/phase8-final-seal | MERGED |
| #60 | Final Consistency Cleanup | docs/phase8-final-consistency-cleanup | THIS PR |

## PostgreSQL Active Trial — PASSED

CI `pg-integration-tests` job passes with `pg_auto_adjustment_apply_and_rollback_cycle` test.
Test seeds 20 dispatches, enables active auto-adjustment, exercises apply + blocked/active paths
against real PostgreSQL 16 service container. Storage bug fixed: INT4/INT8 type alignment
in `record_dispatch` write path and `pg_dispatch_history_row` read path.

## Validation Results (2026-06-12)

- `cargo fmt --check` ✓
- `cargo clippy -p engine -- -D warnings` ✓
- `cargo test -p engine` — 1534 tests pass ✓
- `uv run --no-project python scripts/check_agent_handoff.py` ✓
- `uv run --no-project python scripts/acp_secret_scan.py` ✓
- `bash scripts/check_wire_codegen_drift.sh` ✓
- `git diff --check` ✓
- All 7 CI jobs green on main ✓

## Definition

Phase 8 is the final planned phase. After Phase 8, no "core completion" phase remains.
Future work is maintenance, bugfixes, pilots, or v2 proposals only.

## Gap Inventory

| Gap | Category | Status | Notes |
|---|---|---|---|
| PostgreSQL active apply+rollback trial | MUST_COMPLETE | **PASSED** | PR #55: pg_auto_adjustment_apply_and_rollback_cycle, INT4/INT8 fix, CI green |
| Local install | VERIFY_ONLY | EXISTS | scripts/install.sh present |
| Upgrade | VERIFY_ONLY | EXISTS | scripts/upgrade.sh present |
| Backup creation | VERIFY_ONLY | EXISTS | GET /api/v1/backups + POST /api/v1/backups with confirm |
| Backup restore | VERIFY_ONLY | EXISTS | POST /api/v1/backups/:id/restore with confirm |
| Native runtime smoke | VERIFY_ONLY | EXISTS | scripts/smoke_native_runtime.py in CI |
| Docker build/compose | VERIFY_ONLY | EXISTS | docker-compose.yml + 3 Dockerfiles, CI docker-build job |
| Dashboard/operator surface | VERIFY_ONLY | EXISTS | Phase 7 DONE, read-only, lint-readonly enforced |
| Auth and API scopes | VERIFY_ONLY | EXISTS | ACP_REQUIRE_AUTH, team:admin, scoped keys |
| Audit log | VERIFY_ONLY | EXISTS | append_audit + search_audit_events + dashboard Audit tab |
| Health/readiness/metrics | VERIFY_ONLY | EXISTS | GET /health, /ready, /metrics, /metrics/observability |
| Provider default-off boundary | VERIFY_ONLY | EXISTS | ACP_ENABLE_PROVIDER_EXECUTION=1 required |
| CLI default-off boundary | VERIFY_ONLY | EXISTS | ACP_ENABLE_CLI_EXECUTION=1 required |
| Target repo write boundary | VERIFY_ONLY | EXISTS | App never writes target repos |
| Release/tag/deploy boundary | VERIFY_ONLY | EXISTS | No auto release/tag/deploy behavior |
| Mutation controls decision | REJECT_FOR_V1 | DOCUMENTED | v1 GA is read-only operator UI + API/CLI/admin backend |
| Documentation consistency | MUST_COMPLETE | **DONE** | PR #60: final consistency cleanup, stale terms resolved |
| CI completeness | VERIFY_ONLY | EXISTS | 7 CI jobs covering Rust/Python/TS/Docker/PG/native |

## PostgreSQL Active Trial Plan

CI already runs PostgreSQL 16 service container with `ACP_TEST_DATABASE_URL`.
Existing pg-tests cover: config, plans, runs, decisions, executor pool, heartbeat, audit, provider audit, supervised patches.
Missing: auto-adjustment apply/rollback cycle against PostgreSQL.

Plan: add pg integration test that:
1. Seeds dispatches via `record_dispatch`
2. Sets `ACP_ENABLE_AUTO_ADJUSTMENT=1`, `ACP_AUTO_ADJUSTMENT_ACTIVE=1`
3. Calls `apply_auto_adjustment` with `confirm_auto_adjustment=true`
4. Verifies snapshot persisted, proposal active, audit events recorded
5. Calls `rollback_auto_adjustment` with `confirm_auto_adjustment_rollback=true`
6. Verifies rollback state, proposal restored, audit events recorded
7. Cleans up env vars

## Safety Boundary Checklist

- Dashboard: read-only operator UI, no mutation controls (enforced by lint-readonly.mjs)
- Provider execution: default-off, env-gated (`ACP_ENABLE_PROVIDER_EXECUTION=1`)
- CLI execution: default-off, env-gated (`ACP_ENABLE_CLI_EXECUTION=1`)
- Target repo writes: disabled by default, never by app runtime
- Release/tag/deploy: no auto behavior, requires explicit human approval
- Active YAML/rubric/policy mutation: requires `team:admin` + confirmation + audit
- Destructive operations: require `team:admin` + confirmation + audit
- Hosted/cloud/multi-tenant: NOT current target, requires separate approval

## Final Seal Criteria

- All MUST_COMPLETE items done
- All CI jobs green on main
- PostgreSQL active trial PASSED
- No stale docs
- No secret scan failure
- No wire drift
- Release package smoke passes
- Safety boundary audit passes
- Phase 8 marked DONE
- Core plan marked COMPLETE
