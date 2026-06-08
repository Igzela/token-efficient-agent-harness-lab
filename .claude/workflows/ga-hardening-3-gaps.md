export const meta = {
  name: 'ga-hardening-3-gaps',
  description: 'Fix 3 GA gaps: TLS hardening, PostgreSQL integration tests, PostgreSQL backup boundary',
  phases: [
    { title: 'TLS Hardening', detail: 'Fatal exit on single-sided TLS env' },
    { title: 'PG Integration Tests', detail: 'Add pg-tests with real PostgreSQL CI job' },
    { title: 'PG Backup Boundary', detail: 'Block SQLite file-copy backup in PG mode' },
    { title: 'Verify', detail: 'Run full verification suite' },
    { title: 'Docs', detail: 'Update handoff surfaces' },
  ],
}

// ── Phase 1: TLS Hardening ──
// Current: main.rs lines 220-269 use match (cert, key) where the wildcard `_` arm
// silently falls back to HTTP when only one of the two is set.
// Fix: match on (cert, key) with explicit arms for the error case.

phase('TLS Hardening')

await agent(
  `Fix TLS configuration in engine/src/main.rs.

CURRENT PROBLEM (lines 220-269):
The match on (tls_cert_path, tls_key_path) has:
  - (Some(cert), Some(key)) → TLS enabled
  - _ → HTTP fallback (catches BOTH "neither set" AND "only one set")

REQUIRED FIX:
Replace the match with explicit arms:
  - (Some(cert), Some(key)) → TLS enabled (existing behavior)
  - (None, None) → HTTP fallback with message "TLS disabled (neither ACP_TLS_CERT_PATH nor ACP_TLS_KEY_PATH set)"
  - (Some(_), None) → eprintln!("[acp-fatal] ACP_TLS_CERT_PATH is set but ACP_TLS_KEY_PATH is not. Both must be set to enable TLS.") then std::process::exit(1)
  - (None, Some(_)) → eprintln!("[acp-fatal] ACP_TLS_KEY_PATH is set but ACP_TLS_CERT_PATH is not. Both must be set to enable TLS.") then std::process::exit(1)

Also add a unit test in the existing #[cfg(test)] mod tests block:
  #[test]
  fn tls_single_sided_env_not_allowed() — this is a documentation/smoke test that verifies the match logic is correct by testing production_profile_violations_inner or by noting the match arms exist. Since the TLS check happens in async main(), the test should verify the LOGIC, not the async startup. Add a helper function:
  fn validate_tls_config(cert: Option<&str>, key: Option<&str>) -> Result<(), String>
  that returns Err(...) for single-sided, Ok(()) for both-None or both-Some.
  Then call it from main() instead of inline match. Test covers all 4 cases.

CONSTRAINTS:
- Do not change behavior when both are set or both are unset
- Keep the existing TLS server code unchanged
- File: engine/src/main.rs only
- Run: cargo fmt -p engine after editing`,
  { label: 'tls-hardening', phase: 'TLS Hardening', model: 'opus' }
)

// ── Phase 2: PostgreSQL Integration Tests ──

phase('PG Integration Tests')

await agent(
  `Create PostgreSQL integration tests gated behind the pg-tests feature.

CONTEXT:
- engine/Cargo.toml already has: pg-tests = ["pg"] feature
- engine/src/storage/local_product_store/mod.rs has pub fn new_postgres()
- No tests use ACP_TEST_DATABASE_URL or the pg-tests feature today

TASK:
Create engine/tests/test_pg_integration.rs with these properties:
1. Every test function is gated with #[cfg(feature = "pg-tests")]
2. A helper fn test_store() -> LocalProductStore reads ACP_TEST_DATABASE_URL env var; if unset, calls return (skip with eprintln). Use std::env::var and early return pattern.
3. The helper calls LocalProductStore::new_postgres(&url, || utc_now_string) and expects success.
4. Test functions (all gated):
   - pg_new_postgres_creates_store — calls test_store(), asserts it exists
   - pg_ddl_and_migration — calls test_store(), verifies schema_migrations table exists by inserting a config key and reading it back
   - pg_config_upsert_read — upsert a config key "test_key" with json value, read it back, assert equality
   - pg_plan_create_list_detail — create a plan, list plans, get plan detail, assert all succeed
   - pg_workflow_run_create_detail — create a workflow run, get detail, assert fields match
   - pg_decision_record — insert an orchestration decision record, read it back
   - pg_executor_pool — register an executor, query pool, assert entry exists
   - pg_heartbeat — write heartbeat, read it back, assert timestamp matches
   - pg_audit_record — insert an audit log entry, search for it
   - pg_provider_audit — insert a provider audit event, read it back
   - pg_supervised_patch_metadata — create workspace and artifact metadata, list them

5. Each test should be self-contained. Use a unique key/run-id per test (e.g. format!("test-{}", uuid::Uuid::new_v4())) to avoid collisions.

6. Add a note at the top of the file:
   // PostgreSQL integration tests — gated behind pg-tests feature.
   // Set ACP_TEST_DATABASE_URL=postgres://user:pass@localhost:5432/testdb to run.
   // CI runs these with a PostgreSQL service container.

CONSTRAINTS:
- Only touch engine/tests/test_pg_integration.rs
- Do not modify Cargo.toml (pg-tests feature already exists)
- Use chrono::Utc::now for timestamps
- Use serde_json::json! for JSON values
- All tests must compile without pg-tests feature (gated with #[cfg(feature = "pg-tests")])`,
  { label: 'pg-integration-tests', phase: 'PG Integration Tests', model: 'opus' }
)

// Phase 2b: CI PostgreSQL job

await agent(
  `Add a PostgreSQL integration test job to .github/workflows/tests.yml.

CURRENT FILE has jobs: python-tests, rust-tests, typescript-tests, native-runtime, rust-typescript-cutover, docker-build.

ADD new job after rust-tests:

  pg-integration-tests:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:16
        env:
          POSTGRES_USER: testuser
          POSTGRES_PASSWORD: testpass
          POSTGRES_DB: testdb
        ports:
          - 5432:5432
        options: >-
          --health-cmd="pg_isready -U testuser"
          --health-interval=5s
          --health-timeout=5s
          --health-retries=5
    env:
      ACP_TEST_DATABASE_URL: postgres://testuser:testpass@localhost:5432/testdb
    steps:
      - name: Check out repository
        uses: actions/checkout@v6
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Run PostgreSQL integration tests
        run: cargo test -p engine --features pg-tests -- --test-threads=1

CONSTRAINTS:
- Only modify .github/workflows/tests.yml
- Keep existing jobs unchanged
- Use postgres:16 image
- --test-threads=1 because tests share the same database
- Place the job between rust-tests and typescript-tests`,
  { label: 'ci-pg-job', phase: 'PG Integration Tests', model: 'opus' }
)

// ── Phase 3: PostgreSQL Backup Boundary ──

phase('PG Backup Boundary')

await agent(
  `Fix backup behavior in PostgreSQL mode. Three changes needed:

CHANGE 1: engine/src/storage/local_product_store/mod.rs
Add a public method to detect PostgreSQL backend:
  pub fn is_postgres(&self) -> bool {
      match &self.db {
          DatabaseConnection::Sqlite(_) => false,
          #[cfg(feature = "pg")]
          DatabaseConnection::Pg(_) => true,
          #[cfg(not(feature = "pg"))]
          _ => false,
      }
  }

CHANGE 2: engine/src/main.rs — auto-backup guard
In the scheduler setup block (around line 180), AFTER the line:
  if backup_interval_sec > 0 {
Add a check BEFORE creating the BackupManager:
  let store_is_pg = std::env::var("ACP_DATABASE_URL").is_ok();
  if store_is_pg {
      eprintln!("[acp-warning] ACP_BACKUP_INTERVAL_SEC is set but PostgreSQL mode does not support app-managed file-copy backup. Use pg_dump or managed backup. Disabling app auto-backup.");
  } else {
      // existing backup setup code
  }

This means the auto-backup block becomes:
  if backup_interval_sec > 0 {
      if std::env::var("ACP_DATABASE_URL").is_ok() {
          eprintln!("[acp-warning] ACP_BACKUP_INTERVAL_SEC={} is ignored in PostgreSQL mode — use pg_dump or your managed backup service. App auto-backup disabled.", backup_interval_sec);
      } else {
          let bm = ...existing code...
      }
  }

CHANGE 3: engine/src/http_server/handlers/backups.rs (or wherever backup endpoints live)
In the POST /api/v1/backups handler, add a guard:
  If store.is_postgres(), return an error response:
  {"error": "backup_not_supported", "message": "PostgreSQL mode: use pg_dump or managed backup. App file-copy backup is not available for PostgreSQL backends."}
  with HTTP 400 or 422.

Find the backup handler file first:
  grep -rn "POST.*backup\|create_backup\|fn.*backup" engine/src/http_server/ --include="*.rs"

Then add the guard at the top of the create-backup handler function.

CONSTRAINTS:
- SQLite backup behavior must be completely unchanged
- The warning message must be clear and actionable
- Do not touch BackupManager itself — it remains SQLite-only
- Run cargo fmt after edits`,
  { label: 'pg-backup-boundary', phase: 'PG Backup Boundary', model: 'opus' }
)

// ── Phase 4: Verify ──

phase('Verify')

await agent(
  `Run the full verification suite and report results. Run each command sequentially:

1. cargo fmt --check -p engine
2. cargo test -p engine
3. cargo clippy -p engine --all-targets -- -D warnings
4. cargo build -p engine --features pg
5. PATH="$HOME/.bun/bin:$PATH" bash scripts/verify_rust_typescript_stack.sh
6. uv run --no-project python scripts/check_agent_handoff.py
7. uv run --no-project python tools/check_security_baseline.py

Report each result as PASS or FAIL with relevant output.
If any test fails, report the exact error and which test.`,
  { label: 'verify-suite', phase: 'Verify', model: 'sonnet' }
)

// ── Phase 5: Docs ──

phase('Docs')

await agent(
  `Update handoff documentation to reflect the 3 GA gap fixes. Files to update:

1. docs/CURRENT_STATUS.md:
   - In "Current State" section, add a line noting: "GA hardening 3-gap fix: TLS single-sided env fatal exit, PostgreSQL integration tests (pg-tests feature + CI job), PostgreSQL backup boundary guard"
   - Update test count if it changed
   - Update the "PostgreSQL optional storage backend" description to note backup boundary and integration test coverage

2. docs/NEXT_DECISION.md:
   - In "High-Availability Hardening Track" table, update HA-5 TLS row to note: "single-sided TLS env now fatal exits"
   - In "PostgreSQL optional storage backend" section, add: "Integration tests gated behind pg-tests feature with CI PostgreSQL service. App-managed backup disabled in PG mode; operators must use pg_dump or managed backup."

3. docs/MODULE_MAP.md:
   - In "Utility Scripts" or "Infrastructure" section, note: "PostgreSQL integration tests in engine/tests/test_pg_integration.rs (pg-tests feature, ACP_TEST_DATABASE_URL)"

4. README.md:
   - In the PostgreSQL section, add a note: "PostgreSQL integration tests: cargo test -p engine --features pg-tests (requires ACP_TEST_DATABASE_URL)"

5. CLAUDE.md:
   - Update test strategy section to mention pg-tests feature

6. AGENTS.md:
   - Add PostgreSQL integration test verification command

Read each file first, then make minimal targeted edits. Only update facts that changed.`,
  { label: 'handoff-docs', phase: 'Docs', model: 'sonnet' }
)
