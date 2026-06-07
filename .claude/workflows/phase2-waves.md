export const meta = {
  name: 'phase2-executor-pool-waves',
  description: 'Wave 2: Scheduler + DynamicController integration. Wave 3: API + SDK + Dashboard. Wave 4: Verify.',
  phases: [
    { title: 'Wave 2: Scheduler Integration', detail: 'Integrate ExecutorPool into scheduler tick and DynamicController' },
    { title: 'Wave 3: API + SDK + Dashboard', detail: 'Add endpoint, SDK methods, dashboard component' },
    { title: 'Verify', detail: 'Run cargo test, fmt, clippy, TypeScript build, handoff guard' },
  ],
}

phase('Wave 2: Scheduler Integration')

const schedulerResult = await agent(
  'You are implementing Macro-Orchestrator Phase 2: Resource / Executor Pool - Scheduler integration.\n\n' +
  'The executor_pool module is already implemented and compiles at engine/src/executor_pool.rs.\n' +
  'Schema migration v8 for executor_pool table is in engine/src/storage/local_product_store/migrations.rs.\n' +
  'executor_pool_store.rs with save/load is in engine/src/storage/local_product_store/executor_pool_store.rs.\n\n' +
  'YOUR TASK: Integrate ExecutorPool into the scheduler.\n\n' +
  'Files to modify:\n' +
  '1. engine/src/scheduler.rs - Add executor_pool field to WorkflowScheduler, init pool in start(), use pool in scheduler_tick() and dynamic_scheduler_tick(), add pool snapshot to status(), call tick_cooldowns().\n' +
  '2. engine/src/workflow/dynamic_controller.rs - Add optional executor_pool field, use it for suggested_executor_type in ControllerTickResult, acquire/release around tick execution.\n\n' +
  'Key patterns from existing code:\n' +
  '- WorkflowScheduler::start() creates executor at line ~135 via create_scheduler_executor()\n' +
  '- scheduler_tick() calls store.tick_with_executor() at line ~333\n' +
  '- dynamic_scheduler_tick() creates DynamicWorkflowController per run at line ~460\n' +
  '- DynamicWorkflowController::tick() calls store.tick_with_executor_and_command() and returns ControllerTickResult with suggested_executor_type\n' +
  '- SchedulerConfig::from_env() reads ACP_SCHEDULER_EXECUTOR\n\n' +
  'Requirements:\n' +
  '- WorkflowScheduler gets: executor_pool: Arc<ExecutorPool>\n' +
  '- start() calls register_default_executors(&pool, cli_enabled) and stores pool\n' +
  '- scheduler_tick(): before tick, call pool.best_for_task() + pool.acquire(); after tick, pool.release()\n' +
  '- dynamic_scheduler_tick(): same acquire/release pattern; pass pool to DynamicWorkflowController\n' +
  '- DynamicWorkflowController gets: executor_pool: Option<Arc<crate::executor_pool::ExecutorPool>>\n' +
  '- DynamicWorkflowController::tick(): if pool available, use pool.best_for_task() for suggested_executor_type\n' +
  '- status(): add "executor_pool" key with pool.snapshot()\n' +
  '- Add pool.tick_cooldowns() at start of each tick cycle\n' +
  '- Maintain all existing tests passing - do not break existing signatures\n\n' +
  'IMPORTANT:\n' +
  '- Do NOT change the public API of tick_with_executor or tick_with_executor_and_command\n' +
  '- The pool wraps around the existing flow, it does not replace it\n' +
  '- All existing tests must continue to pass\n' +
  '- Run: cargo check -p engine after changes\n' +
  '- Run: cargo test -p engine --lib scheduler to verify scheduler tests pass',
  { label: 'scheduler-integration', phase: 'Wave 2: Scheduler Integration', model: 'opus' }
)

const controllerResult = await agent(
  'You are implementing Macro-Orchestrator Phase 2: Resource / Executor Pool - DynamicWorkflowController integration.\n\n' +
  'The executor_pool module is at engine/src/executor_pool.rs (already compiles).\n' +
  'The scheduler integration was done in the previous step.\n\n' +
  'YOUR TASK: Integrate ExecutorPool into DynamicWorkflowController.\n\n' +
  'File to modify: engine/src/workflow/dynamic_controller.rs\n\n' +
  'Changes needed:\n' +
  '1. Add executor_pool: Option<Arc<crate::executor_pool::ExecutorPool>> field to DynamicWorkflowController struct\n' +
  '2. Add with_executor_pool() constructor variant or modify existing constructor to accept pool\n' +
  '3. In tick(): If pool is available, call pool.best_for_task(task_type, task_domain) to set suggested_executor_type in ControllerTickResult. Before calling store.tick_with_executor_and_command(), call pool.acquire(executor_type) - if acquire fails, skip this run. After tick completes, call pool.release(executor_type, success, latency_ms, cost) based on result.\n' +
  '4. Add pool_failure_score and pool_active_count to ControllerTickResult\n\n' +
  'Key context:\n' +
  '- DynamicWorkflowController::tick() takes executor: &dyn NodeExecutor from scheduler\n' +
  '- ControllerTickResult already has suggested_executor_type field\n' +
  '- The tick method calls store.tick_with_executor_and_command() which returns a Value with result info\n\n' +
  'IMPORTANT:\n' +
  '- Keep backward compatibility - pool is optional, existing behavior unchanged when pool is None\n' +
  '- All existing tests must pass\n' +
  '- Run: cargo check -p engine after changes\n' +
  '- Run: cargo test -p engine --lib workflow::dynamic_controller to verify',
  { label: 'controller-integration', phase: 'Wave 2: Scheduler Integration', model: 'opus' }
)

phase('Wave 3: API + SDK + Dashboard')

const apiResult = await agent(
  'You are implementing Macro-Orchestrator Phase 2: Resource / Executor Pool - API endpoint.\n\n' +
  'The executor_pool module is at engine/src/executor_pool.rs.\n' +
  'The store save/load is at engine/src/storage/local_product_store/executor_pool_store.rs.\n\n' +
  'YOUR TASK: Create the HTTP API endpoint and wire it into the router.\n\n' +
  'Files to create/modify:\n' +
  '1. engine/src/http_server/handlers/executor_pool.rs (NEW) - handler function\n' +
  '2. engine/src/http_server/handlers/mod.rs - add pub mod executor_pool\n' +
  '3. engine/src/http_server/routes.rs - add the route\n\n' +
  'Pattern to follow: engine/src/http_server/handlers/scheduler.rs\n' +
  'The handler should:\n' +
  '- Require "health:read" scope (same pattern as scheduler handler)\n' +
  '- Return: {"schema_version": "executor_pool.v1", "executors": [...], "total_active": N, "total_capacity": N}\n' +
  '- If scheduler is None, return empty pool with totals=0\n\n' +
  'Route: GET /api/v1/executor-pool\n\n' +
  'IMPORTANT:\n' +
  '- Follow existing handler patterns exactly\n' +
  '- Run: cargo check -p engine after changes\n' +
  '- Run: cargo test -p engine --test test_http_server to verify HTTP tests pass',
  { label: 'api-endpoint', phase: 'Wave 3: API + SDK + Dashboard', model: 'opus' }
)

const sdkResult = await agent(
  'You are implementing Macro-Orchestrator Phase 2: Resource / Executor Pool - SDK methods.\n\n' +
  'YOUR TASK: Add fetchExecutorPool to TypeScript SDK and fetch_executor_pool to Python SDK.\n\n' +
  'TypeScript SDK (sdk/typescript/src/):\n' +
  '1. Find api-types.ts or the main types file - add ExecutorPoolCapabilities, ExecutorPoolEntry, ExecutorPoolStatus interfaces\n' +
  '2. Find the client file - add fetchExecutorPool() method that calls GET /api/v1/executor-pool\n\n' +
  'Python SDK (sdk/python/):\n' +
  '1. Find client.py - add fetch_executor_pool(self) -> dict method that calls GET /api/v1/executor-pool\n\n' +
  'TypeScript types:\n' +
  '- ExecutorPoolCapabilities: supported_task_types (string[]), supported_task_domains (string[]), requires_auth (boolean), requires_cli (boolean), max_timeout_ms (number)\n' +
  '- ExecutorPoolEntry: executor_type (string), capabilities (ExecutorPoolCapabilities), available (boolean), active_count (number), concurrency_limit (number), cooldown_until (string|null), failure_score (number), cost_per_execution_usd (number|null), daily_cost_usd (number), daily_cost_limit_usd (number|null), total_executions (number), success_rate (number), avg_latency_ms (number), last_executed_at (string|null)\n' +
  '- ExecutorPoolStatus: schema_version (string), executors (ExecutorPoolEntry[]), total_active (number), total_capacity (number)\n\n' +
  'IMPORTANT:\n' +
  '- Follow existing patterns in each SDK exactly\n' +
  '- Run: cd sdk/typescript && bun run build && bun run test\n' +
  '- Run: cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests',
  { label: 'sdk-methods', phase: 'Wave 3: API + SDK + Dashboard', model: 'opus' }
)

const dashboardResult = await agent(
  'You are implementing Macro-Orchestrator Phase 2: Resource / Executor Pool - Dashboard component.\n\n' +
  'YOUR TASK: Create ExecutorPool dashboard component and wire it into navigation.\n\n' +
  'Files to create/modify:\n' +
  '1. dashboard/src/components/ExecutorPool.tsx (NEW) - main component\n' +
  '2. dashboard/src/lib/types.ts - add ExecutorPoolCapabilities, ExecutorPoolEntry, ExecutorPoolStatus interfaces\n' +
  '3. dashboard/src/lib/api.ts - add fetchExecutorPool() function\n' +
  '4. dashboard/src/app/page.tsx or main layout - add ExecutorPool tab/component to navigation\n\n' +
  'Component design:\n' +
  '- Table with columns: Type, Status (available/unavailable/cooldown pill), Active/Capacity (progress bar), Failure Score, Success Rate, Avg Latency, Cost/Exec, Daily Cost\n' +
  '- Fetches from GET /api/v1/executor-pool via fetchExecutorPool()\n' +
  '- Uses EmptyState component when pool is empty\n' +
  '- Shows In Cooldown badge with time remaining when cooldown_until is set\n' +
  '- Capacity bar: green when <75%, yellow when 75-90%, red when >90%\n\n' +
  'IMPORTANT:\n' +
  '- Follow existing component patterns (SchedulerStatus.tsx, WorkflowRuns.tsx)\n' +
  '- Use existing CSS utility classes\n' +
  '- Run: cd dashboard && bun run typecheck && bun run lint && bun run build',
  { label: 'dashboard-component', phase: 'Wave 3: API + SDK + Dashboard', model: 'opus' }
)

phase('Verify')

const verifyResult = await agent(
  'Verify the Phase 2 Executor Pool implementation.\n\n' +
  'Run these commands and report results:\n' +
  '1. cargo test -p engine (all tests must pass, expect ~1242 tests - 1226 baseline + ~16 new executor pool tests)\n' +
  '2. cargo fmt --check\n' +
  '3. cargo clippy -p engine --all-targets -- -D warnings\n' +
  '4. cd sdk/typescript && bun run build && bun run test\n' +
  '5. cd dashboard && bun run typecheck && bun run lint && bun run build\n' +
  '6. uv run --no-project python scripts/check_agent_handoff.py\n' +
  '7. bash scripts/check_wire_codegen_drift.sh\n\n' +
  'Report each result. If any fail, show the error output.',
  { label: 'verification', phase: 'Verify', model: 'sonnet' }
)

return { schedulerResult, controllerResult, apiResult, sdkResult, dashboardResult, verifyResult }
