export const meta = {
  name: 'phase2-executor-pool-waves2-3',
  description: 'Wave 2: Scheduler + DynamicController integration. Wave 3: API + SDK + Dashboard.',
  phases: [
    { title: 'Wave 2: Scheduler Integration', detail: 'Integrate ExecutorPool into scheduler tick and DynamicController' },
    { title: 'Wave 3: API + SDK + Dashboard', detail: 'Add endpoint, SDK methods, dashboard component' },
    { title: 'Verify', detail: 'Run cargo test, fmt, clippy, TypeScript build, handoff guard' },
  ],
}

// ===== Phase 1: Scheduler Integration =====
phase('Wave 2: Scheduler Integration')

const schedulerResult = await agent(
  `You are implementing Macro-Orchestrator Phase 2: Resource / Executor Pool.

The executor_pool module is already implemented and compiles at engine/src/executor_pool.rs.
Schema migration v8 for executor_pool table is in engine/src/storage/local_product_store/migrations.rs.
executor_pool_store.rs with save/load is in engine/src/storage/local_product_store/executor_pool_store.rs.

YOUR TASK: Integrate ExecutorPool into the scheduler.

Files to modify:
1. engine/src/scheduler.rs — Add executor_pool field to WorkflowScheduler, init pool in start(), use pool in scheduler_tick() and dynamic_scheduler_tick(), add pool snapshot to status(), call tick_cooldowns().
2. engine/src/workflow/dynamic_controller.rs — Add optional executor_pool field, use it for suggested_executor_type in ControllerTickResult, acquire/release around tick execution.

Key patterns from existing code:
- WorkflowScheduler::start() creates executor at line ~135 via create_scheduler_executor()
- scheduler_tick() calls store.tick_with_executor() at line ~333
- dynamic_scheduler_tick() creates DynamicWorkflowController per run at line ~460
- DynamicWorkflowController::tick() calls store.tick_with_executor_and_command() and returns ControllerTickResult with suggested_executor_type
- SchedulerConfig::from_env() reads ACP_SCHEDULER_EXECUTOR

Requirements:
- WorkflowScheduler gets: executor_pool: Arc<ExecutorPool>
- start() calls register_default_executors(&pool, cli_enabled) and stores pool
- scheduler_tick(): before tick, call pool.best_for_task() + pool.acquire(); after tick, pool.release()
- dynamic_scheduler_tick(): same acquire/release pattern; pass pool to DynamicWorkflowController
- DynamicWorkflowController gets: executor_pool: Option<Arc<ExecutorPool>>
- DynamicWorkflowController::tick(): if pool available, use pool.best_for_task() for suggested_executor_type
- status(): add "executor_pool" key with pool.snapshot()
- Add pool.tick_cooldowns() at start of each tick cycle
- Maintain all existing tests passing — do not break existing signatures

IMPORTANT:
- Do NOT change the public API of tick_with_executor or tick_with_executor_and_command
- The pool wraps around the existing flow, it doesn't replace it
- All existing tests must continue to pass
- Run: cargo check -p engine after changes
- Run: cargo test -p engine --lib scheduler to verify scheduler tests pass`,
  { label: 'scheduler-integration', phase: 'Wave 2: Scheduler Integration', model: 'opus' }
)

const controllerResult = await agent(
  `You are implementing Macro-Orchestrator Phase 2: Resource / Executor Pool — DynamicWorkflowController integration.

The executor_pool module is at engine/src/executor_pool.rs (already compiles).
The scheduler integration was done in the previous step.

YOUR TASK: Integrate ExecutorPool into DynamicWorkflowController.

File to modify: engine/src/workflow/dynamic_controller.rs

Changes needed:
1. Add executor_pool: Option<Arc<crate::executor_pool::ExecutorPool>> field to DynamicWorkflowController struct
2. Add with_executor_pool() constructor variant or modify existing constructor to accept pool
3. In tick(): 
   - If pool is available, call pool.best_for_task(task_type, task_domain) to set suggested_executor_type in ControllerTickResult
   - Before calling store.tick_with_executor_and_command(), call pool.acquire(executor_type) — if acquire fails, skip this run
   - After tick completes, call pool.release(executor_type, success, latency_ms, cost) based on result
4. Add pool_failure_score and pool_active_count to ControllerTickResult

Key context:
- DynamicWorkflowController::tick() signature: pub fn tick(&self, store, run_id, actor, executor) -> Result<ControllerTickResult, String>
- The executor parameter is &dyn NodeExecutor passed from scheduler
- ControllerTickResult already has suggested_executor_type field
- The tick method calls store.tick_with_executor_and_command() which returns a Value with result info

IMPORTANT:
- Keep backward compatibility — pool is optional, existing behavior unchanged when pool is None
- All existing tests must pass
- Run: cargo check -p engine after changes
- Run: cargo test -p engine --lib workflow::dynamic_controller to verify`,
  { label: 'controller-integration', phase: 'Wave 2: Scheduler Integration', model: 'opus' }
)

// ===== Phase 2: API + SDK + Dashboard =====
phase('Wave 3: API + SDK + Dashboard')

const apiResult = await agent(
  `You are implementing Macro-Orchestrator Phase 2: Resource / Executor Pool — API endpoint.

The executor_pool module is at engine/src/executor_pool.rs.
The store save/load is at engine/src/storage/local_product_store/executor_pool_store.rs.

YOUR TASK: Create the HTTP API endpoint and wire it into the router.

Files to create/modify:
1. engine/src/http_server/handlers/executor_pool.rs (NEW) — handler function
2. engine/src/http_server/handlers/mod.rs — add pub mod executor_pool
3. engine/src/http_server/routes.rs — add the route

Pattern to follow (from engine/src/http_server/handlers/scheduler.rs):
```rust
use axum::{extract::State, Json};
use serde_json::{json, Value};

use crate::http_server::handlers::{ApiError, RequestContext};
use crate::http_server::state::AxumApiState;

pub async fn api_executor_pool(
    State(state): State<AxumApiState>,
    context: RequestContext,
) -> Result<Json<Value>, ApiError> {
    // check scope
    // get executor pool from state (scheduler or fallback)
    // return JSON with schema_version, executors, total_active, total_capacity
}
```

The handler should:
- Require "health:read" scope (same pattern as scheduler handler)
- Return: {"schema_version": "executor_pool.v1", "executors": [...], "total_active": N, "total_capacity": N}
- If scheduler is None, return empty pool with totals=0

Route: GET /api/v1/executor-pool

IMPORTANT:
- Follow existing handler patterns exactly
- Run: cargo check -p engine after changes
- Run: cargo test -p engine --test test_http_server to verify HTTP tests pass`,
  { label: 'api-endpoint', phase: 'Wave 3: API + SDK + Dashboard', model: 'opus' }
)

const sdkResult = await agent(
  `You are implementing Macro-Orchestrator Phase 2: Resource / Executor Pool — SDK methods.

YOUR TASK: Add fetchExecutorPool to TypeScript SDK and fetch_executor_pool to Python SDK.

TypeScript SDK (sdk/typescript/src/):
1. Find api-types.ts or the main types file — add ExecutorPoolCapabilities, ExecutorPoolEntry, ExecutorPoolStatus interfaces
2. Find the client file — add fetchExecutorPool() method that calls GET /api/v1/executor-pool

Python SDK (sdk/python/):
1. Find client.py — add fetch_executor_pool(self) -> dict method that calls GET /api/v1/executor-pool

TypeScript types to add:
```typescript
export interface ExecutorPoolCapabilities {
  supported_task_types: string[];
  supported_task_domains: string[];
  requires_auth: boolean;
  requires_cli: boolean;
  max_timeout_ms: number;
}

export interface ExecutorPoolEntry {
  executor_type: string;
  capabilities: ExecutorPoolCapabilities;
  available: boolean;
  active_count: number;
  concurrency_limit: number;
  cooldown_until: string | null;
  failure_score: number;
  cost_per_execution_usd: number | null;
  daily_cost_usd: number;
  daily_cost_limit_usd: number | null;
  total_executions: number;
  success_rate: number;
  avg_latency_ms: number;
  last_executed_at: string | null;
}

export interface ExecutorPoolStatus {
  schema_version: string;
  executors: ExecutorPoolEntry[];
  total_active: number;
  total_capacity: number;
}
```

IMPORTANT:
- Follow existing patterns in each SDK exactly
- Run: cd sdk/typescript && bun run build && bun run test
- Run: cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests`,
  { label: 'sdk-methods', phase: 'Wave 3: API + SDK + Dashboard', model: 'opus' }
)

const dashboardResult = await agent(
  `You are implementing Macro-Orchestrator Phase 2: Resource / Executor Pool — Dashboard component.

YOUR TASK: Create ExecutorPool dashboard component and wire it into navigation.

Files to create/modify:
1. dashboard/src/components/ExecutorPool.tsx (NEW) — main component
2. dashboard/src/lib/types.ts — add ExecutorPoolCapabilities, ExecutorPoolEntry, ExecutorPoolStatus interfaces
3. dashboard/src/lib/api.ts — add fetchExecutorPool() function
4. dashboard/src/app/page.tsx or main layout — add ExecutorPool tab/component to navigation

Component design:
- Table with columns: Type, Status (available/unavailable/cooldown pill), Active/Capacity (progress bar), Failure Score, Success Rate, Avg Latency, Cost/Exec, Daily Cost
- Fetches from GET /api/v1/executor-path via fetchExecutorPool()
- Uses EmptyState component when pool is empty
- Shows "In Cooldown" badge with time remaining when cooldown_until is set
- Capacity bar: green when <75%, yellow when 75-90%, red when >90%

TypeScript types (in dashboard/src/lib/types.ts):
```typescript
export interface ExecutorPoolCapabilities {
  supported_task_types: string[];
  supported_task_domains: string[];
  requires_auth: boolean;
  requires_cli: boolean;
  max_timeout_ms: number;
}

export interface ExecutorPoolEntry {
  executor_type: string;
  capabilities: ExecutorPoolCapabilities;
  available: boolean;
  active_count: number;
  concurrency_limit: number;
  cooldown_until: string | null;
  failure_score: number;
  cost_per_execution_usd: number | null;
  daily_cost_usd: number;
  daily_cost_limit_usd: number | null;
  total_executions: number;
  success_rate: number;
  avg_latency_ms: number;
  last_executed_at: string | null;
}

export interface ExecutorPoolStatus {
  schema_version: string;
  executors: ExecutorPoolEntry[];
  total_active: number;
  total_capacity: number;
}
```

IMPORTANT:
- Follow existing component patterns (SchedulerStatus.tsx, WorkflowRuns.tsx)
- Use existing CSS utility classes
- Run: cd dashboard && bun run typecheck && bun run lint && bun run build`,
  { label: 'dashboard-component', phase: 'Wave 3: API + SDK + Dashboard', model: 'opus' }
)

// ===== Phase 3: Verify =====
phase('Verify')

const verifyResult = await agent(
  `Verify the Phase 2 Executor Pool implementation.

Run these commands and report results:
1. cargo test -p engine (all tests must pass, expect ~1242 tests — 1226 baseline + 16 new executor pool tests)
2. cargo fmt --check
3. cargo clippy -p engine --all-targets -- -D warnings
4. cd sdk/typescript && bun run build && bun run test
5. cd dashboard && bun run typecheck && bun run lint && bun run build
6. uv run --no-project python scripts/check_agent_handoff.py
7. bash scripts/check_wire_codegen_drift.sh

Report each result. If any fail, show the error output.`,
  { label: 'verification', phase: 'Verify', model: 'sonnet' }
)

return { schedulerResult, controllerResult, apiResult, sdkResult, dashboardResult, verifyResult }
