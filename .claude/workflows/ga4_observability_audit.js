export const meta = {
  name: 'ga4-observability-audit',
  description: 'GA-4: Observability/Audit — enriched metrics, operation audit events, scheduler diagnostics, runbook',
  phases: [
    { title: 'Enrich Metrics', detail: 'Add artifact_count, secret_block_count, queue_length to metrics endpoint' },
    { title: 'Audit Events', detail: 'Add explicit audit events for CLI tick, capture, export, cleanup, quarantine at HTTP handler level' },
    { title: 'Scheduler Diagnostics', detail: 'Expose retry_count and total_execution_time_ms in scheduler status' },
    { title: 'Runbook', detail: 'Add operational runbook to existing ops docs' },
    { title: 'Tests', detail: 'Write Rust tests for all new metrics and audit events' },
  ],
};

const PROJ = '/home/igzela/Projects/token-efficient-agent-harness-lab';

// Phase 1: Enrich the /api/v1/metrics endpoint with new fields
await phase('Enrich Metrics');

// 1a. Add artifact_count and secret_block_count to stats()
await agent(
  `Enrich the stats() method in ${PROJ}/engine/src/storage/local_product_store/mod.rs to also include:
  - "supervised_patch_artifact_count": count from supervised_patch_artifacts table
  - "secret_block_count": count of artifacts where details_json contains "secret_scan_status":"blocked"
  - "queue_length": count of workflow_run_nodes with status='pending'

  The stats() method is at line 304. Add these 3 new keys to the existing json!{} return value. Use the existing count_table helper for the first one. For secret_block_count, use a custom SQL query. For queue_length, use a custom SQL query.

  Do NOT change any other methods. Only modify the stats() method.`,
  { label: 'enrich-stats', phase: 'Enrich Metrics', model: 'opus' }
);

// 1b. Add the new fields to /api/v1/metrics handler
await agent(
  `Update the api_metrics handler in ${PROJ}/engine/src/http_server/handlers/operations.rs to include the 3 new stats fields:
  - supervised_patch_artifact_count (from stats["supervised_patch_artifact_count"])
  - secret_block_count (from stats["secret_block_count"])
  - queue_length (from stats["queue_length"])

  Add them to the existing json!{} response. Use the same pattern as the existing fields (dispatch_count, plan_count, etc.).`,
  { label: 'enrich-metrics-handler', phase: 'Enrich Metrics', model: 'opus' }
);

// Phase 2: Add explicit audit events at HTTP handler level
await phase('Audit Events');

await agent(
  `Add explicit audit events to the HTTP handlers in ${PROJ}/engine/src/http_server/handlers/supervised_patch.rs.

  The store already logs some events at the storage layer, but we need richer audit at the handler level for operational visibility. Add audit events for:

  1. In api_capture_supervised_patch: after successful capture, log audit event "supervised_patch.capture" with details including artifact_id, workspace_id, changed_files_count, and secret_scan_status from the returned artifact.

  2. In api_export_supervised_patch: after successful export, log audit event "supervised_patch.export" with details including artifact_id, exported_by, export_eligible status.

  3. In api_cleanup_supervised_patch_workspace: after successful cleanup, log audit event "supervised_patch.cleanup" with details including workspace_id.

  4. In api_quarantine_supervised_patch_workspace: after successful quarantine, log audit event "supervised_patch.quarantine" with details including workspace_id.

  The pattern to use: call store.append_audit(actor, action, resource, &details). The actor comes from context.api_key_id. The function signature is: pub fn append_audit(&self, actor: &str, action: &str, resource: &str, details: &Value) -> Result<Value, String>.

  Look at how the storage layer already uses append_audit_locked for the pattern. The handler-level audit should use the public append_audit method on the store.

  IMPORTANT: Do not modify the storage-layer audit calls. This is ADDITIVE audit at the handler level for richer operational visibility.`,
  { label: 'audit-events', phase: 'Audit Events', model: 'opus' }
);

// Phase 3: Scheduler diagnostics
await phase('Scheduler Diagnostics');

await agent(
  `Enrich the scheduler status in ${PROJ}/engine/src/scheduler.rs to include:

  1. "retry_count" - total number of retries observed (track with an AtomicU64, incremented when a node execution fails but will be retried)
  2. "total_execution_time_ms" - cumulative execution time across all ticks (track with an AtomicU64)

  In the scheduler_tick function, measure the elapsed time of each store.tick_with_executor call and add it to total_execution_time_ms. Also, check if the tick result action was "node_retry" and increment retry_count if so.

  These need to be new fields on WorkflowScheduler struct:
  - retry_count: Arc<std::sync::atomic::AtomicU64>
  - total_execution_time_ms: Arc<std::sync::atomic::AtomicU64>

  Initialize them to 0 in new(). Pass them into the spawned thread. Include them in the status() return value.

  In the scheduler_tick function, change its return type to return both the tick count AND any retry/execution-time info. The simplest approach: change scheduler_tick to return a struct or add output params. Or: add the tracking at the WorkflowScheduler level by measuring timing around scheduler_tick calls in the thread loop.

  Recommended approach: In the spawned thread loop in start(), measure the time of scheduler_tick and add to total_execution_time_ms. For retry_count, change scheduler_tick to return a TickResult struct with ticks and retries fields.`,
  { label: 'scheduler-diagnostics', phase: 'Scheduler Diagnostics', model: 'opus' }
);

// Phase 4: Runbook
await phase('Runbook');

await agent(
  `Add an operational runbook section to ${PROJ}/docs/DATA_DIRECTORY.md. Add a new section "## Operational Runbook" after the existing content. Include:

  ### Health Checks
  - GET /api/v1/health — basic liveness
  - GET /api/v1/ready — readiness with store connectivity
  - GET /api/v1/metrics — operational metrics (dispatch count, artifact count, secret block count, queue length, costs)
  - GET /api/v1/scheduler/status — scheduler state (running, tick_count, error_count, active_runs, retry_count, total_execution_time_ms)
  - GET /api/v1/storage/integrity — SQLite integrity check

  ### Key Metrics to Monitor
  - queue_length > 0 with active_runs = 0: scheduler may be stuck
  - error_count rising: check last_error in scheduler status
  - secret_block_count > 0: review blocked artifacts for credential leaks
  - retry_count rising: executor instability or task complexity issues
  - pricing_configured = false with provider_enabled = true: missing cost tracking

  ### Audit Events
  - workflow_run.create / workflow_run.completed / workflow_run.failed — run lifecycle
  - supervised_patch.capture — artifact capture with secret scan results
  - supervised_patch.export — artifact export with approval binding
  - supervised_patch.cleanup / supervised_patch.quarantine — workspace lifecycle
  - supervised_patch.workspace_status_update — state transitions

  ### Backup & Recovery
  - POST /api/v1/backups with confirm_local_backup=true
  - GET /api/v1/backups/:id/verify — checksum verification
  - POST /api/v1/backups/:id/restore with confirm_restore=true
  - scripts/acp_restore_smoke.py for DR rehearsal

  Keep it concise and actionable. No new files.`,
  { label: 'runbook', phase: 'Runbook', model: 'sonnet' }
);

// Phase 5: Tests
await phase('Tests');

await agent(
  `Write Rust tests for the GA-4 changes in the appropriate test files.

  1. In ${PROJ}/engine/tests/test_http_server.rs, add tests:
  - axum_metrics_includes_artifact_count: POST a workspace and capture, verify artifact_count >= 1 in metrics
  - axum_metrics_includes_queue_length: create a plan and run, verify queue_length >= 0 in metrics
  - axum_metrics_includes_secret_block_count: verify secret_block_count is present in metrics

  2. In ${PROJ}/engine/tests/test_http_server.rs, add audit event tests:
  - axum_capture_logs_audit_event: POST workspace capture, then GET /api/v1/audit and verify a "supervised_patch.capture" action exists
  - axum_cleanup_logs_audit_event: POST workspace cleanup, then verify audit event
  - axum_quarantine_logs_audit_event: POST workspace quarantine, then verify audit event

  3. In ${PROJ}/engine/src/scheduler.rs (inline tests), add:
  - scheduler_status_includes_retry_count: verify retry_count field exists and is 0 initially
  - scheduler_status_includes_execution_time: verify total_execution_time_ms field exists

  Follow the existing test patterns. Use tempdir(), LocalProductStore::new(), build_axum_router(), app.clone().oneshot().await for HTTP tests. Use test_store() for scheduler inline tests.

  The existing test patterns are well established — follow the same style.`,
  { label: 'ga4-tests', phase: 'Tests', model: 'opus' }
);
