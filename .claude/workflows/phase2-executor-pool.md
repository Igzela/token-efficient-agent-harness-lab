# Phase 2 — Resource / Executor Pool Implementation

**Branch:** `feat/dashboard-ux-polish` (current)
**Baseline:** 1226 Rust tests, all passing
**Target:** Add executor pool module, SQLite schema v8, scheduler integration, dynamic controller integration, API endpoint, SDK methods, dashboard component. ~20 new tests minimum.

## Wave 1 — Core Executor Pool + Schema (parallel)

### Task 1.1: Executor Pool Module
**File:** `engine/src/executor_pool.rs` (NEW)

Create the `executor_pool` module with:
- `ExecutorPool` struct with `RwLock<HashMap<String, ExecutorEntry>>` and `Arc<LocalProductStore>`
- `ExecutorEntry` with `executor_type`, `executor: Arc<dyn NodeExecutor>`, `capabilities: ExecutorCapabilities`, `status: ExecutorStatus`, `cost_profile: CostProfile`, `metrics: ExecutorMetrics`
- `ExecutorCapabilities`: `supported_task_types: Vec<String>`, `supported_task_domains: Vec<String>`, `requires_auth: bool`, `requires_cli: bool`, `max_timeout_ms: u64`
- `ExecutorStatus`: `available: bool`, `active_count: u64`, `concurrency_limit: u64`, `cooldown_until: Option<String>`, `failure_score: f64`
- `CostProfile`: `cost_per_execution_usd: Option<f64>`, `daily_cost_usd: Option<f64>`, `daily_cost_limit_usd: Option<f64>`
- `ExecutorMetrics`: `total_executions: u64`, `successful_executions: u64`, `failed_executions: u64`, `avg_latency_ms: f64`, `total_latency_ms: u64`, `last_executed_at: Option<String>`
- `ExecutorPoolEntry` (serializable for API): flattened version of all the above

Methods:
- `new(store: Arc<LocalProductStore>) -> Self`
- `register(&self, entry: ExecutorEntry)`
- `get(&self, executor_type: &str) -> Option<Arc<dyn NodeExecutor>>`
- `best_for_task(&self, task_type: &str, task_domain: &str) -> Option<String>` — finds available executors matching task capabilities, returns highest success rate one
- `acquire(&self, executor_type: &str) -> bool` — increments active_count if available and under concurrency limit and not in cooldown
- `release(&self, executor_type: &str, success: bool, latency_ms: u64, cost: Option<f64>)` — decrements active_count, updates metrics, updates failure_score, triggers cooldown if failure_score >= 0.8
- `snapshot(&self) -> Vec<ExecutorPoolEntry>` — returns all entries for API
- `start_cooldown(&self, executor_type: &str, duration_ms: u64)`
- `tick_cooldowns(&self)` — checks all entries, sets available=true when cooldown expired
- `total_active(&self) -> u64`
- `total_capacity(&self) -> u64`

Failure score formula:
```
decay_factor = 0.95
failure_weight = 0.2
new_score = current_score * decay_factor + (if failed { failure_weight } else { 0.0 })
cooldown if new_score >= 0.8, duration = min(60000, base_cooldown * (1 + floor(score * 5)))
```

Default pool registration function:
```rust
pub fn register_default_executors(pool: &ExecutorPool, cli_enabled: bool)
```
- noop: concurrency 100, all task types
- stub: concurrency 100, all task types
- command: concurrency 4, all task types
- claude_code_cli: concurrency 2, code_* domains, requires_auth=true, requires_cli=true (only if cli_enabled)
- codex_cli: concurrency 2, code_* domains, requires_auth=true, requires_cli=true (only if cli_enabled)

Inline tests (~15 tests): register, acquire/release, failure score decay, cooldown trigger/tick, best_for_task matching, concurrency limit, snapshot, total_active/capacity.

### Task 1.2: SQLite Migration v8
**File:** `engine/src/storage/local_product_store/migrations.rs`

- Bump `CURRENT_SCHEMA_VERSION` from 7 to 8
- Add migration entry to `MIGRATIONS` array
- Add `migrate_v8_add_executor_pool()` method creating `executor_pool` table:
  ```sql
  CREATE TABLE IF NOT EXISTS executor_pool (
      executor_type TEXT PRIMARY KEY,
      capabilities_json TEXT NOT NULL DEFAULT '{}',
      status_json TEXT NOT NULL DEFAULT '{}',
      cost_profile_json TEXT NOT NULL DEFAULT '{}',
      metrics_json TEXT NOT NULL DEFAULT '{}',
      updated_at TEXT NOT NULL
  );
  ```
- Add persistence methods to `LocalProductStore`:
  - `save_executor_pool_snapshot(entries: &[ExecutorPoolEntry])` — upsert all entries
  - `load_executor_pool_snapshot() -> Vec<ExecutorPoolEntry>` — load all entries

### Task 1.3: Wire executor_pool into engine module tree
**File:** `engine/src/lib.rs` or `engine/src/main.rs`

- Add `pub mod executor_pool;`

## Wave 2 — Scheduler + DynamicController Integration (sequential after Wave 1)

### Task 2.1: Scheduler Integration
**File:** `engine/src/scheduler.rs`

Changes:
1. Add `executor_pool: Arc<ExecutorPool>` to `WorkflowScheduler` struct
2. In `WorkflowScheduler::start()`:
   - Create `ExecutorPool` with `Arc::clone(&store)`
   - Call `register_default_executors(&pool, cli_enabled)`
   - Store `Arc::clone(&pool)` in scheduler
   - Pass pool to scheduler_tick/dynamic_scheduler_tick via closure or Arc
3. In `scheduler_tick()`:
   - Before `store.tick_with_executor()`: call `pool.best_for_task(task_type, task_domain)` to get executor_type, then `pool.get(executor_type)` to get the executor
   - Call `pool.acquire(executor_type)` before tick; if false, skip this run
   - After tick: call `pool.release(executor_type, success, latency_ms, cost)` based on tick result
4. In `dynamic_scheduler_tick()`:
   - Same acquire/release pattern but through `DynamicWorkflowController`
   - Pass pool to controller constructor
5. In `status()`: add `executor_pool: pool.snapshot()` to the returned JSON
6. Add `pool.tick_cooldowns()` call at the start of each tick cycle

### Task 2.2: DynamicController Integration
**File:** `engine/src/workflow/dynamic_controller.rs`

Changes:
1. Add `executor_pool: Option<Arc<ExecutorPool>>` field to `DynamicWorkflowController`
2. Add `with_executor_pool()` constructor variant
3. In `tick()`:
   - If pool is available, use `pool.best_for_task()` for `suggested_executor_type` in `ControllerTickResult`
   - Acquire from pool before executing, release after
4. Update `ControllerTickResult` to include `pool_failure_score: Option<f64>` and `pool_active_count: Option<u64>`

## Wave 3 — API + SDK + Dashboard (parallel after Wave 2)

### Task 3.1: HTTP API Endpoint
**File:** `engine/src/http_server/handlers/executor_pool.rs` (NEW)

```rust
pub async fn api_executor_pool(
    State(state): State<AxumApiState>,
    context: RequestContext,
) -> Result<Json<Value>, ApiError>
```
- Requires `"health:read"` scope
- Returns `{"schema_version": "executor_pool.v1", "executors": [...], "total_active": N, "total_capacity": N}`
- If scheduler is None, returns empty pool with totals=0

### Task 3.2: Wire Route
**File:** `engine/src/http_server/routes.rs`

- Add `.route("/api/v1/executor-pool", get(handlers::executor_pool::api_executor_pool))` to the router

**File:** `engine/src/http_server/handlers/mod.rs`
- Add `pub mod executor_pool;`

### Task 3.3: TypeScript SDK
**File:** `sdk/typescript/src/api-types.ts` (or wherever types live)

Add interfaces:
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

**File:** SDK client file — add `fetchExecutorPool(): Promise<ExecutorPoolStatus>`

### Task 3.4: Python SDK
**File:** `sdk/python/src/agent_control_plane/client.py`

Add method:
```python
def fetch_executor_pool(self) -> dict:
    """Get executor pool status."""
    return self._get("/api/v1/executor-pool")
```

### Task 3.5: Dashboard Component
**File:** `dashboard/src/components/ExecutorPool.tsx` (NEW)

- Table with columns: Type, Status (pill), Active/Capacity (progress bar), Failure Score, Success Rate, Avg Latency, Cost/Exec, Daily Cost
- Cooldown shown as countdown or "In Cooldown" badge
- Fetches from `GET /api/v1/executor-pool`

**File:** `dashboard/src/lib/types.ts`
- Add `ExecutorPoolEntry`, `ExecutorPoolCapabilities`, `ExecutorPoolStatus` interfaces

**File:** `dashboard/src/lib/api.ts`
- Add `fetchExecutorPool()` function

**File:** `dashboard/src/components/SchedulerStatus.tsx`
- Add pool summary sub-section (or link to ExecutorPool tab)

## Wave 4 — Verification

1. `cargo test -p engine` — all tests pass (baseline 1226 + new ~20)
2. `cargo fmt --check`
3. `cargo clippy -p engine --all-targets -- -D warnings`
4. `cd sdk/typescript && bun run build && bun run test`
5. `cd dashboard && bun run typecheck && bun run lint && bun run build`
6. `uv run --no-project python scripts/check_agent_handoff.py`
7. `bash scripts/check_wire_codegen_drift.sh`

## Wave 5 — Documentation + Commit

1. Update `docs/CURRENT_STATUS.md` — add Executor Pool row to macro-orchestrator readiness
2. Update `docs/NEXT_DECISION.md` — mark Phase 2 done, Phase 3 (Queue/Priority/Backpressure) as next
3. Update `docs/MODULE_MAP.md` — add `executor_pool` to active modules table
4. Update `docs/plans/PHASE2_RESOURCE_EXECUTOR_POOL.md` — mark DONE
5. Commit and push
