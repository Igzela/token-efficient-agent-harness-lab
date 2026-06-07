# Phase 2 — Resource / Executor Pool

**Macro-Orchestrator Track · Phase 2 of 5**
**Status:** PLANNED — awaiting approval

---

## Goal

Model executors as first-class poolable resources with capability metadata, availability tracking, concurrency limits, cooldown, failure scoring, and cost profile. The scheduler picks executors from the pool per run/node instead of using a single system-wide instance. Dashboard and SDK read executor-pool status.

## Constraints (must not violate)

| Boundary | Rule |
|---|---|
| No parallel scheduler/DAG/kernel | Extends existing `scheduler.rs`, `node_executor.rs`, `workflow/`, `storage/`, `http_server/` |
| No target repo writes | Executor pool is app-owned state only |
| No real provider default-on | Provider-based executors remain env-gated and opt-in |
| No sandbox/process/VM expansion | CLI executor path unchanged |
| No hosted SaaS | Local self-hosted only |
| R-series sealed at R7 | No file splitting |

---

## Architecture

### ExecutorPool — new module: `engine/src/executor_pool.rs`

A thread-safe in-memory executor registry backed by SQLite for durability. The pool tracks each registered executor by type string, maps it to an `Arc<dyn NodeExecutor>` instance, and exposes capability/health/cost metadata.

**Core struct:**

```rust
pub struct ExecutorPool {
    entries: RwLock<HashMap<String, ExecutorEntry>>,
    store: Arc<LocalProductStore>,
}

pub struct ExecutorEntry {
    pub executor_type: String,
    pub executor: Arc<dyn NodeExecutor>,        // live instance
    pub capabilities: ExecutorCapabilities,
    pub status: ExecutorStatus,
    pub cost_profile: CostProfile,
    pub metrics: ExecutorMetrics,               // mutable, updated after each tick
}

pub struct ExecutorCapabilities {
    pub supported_task_types: Vec<String>,      // e.g. ["code_generate", "code_debug"]
    pub supported_task_domains: Vec<String>,    // e.g. ["code", "architecture"]
    pub requires_auth: bool,                    // needs API key / CLI binary
    pub requires_cli: bool,                     // needs CLI binary present
    pub max_timeout_ms: u64,
}

pub struct ExecutorStatus {
    pub available: bool,                        // false during cooldown or manual disable
    pub active_count: u64,                      // currently running nodes
    pub concurrency_limit: u64,                 // max concurrent nodes
    pub cooldown_until: Option<String>,         // ISO timestamp
    pub failure_score: f64,                     // 0.0 (healthy) to 1.0 (unhealthy), decays over time
}

pub struct CostProfile {
    pub cost_per_execution_usd: Option<f64>,    // estimated per-node cost
    pub daily_cost_usd: Option<f64>,            // today's total
    pub daily_cost_limit_usd: Option<f64>,      // configurable cap
}

pub struct ExecutorMetrics {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub avg_latency_ms: f64,
    pub total_latency_ms: u64,
    pub last_executed_at: Option<String>,
}
```

**Key methods:**

```rust
impl ExecutorPool {
    pub fn new(store: Arc<LocalProductStore>) -> Self;
    pub fn register(&self, entry: ExecutorEntry);
    pub fn get(&self, executor_type: &str) -> Option<Arc<dyn NodeExecutor>>;
    pub fn best_for_task(&self, task_type: &str, task_domain: &str) -> Option<String>;
    pub fn acquire(&self, executor_type: &str) -> bool;       // increment active_count, check concurrency
    pub fn release(&self, executor_type: &str, success: bool, latency_ms: u64, cost: Option<f64>);
    pub fn snapshot(&self) -> Vec<ExecutorPoolEntry>;          // for API/dashboard
    pub fn start_cooldown(&self, executor_type: &str, duration_ms: u64);
    pub fn tick_cooldowns(&self);                              // called by scheduler tick loop
}
```

### ExecutorPoolEntry (API/Dashboard serializable)

```rust
pub struct ExecutorPoolEntry {
    pub executor_type: String,
    pub capabilities: ExecutorCapabilities,
    pub available: bool,
    pub active_count: u64,
    pub concurrency_limit: u64,
    pub cooldown_until: Option<String>,
    pub failure_score: f64,
    pub cost_per_execution_usd: Option<f64>,
    pub daily_cost_usd: f64,
    pub daily_cost_limit_usd: Option<f64>,
    pub total_executions: u64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub last_executed_at: Option<String>,
}
```

---

## Schema Changes

### Migration v8: `executor_pool` table

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

Persistence is for audit/restart recovery. The live `ExecutorPool` in-memory structure holds the `Arc<dyn NodeExecutor>` instances; the DB row holds the metadata snapshots.

---

## Integration Points

### 1. Scheduler integration (`engine/src/scheduler.rs`)

**Changes to `WorkflowScheduler`:**
- Add `executor_pool: Arc<ExecutorPool>` field.
- `WorkflowScheduler::start()` registers all configured executors into the pool instead of creating a single `Arc<dyn NodeExecutor>`.
- `scheduler_tick()` and `dynamic_scheduler_tick()` call `pool.best_for_task(task_type, task_domain)` per run to select an executor, then `pool.acquire()` before `tick_with_executor()` and `pool.release()` after.
- After tick, `pool.release()` updates metrics (success/failure, latency, cost) and checks failure_score to trigger cooldown.

**Changes to `SchedulerConfig`:**
- Add `executor_pool_init: bool` (default true) — when true, scheduler registers available executors at startup.
- Keep `executor_type` as the **default fallback** executor type for runs that don't match any pool entry.

**Changes to `SchedulerModules`:**
- Pass `executor_pool: Arc<ExecutorPool>` through so dynamic controller can also use it.

### 2. DynamicController integration (`engine/src/workflow/dynamic_controller.rs`)

- `DynamicWorkflowController` receives `executor_pool: Arc<ExecutorPool>` in its constructor.
- Phase 5 (suggested executor) now actually looks up `pool.best_for_task()` and uses it to suggest in `ControllerTickResult.suggested_executor_type`.
- The controller's `tick()` method acquires/releases from the pool.

### 3. Scheduler feedback integration (`engine/src/storage/local_product_store/feedback.rs`)

- `insert_scheduler_feedback()` already writes `executor_type`, `latency_ms`, `cost`, `success`. No schema change needed.
- The pool's `release()` method is called after feedback is recorded, feeding metrics into the pool.

### 4. OrchestrationDecision integration (`engine/src/workflow/orchestration_decision.rs`)

- `selected_executor` field now reflects the pool-chosen executor type (already a string, no struct change needed).
- `input_signals` can include `pool_failure_score` and `pool_active_count` for the chosen executor.

### 5. HTTP API

**New endpoint:** `GET /api/v1/executor-pool`
```json
{
  "schema_version": "executor_pool.v1",
  "executors": [ ExecutorPoolEntry, ... ],
  "total_active": 2,
  "total_capacity": 12
}
```

**Extended:** `GET /api/v1/scheduler` (existing endpoint) — add `executor_pool` field in the scheduler status JSON containing the pool snapshot alongside existing config.

### 6. SDK

**TypeScript SDK** (`sdk/typescript/src/`):
- New interface `ExecutorPoolEntry` and `ExecutorPoolStatus`.
- New method: `fetchExecutorPool(): Promise<ExecutorPoolStatus>`.

**Python SDK** (`sdk/python/`):
- New method: `fetch_executor_pool() -> dict`.

### 7. Dashboard

**New component:** `dashboard/src/components/ExecutorPool.tsx`
- Table with columns: Executor Type, Status (available/unavailable/cooldown), Active/Capacity, Failure Score, Success Rate, Avg Latency, Cost/Execution, Daily Cost.
- Each row shows a visual capacity bar (active_count / concurrency_limit).
- Cooldown remaining shown as relative time.
- Linked from Scheduler tab as a sub-section or separate tab.

**Updated:** `SchedulerStatus.tsx` — add `executor_pool` sub-section showing pool summary (total executors, total active, total capacity).

**TypeScript types** (`dashboard/src/lib/types.ts`):
```typescript
interface ExecutorPoolEntry {
  executor_type: string;
  capabilities: {
    supported_task_types: string[];
    supported_task_domains: string[];
    requires_auth: boolean;
    requires_cli: boolean;
    max_timeout_ms: number;
  };
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

interface ExecutorPoolStatus {
  schema_version: string;
  executors: ExecutorPoolEntry[];
  total_active: number;
  total_capacity: number;
}
```

---

## Failure Score Model

```
new_failure_score = current_score * decay_factor + (1.0 if failed else 0.0) * failure_weight
decay_factor = 0.95 per execution (healthy executions gradually reduce score)
failure_weight = 0.2 (each failure adds 20% towards 1.0)
cooldown triggered when failure_score >= 0.8
cooldown_duration_ms = min(60000, base_cooldown * (1 + floor(failure_score * 5)))
```

Failure score decays naturally as successful executions occur. Cooldown is automatic on high failure score; manual disable is also supported via API.

---

## Concurrency Model

- `ExecutorPool` uses `RwLock<HashMap>` — reads (get/snapshot) are lock-free under `RwLock::read()`, writes (acquire/release/register) use `RwLock::write()`.
- `active_count` is atomically incremented on `acquire()` and decremented on `release()`.
- `acquire()` returns `false` if `active_count >= concurrency_limit` or `!available` or in cooldown.
- No contention with SQLite — pool state is in-memory only; DB writes are async snapshots.

---

## Default Executor Pool Configuration

At startup, the scheduler registers executors based on env vars:

| Executor Type | Condition | Concurrency | Capabilities |
|---|---|---|---|
| `"noop"` | Always | 100 | All task types |
| `"stub"` | Always | 100 | All task types |
| `"command"` | Always | 4 | All task types |
| `"claude_code_cli"` | `ACP_ENABLE_CLI_EXECUTION=1` + binary found | 2 | code_* domains |
| `"codex_cli"` | `ACP_ENABLE_CLI_EXECUTION=1` + binary found | 2 | code_* domains |

Defaults can be overridden via `ACP_EXECUTOR_POOL_CONFIG` JSON env var (future extensibility).

---

## Implementation Order

### Wave 1 — Core pool + schema
1. `engine/src/executor_pool.rs` — `ExecutorPool`, `ExecutorEntry`, `ExecutorCapabilities`, `ExecutorStatus`, `CostProfile`, `ExecutorMetrics`, `ExecutorPoolEntry` structs and methods.
2. `engine/src/storage/local_product_store/migrations.rs` — migration v8: `executor_pool` table.
3. `engine/src/storage/local_product_store/mod.rs` — `executor_pool_snapshot()`, `load_executor_pool()`, `save_executor_pool()` methods on `LocalProductStore`.
4. Tests for pool acquire/release, failure score, cooldown, best_for_task, snapshot.

### Wave 2 — Scheduler + dynamic controller integration
5. `engine/src/scheduler.rs` — `WorkflowScheduler` gains `executor_pool: Arc<ExecutorPool>`, pool init at start, acquire/release in tick, metrics update after tick.
6. `engine/src/workflow/dynamic_controller.rs` — constructor gains `executor_pool`, `tick()` uses pool for executor selection.
7. Tests for scheduler pool integration, dynamic controller pool selection, feedback-to-pool metrics flow.

### Wave 3 — API + SDK + Dashboard
8. `engine/src/http_server/handlers/` — new `executor_pool.rs` handler for `GET /api/v1/executor-pool`.
9. `engine/src/http_server/routes.rs` — wire the new route.
10. `sdk/typescript/src/` — `ExecutorPoolEntry` type + `fetchExecutorPool()` method.
11. `sdk/python/` — `fetch_executor_pool()` method.
12. `dashboard/src/components/ExecutorPool.tsx` — pool status table.
13. Dashboard types and navigation.

### Wave 4 — Verification + handoff
14. `cargo test -p engine` — all tests pass.
15. `cargo fmt`, `cargo clippy` — clean.
16. `uv run --no-project python scripts/check_agent_handoff.py` — passes.
17. Update `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`.

---

## Verification

```bash
# Full Rust test suite
cargo test -p engine

# Format + clippy
cargo fmt --check
cargo clippy -p engine --all-targets -- -D warnings

# TypeScript SDK
cd sdk/typescript && bun run build && bun run test

# Dashboard typecheck + build
cd dashboard && bun run typecheck && bun run build

# Handoff guard
uv run --no-project python scripts/check_agent_handoff.py

# Wire codegen drift
bash scripts/check_wire_codegen_drift.sh
```

---

## Done-When Criteria

1. `ExecutorPool` module exists with acquire/release/failure-score/cooldown/best_for_task/snapshot.
2. Scheduler tick uses pool to select executor per run (not single system-wide).
3. Dynamic controller tick uses pool for executor selection.
4. `GET /api/v1/executor-pool` returns pool status with all executor metadata.
5. Dashboard ExecutorPool component shows pool table with capacity, health, cost.
6. TypeScript + Python SDK expose `fetchExecutorPool()`.
7. Schema v8 migration creates `executor_pool` table.
8. All Rust tests pass, `cargo clippy` clean, `cargo fmt` clean.
9. `check_agent_handoff.py` passes.
10. No target repo writes, no new parallel runtime, no provider default-on, no sandbox expansion.
