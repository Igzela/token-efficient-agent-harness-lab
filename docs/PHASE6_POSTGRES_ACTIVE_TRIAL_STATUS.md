# Phase 6 PostgreSQL Active Trial Status

Date: 2026-06-12

Status: **BLOCKED — POSTGRES_TEST_DATABASE_UNAVAILABLE**

## Summary

The PostgreSQL active trial from Phase 5 (PR #40) could not be completed because the required environment variable `ACP_TEST_DATABASE_URL` was not available in the local or CI environment. This document records the exact status, missing dependency, and acceptance criteria for future completion.

## Blocker

| Item | Value |
|---|---|
| Status | BLOCKED |
| Reason | `ACP_TEST_DATABASE_URL` environment variable not set |
| Missing env var | `ACP_TEST_DATABASE_URL` |
| Required setup | A running PostgreSQL instance with a test database accessible via the connection string |

## Commands That Would Be Run

If `ACP_TEST_DATABASE_URL` were available:

```bash
# Run PostgreSQL integration tests
cargo test -p engine --features pg-tests

# Verify disabled mode (default)
ACP_DATABASE_URL=$ACP_TEST_DATABASE_URL cargo run -p engine -- --profile local
# Then: GET /api/v1/auto-adjustments should show disabled mode

# Verify dry-run mode
ACP_DATABASE_URL=$ACP_TEST_DATABASE_URL \
ACP_ENABLE_AUTO_ADJUSTMENT=1 \
ACP_AUTO_ADJUSTMENT_DRY_RUN=1 \
cargo run -p engine -- --profile local
# Then: GET /api/v1/auto-adjustments should show dry_run mode, no active apply available

# Verify active apply
ACP_DATABASE_URL=$ACP_TEST_DATABASE_URL \
ACP_ENABLE_AUTO_ADJUSTMENT=1 \
ACP_AUTO_ADJUSTMENT_ACTIVE=1 \
ACP_REQUIRE_AUTH=1 \
ACP_ADMIN_API_KEY=test-admin-key \
cargo run -p engine -- --profile local
# Then: POST /api/v1/auto-adjustments/apply with admin auth should work

# Verify re-entry rejection
# After apply, second apply on same candidate should be rejected

# Verify rollback
POST /api/v1/auto-adjustments/{adjustment_id}/rollback with admin auth

# Verify repeated rollback rejection or safe no-op

# Verify corrupted hash rejection (isolated test database only)

# Verify audit events
GET /api/v1/audit should show snapshot/apply/rollback events

# Verify final state
GET /api/v1/auto-adjustments should show active_proposals=0, active_snapshots=0
```

## Required Operator Setup

To complete this trial, an operator needs:

1. A running PostgreSQL instance (local or remote)
2. A test database created for the trial
3. The connection string set as `ACP_TEST_DATABASE_URL`
4. The `pg-tests` cargo feature enabled
5. Network access from the engine to the PostgreSQL instance

Example setup:
```bash
# Create test database
createdb acp_phase5_test

# Set environment variable
export ACP_TEST_DATABASE_URL="postgresql://user:password@localhost:5432/acp_phase5_test"

# Run tests
cargo test -p engine --features pg-tests
```

## Acceptance Criteria for Later Completion

The PostgreSQL active trial is complete when ALL of the following are verified against a real PostgreSQL database:

1. **Disabled mode** — default mode works, no active apply available
2. **Dry-run mode** — read-only preview, no active apply, no proposal mutation
3. **Active apply** — eligible candidate can be applied with admin auth + confirmation
4. **Re-entry rejection** — second apply on same candidate is rejected
5. **Rollback** — applied adjustment can be rolled back with admin auth + confirmation
6. **Repeated rollback rejection** — second rollback is rejected or safe no-op
7. **Corrupted hash rejection** — tampered safety hash is detected and rejected (isolated test DB only)
8. **Audit events** — all apply/rollback events are recorded in audit log
9. **Final state** — after all operations, active_proposals=0 and active_snapshots=0

## Relationship to Phase 6

This trial is a Phase 6 acceptance criterion. Phase 6 can be marked DONE with this trial BLOCKED, but the DONE wording must be:

> "Phase 6 DONE for operational readiness and observability, with PostgreSQL active trial explicitly tracked as an external environment-dependent follow-up."

Do not claim PostgreSQL active trial passed unless it actually passed.

## References

- Phase 5 Active Trial Playbook: `docs/PHASE5_ACTIVE_TRIAL_PLAYBOOK.md`
- Phase 6 Operational Readiness Plan: `docs/PHASE6_OPERATIONAL_READINESS_PLAN.md`
- PostgreSQL integration tests: `engine/src/storage/local_product_store/pg_backend/`
- Cargo feature: `pg-tests`
