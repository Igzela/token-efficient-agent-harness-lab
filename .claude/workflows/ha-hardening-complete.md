export const meta = {
  name: 'ha-hardening-complete',
  description: 'Implement all remaining HA Hardening Track phases (HA-1, HA-2, HA-3, HA-5, HA-6)',
  phases: [
    { title: 'HA-1 Scheduler Resilience', detail: 'catch_unwind, persistent heartbeat, auto-restart with backoff' },
    { title: 'HA-2 Automated Backup', detail: 'Timer-based backup in scheduler loop with retention policy' },
    { title: 'HA-3 Deep Health', detail: 'Resource monitoring, disk/memory checks, webhook alerts' },
    { title: 'HA-5 TLS + HA-6 Encryption', detail: 'TLS inbound and SQLite encryption at rest' },
    { title: 'Verify', detail: 'Full test suite, fmt, clippy, handoff guard' },
  ],
};

// ─── HA-1: Scheduler Resilience + Persistent Heartbeat ───
phase('HA-1 Scheduler Resilience');

// Wave 1: Storage layer (heartbeat table + migration)
const ha1Storage = await agent(
  `Implement the HA-1 storage layer for scheduler heartbeat persistence.

Files to read first:
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/storage/local_product_store/mod.rs (lines 34-36 for schema version, lines 42-327 for DDL pattern, lines 336-362 for constructor)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/storage/local_product_store/migrations.rs (full file — understand migration pattern, CURRENT_SCHEMA_VERSION, MIGRATIONS array)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/storage/local_product_store/integrity.rs (full file — understand table list for integrity check)

Tasks:
1. Create NEW FILE: engine/src/storage/local_product_store/heartbeat.rs
   - Add to LocalProductStore impl (this file adds methods via a separate impl block):
     - write_heartbeat(&self, tick_count: u64, error_count: u64, uptime_seconds: f64, metadata_json: &str) -> Result<(), String>
       Uses: UPDATE scheduler_heartbeat SET last_heartbeat_at=?, tick_count=?, error_count=?, uptime_seconds=?, metadata_json=?, updated_at=? WHERE id=1
     - read_heartbeat(&self) -> Result<Option<SchedulerHeartbeatRow>, String>
       Uses: SELECT last_heartbeat_at, tick_count, error_count, uptime_seconds, metadata_json, updated_at FROM scheduler_heartbeat WHERE id=1
   - Define SchedulerHeartbeatRow struct with serde Serialize/Deserialize
   - Import serde, serde_json, and the with_conn helper from the parent module

2. Modify engine/src/storage/local_product_store/mod.rs:
   - Add 'pub mod heartbeat;' declaration
   - Add 'scheduler_heartbeat' to the integrity check table list in the integrity_tables list or wherever tables are enumerated for integrity checking

3. Modify engine/src/storage/local_product_store/migrations.rs:
   - Add migrate_v11 method that creates the scheduler_heartbeat table:
     CREATE TABLE IF NOT EXISTS scheduler_heartbeat (
       id INTEGER PRIMARY KEY CHECK (id = 1),
       last_heartbeat_at TEXT NOT NULL DEFAULT '',
       tick_count INTEGER NOT NULL DEFAULT 0,
       error_count INTEGER NOT NULL DEFAULT 0,
       uptime_seconds REAL NOT NULL DEFAULT 0.0,
       metadata_json TEXT NOT NULL DEFAULT '{}',
       updated_at TEXT NOT NULL DEFAULT ''
     );
     INSERT OR IGNORE INTO scheduler_heartbeat (id, last_heartbeat_at, tick_count, error_count, uptime_seconds, metadata_json, updated_at)
     VALUES (1, '', 0, 0, 0.0, '{}', '');
   - Update CURRENT_SCHEMA_VERSION from 10 to 11
   - Add entry to the migrations array/vec

Verification: Run 'cargo test -p engine --lib storage::local_product_store' to confirm no regressions. Also run 'cargo fmt --check' and 'cargo clippy -p engine --all-targets -- -D warnings'.

Constraints:
- Follow existing code style: no comments unless WHY is non-obvious
- Use with_conn pattern for all DB access
- Map rusqlite errors to String via .map_err(|e| e.to_string())
- The heartbeat module adds methods to LocalProductStore via a separate impl block in its own file (same pattern as dispatch.rs, config.rs, etc.)
`,
  { label: 'ha1-storage', phase: 'HA-1 Scheduler Resilience', model: 'opus' }
);

// Wave 2: Scheduler catch_unwind + heartbeat writes + auto-restart
const ha1Scheduler = await agent(
  `Implement HA-1 scheduler resilience: catch_unwind wrapper, persistent heartbeat writes, and auto-restart with exponential backoff.

Files to read first:
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/scheduler.rs (full file — understand the spawned thread loop, start() method, tick handling, status() method, SchedulerConfig)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/storage/local_product_store/heartbeat.rs (the file just created in the previous step)

Tasks:
1. Modify engine/src/scheduler.rs:

   a) Add new fields to WorkflowScheduler:
      - panic_count: Arc<AtomicU64>
      - heartbeat_interval_sec: u64 (read from ACP_HEARTBEAT_INTERVAL_SEC env, default 10)
   
   b) In the start() method's spawned thread loop, wrap the scheduler_tick call in std::panic::catch_unwind:
      - The tick function and all its closures need to be UnwindSafe. Since scheduler_tick takes &Store, &Config, &str, &ExecutorPool (all refs), we need to make the call unwind-safe. Use std::panic::AssertUnwindSafe wrapper around the tick call.
      - On Ok(result): handle as before, PLUS write heartbeat to store if enough time has elapsed since last heartbeat write (track last_heartbeat_write_time as Instant)
      - On Err(panic_payload): 
        * Increment panic_count
        * Log the panic payload (extract string if possible)
        * Sleep with exponential backoff: min(panic_count * 1000, 30000) ms
        * Continue the while loop (do NOT break)
      - After a successful tick, reset the backoff (the next panic will start from 1s again)
   
   c) Add heartbeat write logic:
      - Track last_heartbeat_write as Instant in the thread closure
      - After each successful tick, if elapsed >= heartbeat_interval_sec, call:
        store.write_scheduler_heartbeat(tick_count.load, error_count.load, uptime_seconds, "{}")
      - uptime_seconds = started_at.elapsed().as_secs_f64()

   d) Update status() method to include panic_count in the returned serde_json::Value

2. Update the SchedulerConfig::from_env() to also read ACP_HEARTBEAT_INTERVAL_SEC (default 10).

Verification: Run 'cargo test -p engine --lib scheduler' to confirm no regressions.

Constraints:
- The spawned thread must survive panics — that's the whole point of HA-1
- Use AssertUnwindSafe to make the tick call unwind-safe
- Heartbeat writes go through the existing Mutex<Connection> — no contention issues
- Keep the existing graceful shutdown (running flag + Drop impl) intact
- The backoff is per-panic, not cumulative across the session
`,
  { label: 'ha1-scheduler', phase: 'HA-1 Scheduler Resilience', model: 'opus' }
);

// Wave 3: Health endpoint + metrics updates for HA-1
const ha1Health = await agent(
  `Update health endpoint and metrics for HA-1 persistent heartbeat.

Files to read first:
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/http_server/handlers/health.rs (full file)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/http_server/handlers/operations.rs (full file)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/http_server/state.rs (understand AxumApiState)

Tasks:
1. Modify engine/src/http_server/handlers/health.rs:
   - In api_health, when reading scheduler liveness, add a fallback to persisted heartbeat:
     * First try the in-memory last_tick_at from scheduler.status() (existing behavior)
     * If in-memory is None (scheduler just started or process restarted), read persisted heartbeat from store.read_heartbeat()
     * If persisted heartbeat exists and last_heartbeat_at is non-empty, parse it and check staleness (30s threshold)
     * If persisted heartbeat is stale or missing, mark scheduler as "stale"
   - The checks.scheduler object should include a "persisted": true/false field indicating which source was used

2. Modify engine/src/http_server/handlers/operations.rs:
   - In api_metrics, add scheduler_panic_count and scheduler_restart_count to the response
   - Read these from state.scheduler via the Mutex<WorkflowScheduler> lock and the status() JSON

Verification: Run 'cargo test -p engine --test test_http_server' to confirm no regressions.

Constraints:
- Preserve the existing response shape — only ADD new fields
- The health check must work even if the scheduler is not running (handle Mutex lock failure gracefully)
- Keep the healthy/degraded/unhealthy aggregation logic intact
`,
  { label: 'ha1-health', phase: 'HA-1 Scheduler Resilience', model: 'opus' }
);

// Barrier: wait for all HA-1 waves
await parallel([() => Promise.resolve(ha1Storage), () => Promise.resolve(ha1Scheduler), () => Promise.resolve(ha1Health)]);

// ─── HA-2: Automated Backup + Retention ───
phase('HA-2 Automated Backup');

const ha2Backup = await agent(
  `Implement HA-2 automated backup with retention policy.

Files to read first:
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/storage/backup_manager.rs (full file — understand create_backup, delete_backup, list_backups, BackupRecord)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/scheduler.rs (full file — understand the scheduler loop where backup timer will be added)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/main.rs (lines 130-175 — understand how scheduler is started, what state it receives)

Tasks:
1. Modify engine/src/storage/backup_manager.rs:
   a) Add prune_backups(&self, retain_count: usize) -> Result<Vec<String>, String>:
      - Call self.list_backups() to get all BackupRecord entries
      - Sort by created_at descending (newest first)
      - For each backup beyond retain_count, call self.delete_backup(&record.id)
      - Return Vec of deleted backup IDs
   
   b) Add backup_stats(&self) -> serde_json::Value:
      - Call self.list_backups()
      - Return JSON: {"count": N, "total_size_bytes": N, "oldest_created_at": "...", "newest_created_at": "..."}

2. Modify engine/src/scheduler.rs:
   a) Add auto-backup fields to WorkflowScheduler or pass them into the thread closure:
      - backup_manager: Option<Arc<BackupManager>>
      - backup_interval_sec: u64 (from ACP_BACKUP_INTERVAL_SEC, default 0 = disabled)
      - backup_retain_count: usize (from ACP_BACKUP_RETAIN_COUNT, default 5)
      - db_path: String (for WAL checkpoint)
   
   b) In the spawned thread loop, after tick result handling and before sleep:
      - Track last_backup_time as Instant
      - If backup_interval_sec > 0 && elapsed >= backup_interval_sec:
        * Run store.checkpoint_wal() (WAL checkpoint before backup)
        * Call backup_manager.create_backup("auto", Some("scheduled backup"))
        * Call backup_manager.prune_backups(backup_retain_count)
        * Log backup result
        * Reset last_backup_time
   
   c) Add a builder method or extend new() to accept backup configuration:
      with_auto_backup(backup_manager: Arc<BackupManager>, db_path: String, interval_sec: u64, retain_count: usize)

3. Modify engine/src/main.rs:
   - Read ACP_BACKUP_INTERVAL_SEC and ACP_BACKUP_RETAIN_COUNT from env
   - If ACP_BACKUP_INTERVAL_SEC > 0, pass BackupManager and config to the scheduler via with_auto_backup()
   - Log: "[acp-startup] auto_backup=enabled interval={interval}s retain={count}" or "auto_backup=disabled"

4. Modify engine/src/http_server/handlers/operations.rs:
   - In api_metrics, add backup_auto_enabled (bool), backup_interval_sec, backup_retain_count fields

Verification: Run 'cargo test -p engine' to confirm no regressions.

Constraints:
- Backup I/O happens in the scheduler thread, not the HTTP handler thread
- WAL checkpoint MUST happen before file copy to ensure consistent snapshot
- Retention pruning happens after each backup, not on a separate timer
- If backup_interval_sec is 0, no backup code runs (zero overhead when disabled)
- Use existing BackupManager methods — do not reimplement file copy logic
`,
  { label: 'ha2-backup', phase: 'HA-2 Automated Backup', model: 'opus' }
);

// ─── HA-3: Deep Health + Resource Monitoring ───
phase('HA-3 Deep Health');

const ha3ResourceMonitor = await agent(
  `Implement HA-3 resource monitoring module and deep health checks.

Files to read first:
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/http_server/handlers/health.rs (full file — current health check)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/http_server/handlers/operations.rs (full file — metrics endpoint)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/infrastructure/mod.rs (understand module structure)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/storage/backup_manager.rs (understand list_backups)

Tasks:
1. Create NEW FILE: engine/src/infrastructure/resource_monitor.rs
   Define structs and functions:
   - DiskUsage { free_bytes: u64, total_bytes: u64, usage_pct: f64 }
   - MemoryUsage { available_bytes: u64, total_bytes: u64, usage_pct: f64 }
   - disk_usage(mount_path: &str) -> Result<DiskUsage, String>
     Use libc::statvfs via FFI. The libc crate is already available transitively.
     Calculate: total = blocks * block_size, free = bavail * block_size, usage_pct = (1 - free/total) * 100
   - memory_usage() -> Result<MemoryUsage, String>
     Read /proc/meminfo, parse MemTotal and MemAvailable lines (values in kB, multiply by 1024)
     Calculate: usage_pct = (1 - available/total) * 100
   - db_file_size(db_path: &Path) -> Result<u64, String>
     Use std::fs::metadata(db_path).map(|m| m.len())

2. Modify engine/src/infrastructure/mod.rs:
   - Add 'pub mod resource_monitor;'

3. Modify engine/Cargo.toml:
   - Add 'libc = "0.2" to [dependencies] if not already present (check first)

4. Modify engine/src/http_server/handlers/health.rs — completely rewrite api_health:
   - Define a HealthReport struct (or use serde_json::json! directly):
     {
       "status": "healthy|degraded|unhealthy",
       "checks": {
         "db": {"status": "ok|error", "integrity": "ok|error", "file_size_bytes": N},
         "disk": {"status": "ok|degraded", "free_bytes": N, "total_bytes": N, "usage_pct": N},
         "memory": {"status": "ok|degraded", "available_bytes": N, "total_bytes": N, "usage_pct": N},
         "scheduler": {"status": "ok|stale", "last_heartbeat_at": "...", "persisted": bool},
         "backup": {"status": "ok|degraded|unavailable", "last_backup_at": "...", "age_seconds": N}
       }
     }
   
   - Run all checks:
     * DB: store.check_integrity() — existing
     * DB file size: resource_monitor::db_file_size() if store has a db_path
     * Disk: resource_monitor::disk_usage("/") — use root mount
     * Memory: resource_monitor::memory_usage()
     * Scheduler: existing heartbeat logic (in-memory + persisted fallback from HA-1)
     * Backup: read from backup_manager.list_backups() if available, compute age_seconds
   
   - Read thresholds from env: ACP_DISK_WARN_PCT (default 10.0), ACP_MEM_WARN_PCT (default 10.0)
   
   - Aggregation:
     * unhealthy: db status is "error"
     * degraded: any check is "degraded" (disk < 10% free, memory < 10% available, scheduler stale, backup stale)
     * healthy: all checks pass
   
   - Handle errors gracefully: if disk_usage or memory_usage fails, include {"status": "unknown", "error": "..."} rather than failing the whole health check

5. Optional: Add ACP_HEALTH_ALERT_WEBHOOK_URL support
   - Read env var at handler level
   - If set AND health status changed to degraded/unhealthy, spawn a tokio task to POST the health report to the webhook URL with a 2-second timeout
   - Fire-and-forget — do not block the health response

Verification: Run 'cargo test -p engine' and 'cargo clippy -p engine --all-targets -- -D warnings'.

Constraints:
- Resource monitoring must not panic on unsupported platforms — return graceful errors
- /proc/meminfo parsing is Linux-specific — that's fine, this project targets Linux
- The health endpoint must remain fast — resource checks are cheap syscalls
- Preserve backward compatibility: existing 'db' and 'scheduler' check keys must still exist
- Use serde_json::json! macro for building the response rather than defining many structs
`,
  { label: 'ha3-health', phase: 'HA-3 Deep Health', model: 'opus' }
);

// ─── HA-5 + HA-6: TLS and Encryption in parallel ───
phase('HA-5 TLS + HA-6 Encryption');

const [ha5Result, ha6Result] = await parallel([
  () => agent(
    `Implement HA-5 TLS inbound support.

Files to read first:
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/Cargo.toml (full file — understand current dependencies)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/main.rs (full file — understand server startup, how axum::serve is used, graceful shutdown)

Tasks:
1. Modify engine/Cargo.toml:
   - Add: axum-server = { version = "0.7", features = ["tls-rustls"] }
   - Add: rustls-pemfile = "2"

2. Modify engine/src/main.rs:
   After building the router (after build_axum_router or build_axum_router_with_dashboard), add TLS detection:
   
   let tls_cert_path = std::env::var("ACP_TLS_CERT_PATH").ok();
   let tls_key_path = std::env::var("ACP_TLS_KEY_PATH").ok();
   
   match (tls_cert_path, tls_key_path) {
     (Some(cert_path), Some(key_path)) => {
       // Load cert chain
       let cert_file = std::fs::File::open(&cert_path)
         .map_err(|e| format!("Failed to open TLS cert {}: {}", cert_path, e))?;
       let mut cert_reader = std::io::BufReader::new(cert_file);
       let certs = rustls_pemfile::certs(&mut cert_reader)
         .collect::<Result<Vec<_>, _>>()
         .map_err(|e| format!("Failed to parse TLS certs: {}", e))?;
       
       // Load private key
       let key_file = std::fs::File::open(&key_path)
         .map_err(|e| format!("Failed to open TLS key {}: {}", key_path, e))?;
       let mut key_reader = std::io::BufReader::new(key_file);
       let key = rustls_pemfile::private_key(&mut key_reader)
         .map_err(|e| format!("Failed to parse TLS key: {}", e))?
         .ok_or("No private key found in TLS key file")?;
       
       // Build rustls config
       let tls_config = rustls::ServerConfig::builder()
         .with_no_client_auth()
         .with_single_cert(certs, key)
         .map_err(|e| format!("Failed to build TLS config: {}", e))?;
       
       // Use axum_server for TLS
       println!("[acp-startup] TLS enabled, cert={}, key={}", cert_path, key_path);
       let addr = format!("{}:{}", host, port).parse().unwrap();
       axum_server::bind_rustls(addr, tls_config)
         .serve(router.into_make_service())
         .await
         .map_err(|e| format!("TLS server error: {}", e))?;
     }
     _ => {
       // Existing plain TCP path
       println!("[acp-startup] TLS disabled (set ACP_TLS_CERT_PATH and ACP_TLS_KEY_PATH to enable)");
       let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
       axum::serve(listener, router)
         .with_graceful_shutdown(shutdown_signal())
         .await
         .unwrap();
     }
   }

   IMPORTANT: The TLS path also needs graceful_shutdown support. Use axum_server::Handle:
   let handle = axum_server::Handle::new();
   tokio::spawn({
     let handle = handle.clone();
     async move {
       shutdown_signal().await;
       handle.graceful_shutdown(Some(std::time::Duration::from_secs(10)));
     }
   });
   axum_server::bind_rustls(addr, tls_config)
     .handle(handle)
     .serve(router.into_make_service())
     .await?;

3. Update .env.example to document the new env vars:
   - ACP_TLS_CERT_PATH — path to PEM certificate chain file
   - ACP_TLS_KEY_PATH — path to PEM private key file

Verification: Run 'cargo check -p engine' to confirm compilation. Run 'cargo test -p engine' for regressions. Full TLS integration tests require actual cert files — add a test that generates self-signed certs with rcgen dev-dep if time permits.

Constraints:
- TLS is optional — plain TCP fallback must work exactly as before when env vars are not set
- Fail fast at startup if cert/key files are specified but invalid
- Log TLS status at startup
- Preserve graceful shutdown in both TLS and plain paths
- The axum_server crate handles HTTP/1.1 and HTTP/2 over TLS automatically
`,
    { label: 'ha5-tls', phase: 'HA-5 TLS + HA-6 Encryption', model: 'opus' }
  ),
  () => agent(
    `Implement HA-6 SQLite encryption at rest.

Files to read first:
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/Cargo.toml (full file — understand rusqlite dependency)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/storage/local_product_store/mod.rs (lines 336-362 — understand Connection::open and pragma setup)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/storage/backup_manager.rs (full file — understand BackupRecord and create_backup)
- /home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/main.rs (lines 80-130 — understand how LocalProductStore is created)

Tasks:
1. Modify engine/Cargo.toml:
   - Change rusqlite features from ["bundled"] to ["bundled-sqlcipher"]
   - This is the ONLY crate change needed — bundled-sqlcipher is a superset of bundled

2. Modify engine/src/storage/local_product_store/mod.rs:
   a) Add a new constructor that accepts an optional encryption key:
      pub fn new_with_encryption(
        path: impl AsRef<Path>,
        clock: impl Fn() -> String + Send + Sync + 'static,
        encryption_key: Option<&str>,
      ) -> Result<Self, String>
   
   b) In the new constructor, after Connection::open:
      if let Some(key) = encryption_key {
        conn.execute_batch(&format!("PRAGMA key = '{}';", key.replace('\'', "''")))
          .map_err(|e| format!("Failed to set encryption key: {}", e))?;
      }
   
   c) Modify the existing new(path) method to read ACP_DB_ENCRYPTION_KEY from env:
      pub fn new(path: impl AsRef<Path>) -> Result<Self, String> {
        let key = std::env::var("ACP_DB_ENCRYPTION_KEY").ok();
        Self::new_with_encryption(path, || chrono::Utc::now().to_rfc3339(), key.as_deref())
      }
   
   d) Keep new_with_clock() as a backward-compatible wrapper:
      pub fn new_with_clock(path: impl AsRef<Path>, clock: impl Fn() -> String + Send + Sync + 'static) -> Result<Self, String> {
        let key = std::env::var("ACP_DB_ENCRYPTION_KEY").ok();
        Self::new_with_encryption(path, clock, key.as_deref())
      }
   
   e) Add a method to check if encryption is active:
      pub fn is_encrypted(&self) -> bool { self.encryption_active }
      Store this as a field: encryption_active: bool

3. Modify engine/src/storage/backup_manager.rs:
   a) Add encryption_key_hash: Option<String> to BackupRecord
   
   b) Modify create_backup to accept an optional encryption_key: Option<&str>:
      - If encryption_key is Some, compute SHA-256 hash and store in encryption_key_hash
      - If None, encryption_key_hash stays None
   
   c) Add a new method:
      verify_encryption_key(&self, backup_id: &str, current_key: Option<&str>) -> Result<bool, String>
      - Load backup metadata, find the record
      - If record.encryption_key_hash is None, return true (unencrypted backup)
      - If current_key is None but hash exists, return false
      - Compute SHA-256 of current_key and compare to stored hash

4. Modify engine/src/main.rs:
   - After creating the store, log encryption status:
     if store.is_encrypted() {
       println!("[acp-startup] db_encryption=enabled");
     } else {
       println!("[acp-startup] db_encryption=disabled (set ACP_DB_ENCRYPTION_KEY to enable)");
     }

5. Update .env.example:
   - ACP_DB_ENCRYPTION_KEY — SQLCipher encryption passphrase for SQLite database

Verification: Run 'cargo test -p engine' to confirm no regressions. The bundled-sqlcipher feature is backward-compatible with unencrypted databases.

Constraints:
- SQLCipher PRAGMA key must be set BEFORE any other queries on the connection
- Existing unencrypted databases continue to work when no key is set
- The encryption key hash in backup metadata uses SHA-256 (same crate already in deps: sha2)
- Do NOT store the actual encryption key in backup metadata — only the hash
- The PRAGMA key format for SQLCipher is: PRAGMA key = 'passphrase';
- For raw hex key: PRAGMA key = "x'hex...'"; but passphrase form is simpler for this use case
`,
    { label: 'ha6-encryption', phase: 'HA-5 TLS + HA-6 Encryption', model: 'opus' }
  ),
]);

// ─── Verify ───
phase('Verify');

const verifyResult = await agent(
  `Run the full verification suite for the HA Hardening Track implementation.

Tasks:
1. Run: cargo fmt --check -p engine
2. Run: cargo clippy -p engine --all-targets -- -D warnings
3. Run: cargo test -p engine (this runs ALL tests — report total count and any failures)
4. Run: bash scripts/check_wire_codegen_drift.sh
5. Run: uv run --no-project python scripts/check_agent_handoff.py

Report:
- Total test count (should be 1367 + new tests from HA-1 through HA-6)
- Any test failures (with file name and error message)
- Any clippy warnings
- Any fmt issues
- Handoff guard pass/fail

If any step fails, report the exact error so it can be fixed.
`,
  { label: 'ha-verify', phase: 'Verify', model: 'sonnet' }
);

log('HA Hardening Track implementation complete. All 5 remaining phases (HA-1, HA-2, HA-3, HA-5, HA-6) implemented.');
