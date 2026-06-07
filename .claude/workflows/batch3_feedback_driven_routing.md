export const meta = {
  name: 'batch3-feedback-driven-routing',
  description: 'Persist scheduler feedback across ticks with real executor_type, task_group, success/failure, latency, retry_count, quality, cost',
  phases: [
    { title: 'Research', detail: 'Explore existing routing/feedback modules and scheduler tick path' },
    { title: 'Implement', detail: 'Write FeedbackStore and tests' },
    { title: 'Verify', detail: 'Run tests, clippy, handoff check' },
  ],
}

phase('Research')

const researchResult = await agent(
  'Read these files and summarize the key types, methods, and gaps for Batch 3 Feedback-Driven Routing:\n' +
  '1. engine/src/routing/mod.rs - module structure\n' +
  '2. engine/src/routing/feedback_integrator.rs - FeedbackIntegrator, record_outcome, should_adapt\n' +
  '3. engine/src/routing/history_store.rs - RoutingHistoryStore\n' +
  '4. engine/src/routing/schemas.rs - RoutingObservation schema\n' +
  '5. engine/src/scheduler.rs - tick loop, how feedback is recorded after node execution\n' +
  '6. engine/src/storage/local_product_store/workflow_runs.rs - tick_with_executor_and_command result recording\n' +
  '7. engine/src/storage/local_product_store/mod.rs - DDL and existing tables\n\n' +
  'Output:\n' +
  '- What RoutingObservation currently stores vs what Batch 3 needs\n' +
  '- How scheduler currently records feedback (inline vs persistent)\n' +
  '- What new SQLite table/columns are needed\n' +
  '- What new storage methods are needed\n' +
  '- How DynamicWorkflowController can use persisted feedback for routing decisions',
  { label: 'research', phase: 'Research' }
)

log('Research: ' + (researchResult || '').substring(0, 200))

phase('Implement')

const feedbackImpl = await agent(
  'Implement Batch 3: Feedback-Driven Routing in engine/src/routing/feedback_store.rs.\n\n' +
  'The goal: persist scheduler feedback across ticks so later nodes can select executor/model tier from history.\n\n' +
  'New SQLite table in LocalProductStore DDL (add to mod.rs):\n' +
  '  scheduler_feedback (\n' +
  '    feedback_id TEXT PRIMARY KEY,\n' +
  '    run_id TEXT NOT NULL,\n' +
  '    node_id TEXT,\n' +
  '    executor_type TEXT NOT NULL,\n' +
  '    task_group TEXT NOT NULL,\n' +
  '    task_domain TEXT NOT NULL,\n' +
  '    task_intent TEXT NOT NULL,\n' +
  '    success INTEGER NOT NULL DEFAULT 0,\n' +
  '    latency_ms INTEGER NOT NULL DEFAULT 0,\n' +
  '    retry_count INTEGER NOT NULL DEFAULT 0,\n' +
  '    quality_score REAL NOT NULL DEFAULT 0.0,\n' +
  '    cost REAL NOT NULL DEFAULT 0.0,\n' +
  '    error_domain TEXT,\n' +
  '    created_at TEXT NOT NULL\n' +
  '  )\n\n' +
  'New file: engine/src/routing/feedback_store.rs\n' +
  '- FeedbackRecord struct with all fields above\n' +
  '- FeedbackStoreStats: total_records, success_rate, avg_latency_ms, avg_quality, avg_cost, by_executor_type breakdown\n' +
  '- LocalProductStore methods:\n' +
  '  - insert_scheduler_feedback(run_id, node_id, executor_type, task_group, success, latency_ms, retry_count, quality_score, cost, error_domain) -> FeedbackRecord\n' +
  '  - get_feedback_for_run(run_id) -> Vec<FeedbackRecord>\n' +
  '  - get_feedback_for_task_group(task_group, limit) -> Vec<FeedbackRecord>\n' +
  '  - get_feedback_stats(task_group) -> FeedbackStoreStats\n' +
  '  - suggest_executor_type(task_group) -> Option<String> (returns executor_type with highest success rate from history)\n\n' +
  'Integration with DynamicWorkflowController (engine/src/workflow/dynamic_controller.rs):\n' +
  '- After tick_with_executor_and_command returns, record feedback via insert_scheduler_feedback\n' +
  '- Before ticking, optionally call suggest_executor_type to pick best executor\n' +
  '- Add record_feedback flag to DynamicControllerConfig (default true)\n\n' +
  'Integration with tick_with_executor_and_command (engine/src/storage/local_product_store/workflow_runs.rs):\n' +
  '- After Phase 3 records node result, also call insert_scheduler_feedback with:\n' +
  '  executor_type from output, task_group from node task_type, success from final_status, latency_ms from output, retry count, quality from result, cost=0.0 (placeholder)\n\n' +
  'New file: engine/src/routing/feedback_store_tests.rs\n' +
  'Required tests:\n' +
  '1. test_insert_and_retrieve_feedback\n' +
  '2. test_feedback_for_run_isolation\n' +
  '3. test_feedback_stats_success_rate\n' +
  '4. test_feedback_stats_by_executor_type\n' +
  '5. test_suggest_executor_type_returns_best\n' +
  '6. test_suggest_executor_type_empty_returns_none\n' +
  '7. test_feedback_recorded_after_tick\n' +
  '8. test_feedback_recorded_on_retry\n' +
  '9. test_controller_uses_suggested_executor\n' +
  '10. test_feedback_persists_across_ticks\n\n' +
  'Add mod declaration in engine/src/routing/mod.rs:\n' +
  '  pub mod feedback_store;\n' +
  '  #[cfg(test)] mod feedback_store_tests;\n\n' +
  'Constraints:\n' +
  '- Do NOT create a parallel scheduler or routing kernel\n' +
  '- Do NOT enable provider execution\n' +
  '- Use existing LocalProductStore::with_conn pattern\n' +
  '- All tests use in-memory store',
  { label: 'feedback-impl', phase: 'Implement', model: 'opus' }
)

log('Feedback impl: ' + (feedbackImpl || '').substring(0, 200))

phase('Verify')

const verifyResult = await agent(
  'Run these verification commands and report results:\n\n' +
  '1. cargo test -p engine --lib routing::feedback_store 2>&1 | tail -30\n' +
  '2. cargo test -p engine 2>&1 | tail -5\n' +
  '3. cargo clippy -p engine -- -D warnings 2>&1 | tail -20\n' +
  '4. uv run --no-project python scripts/check_agent_handoff.py 2>&1 | tail -20\n\n' +
  'If any test fails, fix and re-run. If clippy warns, fix.\n' +
  'Report: test count, pass/fail, clippy status, handoff status.',
  { label: 'verify', phase: 'Verify' }
)

log('Verify: ' + (verifyResult || '').substring(0, 300))

return {
  batch: 'Batch 3: Feedback-Driven Routing',
  implementation: feedbackImpl ? 'done' : 'failed',
  verification: verifyResult || 'unknown',
}
