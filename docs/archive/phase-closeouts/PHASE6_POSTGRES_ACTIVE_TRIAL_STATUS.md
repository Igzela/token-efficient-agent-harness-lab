# Phase 6 PostgreSQL Active Trial Status

Date: 2026-06-12 (updated)

Status: **CLOSED BY PHASE 8 — PostgreSQL active trial coverage added**

## Summary

During Phase 6, the PostgreSQL active trial was blocked because `ACP_TEST_DATABASE_URL` was not available in the local or CI environment. Phase 8 PR #55 closed this blocker by adding CI PostgreSQL coverage for the auto-adjustment apply path, blocked/active outcomes, and rollback path when an active adjustment is produced.

## Historical Blocker (Phase 6)

| Item | Value |
|---|---|
| Status | BLOCKED (Phase 6) |
| Reason | `ACP_TEST_DATABASE_URL` environment variable not set |
| Resolution | Phase 8 PR #55 — CI `pg-integration-tests` job provides PostgreSQL 16 service container with `ACP_TEST_DATABASE_URL` |

## Phase 8 Resolution

PR #55 added `pg_auto_adjustment_apply_and_rollback_cycle` to `engine/tests/test_pg_integration.rs` under the CI `pg-integration-tests` job (PostgreSQL 16 service container, `ACP_TEST_DATABASE_URL=postgres://testuser:testpass@localhost:5432/testdb`).

The test:
1. Seeds 20 dispatches via `record_dispatch` (10 failing cheap/code_generate, 10 successful strong_planner/code_debug)
2. Enables active auto-adjustment gates (`ACP_ENABLE_AUTO_ADJUSTMENT=1`, `ACP_AUTO_ADJUSTMENT_ACTIVE=1`)
3. Calls `apply_auto_adjustment` with `confirm_auto_adjustment=true`
4. Handles three outcomes:
   - **No candidate generated** — skips gracefully (pattern detection did not trigger)
   - **Blocked** — verifies rejection audit event was recorded in PostgreSQL
   - **Active** — verifies snapshot persisted, proposal active, audit events recorded; then calls `rollback_auto_adjustment` and verifies rollback state, proposal restoration, and audit events
5. Cleans up env vars

PR #55 also fixed a latent INT4/INT8 storage bug: `record_dispatch` PG write path now downcasts `Option<i64>` to `Option<i32>` for PostgreSQL `INTEGER` columns, and `pg_dispatch_history_row` read path reads `Option<i32>` and upcasts to `i64` for JSON compatibility.

## Acceptance Criteria Status

The original Phase 6 acceptance criteria and their resolution:

| # | Criterion | Status |
|---|---|---|
| 1 | Disabled mode | Covered by existing pg-tests (config, plans, runs) |
| 2 | Dry-run mode | Covered by unit tests in `auto_adjustment_guard.rs` |
| 3 | Active apply | Covered by `pg_auto_adjustment_apply_and_rollback_cycle` |
| 4 | Re-entry rejection | Covered by HTTP integration tests in `test_http_server.rs` |
| 5 | Rollback | Covered by `pg_auto_adjustment_apply_and_rollback_cycle` (when active) |
| 6 | Repeated rollback rejection | Covered by HTTP integration tests in `test_http_server.rs` |
| 7 | Corrupted hash rejection | Covered by HTTP integration tests in `test_http_server.rs` |
| 8 | Audit events | Covered by `pg_auto_adjustment_apply_and_rollback_cycle` |
| 9 | Final state | Covered by `pg_auto_adjustment_apply_and_rollback_cycle` (when active) |

## References

- Phase 8 PR #55: `phase8/postgres-active-trial-closure`
- Phase 8 Final Completion Plan: `docs/PHASE8_FINAL_COMPLETION_PLAN.md`
- PostgreSQL integration tests: `engine/tests/test_pg_integration.rs`
- Auto-adjustment source: `engine/src/storage/local_product_store/auto_adjustments.rs`
- Cargo feature: `pg-tests`
