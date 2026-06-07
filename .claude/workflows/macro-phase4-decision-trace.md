export const meta = {
  name: 'macro-phase4-decision-trace',
  description: 'Macro-Orchestrator Phase 4: Decision Trace / Explainability',
  phases: [
    { title: 'Wave 1', detail: 'Storage + get_decision_by_id + tests' },
    { title: 'Wave 2', detail: 'HTTP endpoints + OpenAPI' },
    { title: 'Wave 3', detail: 'SDKs + Dashboard' },
    { title: 'Wave 4', detail: 'Verification' },
  ],
}

// Wave 1: Storage Layer
phase('Wave 1')
const storageResult = await agent(
  'You are implementing Wave 1 of Macro-Orchestrator Phase 4: Decision Trace.\n\n' +
  'PROJECT: /home/igzela/Projects/token-efficient-agent-harness-lab\n\n' +
  'TASK: Add get_decision_by_id method + 12 unit tests to engine/src/storage/local_product_store/decisions.rs\n\n' +
  'Read the file first. Then:\n\n' +
  '1. Add this method to the impl LocalProductStore block (after decision_log_stats):\n\n' +
  'pub fn get_decision_by_id(&self, decision_id: &str) -> Result<Option<DecisionRecord>, String> {\n' +
  '    self.with_conn(|conn| {\n' +
  '        let mut stmt = conn\n' +
  '            .prepare(\n' +
  '                "SELECT decision_id, run_id, node_id, action, action_reason,\n' +
  '                        selected_executor, blocked_reason, confidence, confidence_score,\n' +
  '                        input_signals_json, created_at\n' +
  '                 FROM orchestration_decisions\n' +
  '                 WHERE decision_id = ?1",\n' +
  '            )\n' +
  '            .map_err(|e| e.to_string())?;\n' +
  '        let mut rows = stmt\n' +
  '            .query_map(params![decision_id], decision_row)\n' +
  '            .map_err(|e| e.to_string())?;\n' +
  '        match rows.next() {\n' +
  '            Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),\n' +
  '            None => Ok(None),\n' +
  '        }\n' +
  '    })\n' +
  '}\n\n' +
  '2. Add a #[cfg(test)] mod tests block at the end of the file with these 12 tests:\n' +
  '- test_record_decision_returns_valid_record\n' +
  '- test_record_decision_persists_to_store\n' +
  '- test_get_decisions_for_run_empty\n' +
  '- test_get_decisions_for_run_ordering\n' +
  '- test_get_decisions_for_run_respects_limit\n' +
  '- test_get_decision_by_id_found\n' +
  '- test_get_decision_by_id_not_found\n' +
  '- test_search_decisions_no_filter\n' +
  '- test_search_decisions_by_action\n' +
  '- test_search_decisions_by_executor\n' +
  '- test_decision_log_stats_empty\n' +
  '- test_decision_log_stats_with_data\n\n' +
  'Use LocalProductStore::new(":memory:") for all tests. Use serde_json::json.\n' +
  'Helper fn test_store() -> LocalProductStore and record_test_decision(store, run_id, action, executor) -> DecisionRecord.\n\n' +
  '3. Verify: cd engine && cargo test -p engine --lib decisions -- --nocapture\n' +
  '4. Also run: cargo test -p engine 2>&1 | tail -5 to get total count\n\n' +
  'WRITE: engine/src/storage/local_product_store/decisions.rs ONLY',
  { label: 'wave1-storage', phase: 'Wave 1', model: 'opus' }
)

// Wave 2: HTTP Endpoints + OpenAPI
phase('Wave 2')
const httpResult = await agent(
  'You are implementing Wave 2 of Macro-Orchestrator Phase 4: HTTP Endpoints + OpenAPI.\n\n' +
  'PROJECT: /home/igzela/Projects/token-efficient-agent-harness-lab\n\n' +
  'TASK: Create decisions HTTP handler, register routes, add OpenAPI paths + tests.\n\n' +
  'Read these reference files first:\n' +
  '- engine/src/http_server/handlers/queue.rs (handler pattern)\n' +
  '- engine/src/http_server/handlers/mod.rs (module registration)\n' +
  '- engine/src/http_server/routes.rs (route registration)\n' +
  '- engine/src/http_server/mod.rs (OpenAPI document)\n\n' +
  'STEPS:\n\n' +
  '1. Create NEW file: engine/src/http_server/handlers/decisions.rs\n' +
  '   Three handler functions following queue.rs pattern:\n' +
  '   - api_decisions: GET /api/v1/decisions (query: limit, offset, search, run_id)\n' +
  '   - api_decision_detail: GET /api/v1/decisions/:decision_id\n' +
  '   - api_decision_stats: GET /api/v1/decisions/stats\n' +
  '   Use authorize with dispatch:read scope. Use require_store. Use internal_error.\n' +
  '   Response format: {"schema_version": AXUM_API_SCHEMA_VERSION, "decisions": [...], "stats": {...}}\n\n' +
  '2. Add pub(crate) mod decisions; to engine/src/http_server/handlers/mod.rs\n\n' +
  '3. Add routes to axum_routes() in engine/src/http_server/routes.rs:\n' +
  '   IMPORTANT: Register /api/v1/decisions/stats BEFORE /api/v1/decisions/:decision_id\n\n' +
  '4. Add OpenAPI paths in engine/src/http_server/mod.rs openapi_document():\n' +
  '   /api/v1/decisions, /api/v1/decisions/stats, /api/v1/decisions/{decision_id}\n\n' +
  '5. Add test_openapi_decisions_routes_documented in the tests module of mod.rs\n\n' +
  '6. Verify:\n' +
  '   cd engine && cargo test -p engine --test test_http_server -- --nocapture\n' +
  '   cd engine && cargo fmt --check\n' +
  '   cd engine && cargo clippy -p engine --all-targets -- -D warnings\n\n' +
  'WRITE: engine/src/http_server/handlers/decisions.rs (NEW), handlers/mod.rs, routes.rs, mod.rs',
  { label: 'wave2-http', phase: 'Wave 2', model: 'opus' }
)

// Wave 3: SDKs + Dashboard
phase('Wave 3')
const sdkResult = await agent(
  'You are implementing Wave 3 of Macro-Orchestrator Phase 4: SDKs + Dashboard.\n\n' +
  'PROJECT: /home/igzela/Projects/token-efficient-agent-harness-lab\n\n' +
  'TASK: Add TypeScript SDK types+methods, Python SDK methods+tests, dashboard types+API client+components+tab.\n\n' +
  'Read these reference files first:\n' +
  '- sdk/typescript/src/api-types.ts (TypeScript types pattern)\n' +
  '- sdk/typescript/src/index.ts (TypeScript client pattern)\n' +
  '- sdk/python/src/agent_control_plane_sdk/client.py (Python client pattern)\n' +
  '- sdk/python/tests/test_client.py (Python test pattern)\n' +
  '- dashboard/src/lib/types.ts (dashboard types)\n' +
  '- dashboard/src/lib/api-client.ts (dashboard API client)\n' +
  '- dashboard/src/components/ExecutorPool.tsx (component pattern)\n' +
  '- dashboard/src/app/page.tsx (tab integration)\n\n' +
  'STEPS:\n\n' +
  '1. Add to sdk/typescript/src/api-types.ts (at end):\n' +
  '   DecisionRecord, DecisionLogStats, DecisionListResponse, DecisionDetailResponse, DecisionStatsResponse interfaces\n\n' +
  '2. Add 3 methods to AgentControlPlaneClient in sdk/typescript/src/index.ts:\n' +
  '   decisions(options), decisionDetail(decisionId), decisionStats()\n' +
  '   Import the new types from api-types\n\n' +
  '3. Add 3 methods to AgentControlPlaneClient in sdk/python/src/agent_control_plane_sdk/client.py:\n' +
  '   decisions(limit, offset, search, run_id), decision_detail(decision_id), decision_stats()\n\n' +
  '4. Add 4 tests to sdk/python/tests/test_client.py:\n' +
  '   ClientDecisionsTest (2 tests), ClientDecisionDetailTest (1 test), ClientDecisionStatsTest (1 test)\n' +
  '   Follow the exact mock pattern of existing test classes\n\n' +
  '5. Add to dashboard/src/lib/types.ts:\n' +
  '   DecisionRecord, DecisionLogStats, DecisionListResponse, DecisionDetailResponse, DecisionStatsResponse\n\n' +
  '6. Add to dashboard/src/lib/api-client.ts:\n' +
  '   fetchDecisions(params), fetchDecisionDetail(decisionId), fetchDecisionStats()\n' +
  '   Import the new types\n\n' +
  '7. Create NEW: dashboard/src/components/DecisionLog.tsx\n' +
  '   Cross-run decision history. Follow ExecutorPool.tsx pattern exactly.\n' +
  '   Summary tiles, search, table with action/confidence pills, error/empty states.\n\n' +
  '8. Create NEW: dashboard/src/components/DecisionTrace.tsx\n' +
  '   Per-run decision chain. Props: runId. Timeline view with colored borders by confidence.\n' +
  '   Each entry: action badge, reason, executor, confidence pill, collapsible input_signals.\n\n' +
  '9. Integrate into dashboard/src/app/page.tsx:\n' +
  '   Add "decisions" to Tab type and tabs array. Import DecisionLog. Render it.\n\n' +
  '10. Verify:\n' +
  '    cd sdk/typescript && bun run build\n' +
  '    cd sdk/python && python -m pytest tests/ -v\n' +
  '    cd dashboard && bun run typecheck\n' +
  '    cd dashboard && bun run build\n\n' +
  'WRITE: sdk/typescript/src/api-types.ts, sdk/typescript/src/index.ts, sdk/python client+tests, ' +
  'dashboard types+api-client+DecisionLog.tsx+DecisionTrace.tsx+page.tsx',
  { label: 'wave3-sdks-dashboard', phase: 'Wave 3', model: 'opus' }
)

// Wave 4: Verification
phase('Wave 4')
const verifyResult = await agent(
  'You are the verification agent for Macro-Orchestrator Phase 4: Decision Trace.\n\n' +
  'PROJECT: /home/igzela/Projects/token-efficient-agent-harness-lab\n\n' +
  'TASK: Run the full verification suite. Fix any issues found.\n\n' +
  'Run these commands in order:\n\n' +
  '1. cd engine && cargo test 2>&1 | tail -20\n' +
  '2. cd engine && cargo fmt --check 2>&1\n' +
  '3. cd engine && cargo clippy -p engine --all-targets -- -D warnings 2>&1 | tail -20\n' +
  '4. cd sdk/typescript && bun run build 2>&1\n' +
  '5. cd sdk/typescript && bun test 2>&1\n' +
  '6. cd dashboard && bun run typecheck 2>&1\n' +
  '7. cd dashboard && bun run build 2>&1\n' +
  '8. cd dashboard && bun run build:static 2>&1\n' +
  '9. cd sdk/python && python -m pytest tests/ -v 2>&1\n' +
  '10. cd /home/igzela/Projects/token-efficient-agent-harness-lab && bash scripts/check_wire_codegen_drift.sh 2>&1\n' +
  '11. cd /home/igzela/Projects/token-efficient-agent-harness-lab && uv run --no-project python scripts/check_agent_handoff.py 2>&1\n\n' +
  'If any test fails, investigate and fix the issue.\n\n' +
  'Report:\n' +
  '- Each command pass/fail\n' +
  '- Total Rust test count\n' +
  '- Any issues found and fixed\n' +
  '- Final verdict: ALL PASS or list of failures',
  { label: 'wave4-verify', phase: 'Wave 4', model: 'sonnet' }
)

log('Phase 4 Decision Trace complete. Verify: ' + (verifyResult ? verifyResult.slice(0, 500) : 'done'))
