# Architecture Book — Current System Baseline

**Generated:** 2026-06-11 | **Baseline:** Post-Dynamic Regulator Phase 5 (PR #40 merged)

---

## 1. Executive Summary

This system is a **local/small-team self-hosted macro-orchestrator control plane** for studying token-efficient agent workflows. It dispatches tasks to model providers or CLI executors, records execution traces, detects performance patterns, simulates alternative routing policies, generates policy proposals, and — under strict human-approved gates — can auto-adjust safe routing tier maps.

**What is automated:**
- Task dispatch and tier selection (rule-based, no LLM in dispatch kernel)
- Feedback trace recording and pattern detection
- Shadow policy simulation
- Policy proposal generation from detected patterns
- Dry-run previews of proposed policy changes

**What requires human approval:**
- Applying any policy proposal to the active routing policy
- Active auto-adjustment (requires `team:admin` auth + explicit confirmation flag)
- Rollback of applied adjustments
- Provider/CLI execution boundary changes
- Auth/security/deploy boundary changes
- Release, tagging, deployment
- Database migrations
- Target repository writes

**Active auto-adjustment after Phase 5:** The system can apply exactly one safe tier-map override per request, but only when explicitly enabled via environment variables (`ACP_ENABLE_AUTO_ADJUSTMENT=1`, `ACP_AUTO_ADJUSTMENT_ACTIVE=1`), authenticated as `team:admin`, and confirmed with `confirm_auto_adjustment=true`. It snapshots the current policy before mutation, records audit events, and supports hash-validated rollback. It is **disabled by default** and remains opt-in.

**Default safety posture:** All mutating policy features are off. Dispatch works without auth. Policy override endpoints require configured auth + admin role. No background scheduler performs auto-adjustment. No batch auto-apply exists. No daemonized auto-adjustment loop exists.

---

## 2. System Purpose and Product Boundary

### What the system is

A self-hosted control plane that orchestrates agent tasks across model providers and CLI executors. It provides:
- Deterministic task dispatch with cost/success/constraint-aware tier selection
- Workflow execution with DAG-based task decomposition
- Feedback-driven policy improvement through trace analysis and simulation
- Human-approved and guarded automatic policy adjustment
- Full audit trail via event-sourced ledger

### What the system is not

- **Not a SaaS platform.** Runs locally or on a small team's LAN.
- **Not an autonomous deployment agent.** No release/tag/deploy automation.
- **Not a target repository writer by default.** The app runtime does not write to target repositories. Agent maintenance mode may create branches/commits/PRs through the Real-World Testing Playbook, but this is a repository workflow, not an app-runtime feature.
- **Not a coding-agent runtime.** It orchestrates and dispatches; it does not execute code directly.
- **No real model-provider calls by default.** Provider execution requires `ACP_ENABLE_PROVIDER_EXECUTION=1`.

### Product boundary rules

| Boundary | Rule |
|---|---|
| Target repo writes | Not allowed by app runtime by default |
| Provider execution | Env-gated (`ACP_ENABLE_PROVIDER_EXECUTION=1`) |
| CLI execution | Env-gated (`ACP_ENABLE_CLI_EXECUTION=1`) |
| Auth/security changes | Explicit human approval required |
| DB migrations | Explicit human approval required |
| Release/tag/deploy | Explicit human approval required |
| Policy mutation | Explicit human approval (Phase 4) or strict gates (Phase 5 active) |

---

## 3. Runtime Architecture

### Core components

```
┌─────────────────────────────────────────────────────────────────┐
│                         HTTP Server (Axum)                       │
│  CORS · Request ID · Auth Middleware · 60+ API routes           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────┐ │
│  │ TaskAnalyzer  │→│ ModelSelector │→│    BudgetManager        │ │
│  └──────────────┘  └──────────────┘  └────────────────────────┘ │
│          │                │                    │                 │
│          └────────────────┴────────────────────┘                │
│                          │                                       │
│                   DispatchDecision                               │
│                          │                                       │
│  ┌───────────────────────┴────────────────────────────────────┐ │
│  │                    HybridExecutor                           │ │
│  │  ┌──────────┐  ┌──────────────┐  ┌───────────────────┐    │ │
│  │  │ Provider  │  │ CLI Executor │  │   Noop Default    │    │ │
│  │  │ Executor  │  │ (Claude/     │  │                   │    │ │
│  │  │          │  │  Codex)      │  │                   │    │ │
│  │  └──────────┘  └──────────────┘  └───────────────────┘    │ │
│  └───────────────────────────────────────────────────────────┘ │
│                          │                                       │
│  ┌───────────────────────┴────────────────────────────────────┐ │
│  │              DynamicWorkflowController                      │ │
│  │  observe → tick → evaluate → mutate                        │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  │ │
│  │  │ DAGMgr   │  │ RunQueue │  │ Backpress│  │ Decomposr│  │ │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘  │ │
│  └───────────────────────────────────────────────────────────┘ │
│                          │                                       │
│  ┌───────────────────────┴────────────────────────────────────┐ │
│  │                  Feedback Subsystem                         │ │
│  │  RunTraceRecorder → PatternDetector → OutcomeAttributor    │ │
│  │  ShadowRouter → PolicySimulator → PolicyProposer           │ │
│  │  ProposalValidator → AutoAdjustmentPolicy                  │ │
│  │  AutoAdjustmentGuard → PolicySnapshot                      │ │
│  └───────────────────────────────────────────────────────────┘ │
│                          │                                       │
│  ┌───────────────────────┴────────────────────────────────────┐ │
│  │              LocalProductStore                              │ │
│  │  SQLite (default, WAL) │ PostgreSQL (opt-in, feature-gated)│ │
│  │  26+ tables · Audit log · Backup manager · Encryption      │ │
│  └───────────────────────────────────────────────────────────┘ │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐ │
│  │  Scheduler · Executor Pool · Circuit Breaker · Health     │ │
│  └───────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### Component descriptions

| Component | Module | Purpose |
|---|---|---|
| HTTP Server | `engine/src/http_server/` | Axum-based API server with 60+ routes, CORS, auth middleware |
| TaskAnalyzer | `engine/src/task_analyzer/` | Analyzes incoming requests for task class, complexity, constraints |
| ModelSelector | `engine/src/model_selector.rs` | Selects model tier based on task analysis |
| BudgetManager | `engine/src/budget_manager.rs` | Enforces cost gates and budget constraints |
| DispatchEngine | `engine/src/dispatch_engine.rs` | Orchestrates the dispatch pipeline |
| DispatchLedger | `engine/src/dispatch_ledger.rs` | Event-sourced ledger for all dispatches |
| HybridExecutor | `engine/src/executor/hybrid.rs` | Routes to provider, CLI, or noop executor based on tier and complexity |
| ProviderExecutor | `engine/src/provider/` | Executes via model provider APIs (env-gated) |
| CLI Executor | `engine/src/cli/` | Executes via Claude Code / Codex CLI (env-gated) |
| DynamicWorkflowController | `engine/src/workflow/dynamic_controller.rs` | Tick-based workflow execution with graph mutation |
| RunQueue | `engine/src/workflow/run_queue.rs` | Priority queue with backpressure for workflow runs |
| DAGManager | `engine/src/workflow/dag_manager/` | Manages DAG structure for workflow runs |
| LocalProductStore | `engine/src/storage/local_product_store/` | Unified storage layer (SQLite/PostgreSQL) |
| BackupManager | `engine/src/storage/backup_manager.rs` | Automated backup with retention |
| Scheduler | `engine/src/scheduler.rs` | Workflow scheduling with persistent heartbeat |
| ExecutorPool | `engine/src/executor_pool.rs` | Executor resource pool management |
| CircuitBreaker | `engine/src/infrastructure/circuit_breaker.rs` | Provider failure circuit breaker |
| Auth | `engine/src/infrastructure/auth.rs` | API key authentication and tenant resolution |

### Dashboard and SDK

| Surface | Location | Purpose |
|---|---|---|
| Dashboard | `dashboard/` | TypeScript SPA (Bun + React) for operator visibility |
| TypeScript SDK | `sdk/typescript/` | Type-safe client for the API |
| Python SDK | `sdk/python/` | REST client for the API |

---

## 4. Data Flow

### End-to-end dispatch flow

```
1. Task/Workflow Input
   │
   ▼
2. Context Assembly (ContextPack)
   │  - cross-node context injection
   │  - budget allocation across nodes
   │
   ▼
3. Dispatch Decision
   │  - TaskAnalyzer → complexity, task_class, constraints
   │  - ModelSelector → tier selection
   │  - BudgetManager → cost gate check
   │  - active_routing_policy() applied
   │  - ShadowRoutes generated (observational only)
   │
   ▼
4. Execution Policy
   │  - HybridExecutor selects: provider / CLI / noop
   │  - Executor type recorded in dispatch record
   │
   ▼
5. Executor Selection & Run
   │  - ProviderExecutor (env-gated)
   │  - CliNodeExecutor (env-gated)
   │  - CommandNodeExecutor
   │  - DynamicWorkflowController (tick-based)
   │
   ▼
6. Run Trace Recording
   │  - RunTraceRecorder captures: dispatch_id, tier, latency,
   │    tokens, cost, success/failure, shadow routes, sections
   │
   ▼
7. Outcome Attribution
   │  - OutcomeAttributor produces: pass/fail/inconclusive,
   │    failure factors, confidence score
   │
   ▼
8. Pattern Detection
   │  - PatternDetector scans traces for:
   │    TierFailureConcentration, HighCostPerPass,
   │    HighLatencyCluster, RepeatedHumanReview, etc.
   │
   ▼
9. Policy Simulation (read-only)
   │  - ShadowRouter generates what-if routes
   │  - PolicySimulator compares actual vs simulated metrics
   │  - All outputs: source="shadow_only", influence_*=false
   │
   ▼
10. Proposal Generation
    │  - PolicyProposer creates ProposalCandidates from patterns
    │  - SafetyFlags: 7 invariants checked
    │  - CandidateEvidence links to traces and simulation
    │
    ▼
11. Human Approval (Phase 4)
    │  - /proposals/:id/approve requires team:admin + auth
    │  - /proposals/:id/reject, deactivate, rollback
    │  - SAFE_POLICY_OVERRIDE_TIERS restricts target tiers
    │
    ▼
12. Guarded Auto-Adjustment (Phase 5)
    │  - AutoAdjustmentGuard checks env gates
    │  - AutoAdjustmentPolicy evaluates eligibility (confidence ≥ 0.85)
    │  - PolicySnapshotRecord persists pre-mutation state + SHA-256 hash
    │  - POST /auto-adjustments/apply (one candidate per request)
    │  - Audit events recorded
    │
    ▼
13. Rollback / Audit
    - POST /auto-adjustments/:id/rollback (hash-validated)
    - GET /audit for full audit trail
    - GET /auto-adjustments for adjustment report
```

### Three distinct flows

| Flow | Description | Mutates state? |
|---|---|---|
| **Read-only analysis** | Traces, patterns, cost-of-pass, shadow routes, simulation reports | No |
| **Human-approved mutation** | Policy proposal CRUD with admin approval | Yes (with approval) |
| **Guarded active auto-adjustment** | Auto-apply safe tier-map overrides under strict gates | Yes (with env + auth + confirmation) |

---

## 5. Dynamic Regulator Phase 1–5 Map

| Phase | Goal | Status | Key Modules |
|---|---|---|---|
| 0 | Baseline Documentation and Observability | **PARTIAL** | `dispatch_metrics()`, `/api/v1/dispatch-metrics` |
| 1 | ContextPack Cross-Node Assembly | **DONE** | `engine/src/workflow/context_pack/` (assembly, budget, rules, types, validation) |
| 2 | Feedback Ledger and Replayable Run Traces | **DONE** | `RunTraceRecorder`, `OutcomeAttributor`, `PatternDetector` |
| 3 | Shadow Adaptive Policy Simulation | **DONE** | `ShadowRouter`, `PolicySimulator`, `ShadowRoute` in dispatch_decision |
| 4 | Human-Approved Policy Proposals | **DONE** | `PolicyProposer`, `ProposalValidator`, `ProposalSerializer`, full CRUD lifecycle |
| 5 | Guarded Auto-Adjustment | **DONE** | `AutoAdjustmentPolicy`, `AutoAdjustmentGuard`, `PolicySnapshotPreview/Record` |

### Phase 5 final status

Phase 5 is **DONE** per PR #40 signoff (2026-06-11). Active runtime remains **opt-in and disabled by default**. The controlled SQLite active apply/rollback drill passed all checks. PostgreSQL active trial was **not run** (`ACP_TEST_DATABASE_URL` not set).

**What Phase 5 delivered:**
- `AutoAdjustmentPolicy` evaluates candidate eligibility with 0.85 confidence threshold
- `AutoAdjustmentGuard` enforces disabled/dry-run/active modes via 3 env vars
- `PolicySnapshotRecord` persists pre-mutation state with SHA-256 safety hash
- One-shot apply endpoint: `POST /api/v1/auto-adjustments/apply`
- Rollback endpoint: `POST /api/v1/auto-adjustments/{id}/rollback`
- Audit events for all apply/rollback acceptances and rejections
- Content-stable candidate IDs
- Re-entry rejection (same candidate cannot be applied twice)
- Same-policy-key blocking (one active adjustment per policy key)

**What Phase 5 does NOT deliver:**
- No background scheduler for auto-adjustment
- No batch auto-apply
- No daemonized auto-adjustment loop
- No auto-rollback on degradation
- No dashboard Auto-Adjustments tab
- No TypeScript SDK wiring for auto-adjustment endpoints

---

## 6. API Surface

### Dispatch / Scheduler

| Endpoint | Method | Risk | Auth | Notes |
|---|---|---|---|---|
| `/api/v1/dispatch` | POST | Medium | None (basic) | Core dispatch; applies cost gates and active routing policy |
| `/api/v1/dispatches` | GET | Read-only | None | List/search dispatches |
| `/api/v1/dispatches/:id` | GET | Read-only | None | Single dispatch detail |
| `/api/v1/dispatch-metrics` | GET | Read-only | None | Aggregated dispatch metrics |
| `/api/v1/scheduler/status` | GET | Read-only | None | Scheduler health and status |

### Feedback

| Endpoint | Method | Risk | Auth | Notes |
|---|---|---|---|---|
| `/api/v1/feedback/traces` | GET | Read-only | None | Filterable by task_class, tier, status |
| `/api/v1/feedback/patterns` | GET | Read-only | None | Detected feedback patterns |
| `/api/v1/feedback/cost-of-pass` | GET | Read-only | None | Cost-of-pass analysis |

### Simulation

| Endpoint | Method | Risk | Auth | Notes |
|---|---|---|---|---|
| `/api/v1/simulation/report` | GET | Read-only | None | Simulation report |
| `/api/v1/simulation/policy-delta` | GET | Read-only | None | Policy simulation with named candidate |

### Policy Proposals

| Endpoint | Method | Risk | Auth | Notes |
|---|---|---|---|---|
| `/api/v1/proposals` | GET | Read-only | None | List proposals |
| `/api/v1/proposals` | POST | Medium | None | Create proposal |
| `/api/v1/proposals/:id` | GET | Read-only | None | Proposal detail |
| `/api/v1/proposals/:id/approve` | POST | High | team:admin | Approve proposal |
| `/api/v1/proposals/:id/reject` | POST | High | team:admin | Reject proposal |
| `/api/v1/proposals/:id/deactivate` | POST | High | team:admin | Deactivate proposal |
| `/api/v1/proposals/:id/rollback` | POST | High | team:admin | Rollback proposal |
| `/api/v1/proposals/generated` | GET | Read-only | None | Auto-generated proposals from patterns |

### Auto-Adjustments

| Endpoint | Method | Risk | Auth | Notes |
|---|---|---|---|---|
| `/api/v1/auto-adjustments` | GET | Read-only | None | Auto-adjustment report |
| `/api/v1/auto-adjustments/apply` | POST | Critical | team:admin + confirmation | Apply one generated candidate |
| `/api/v1/auto-adjustments/:id/rollback` | POST | Critical | team:admin + confirmation | Rollback with hash validation |

### Audit

| Endpoint | Method | Risk | Auth | Notes |
|---|---|---|---|---|
| `/api/v1/audit` | GET | Read-only | None | Full audit log |

### Health / Metrics

| Endpoint | Method | Risk | Auth | Notes |
|---|---|---|---|---|
| `/api/v1/health` | GET | Read-only | None | Deep health (DB, disk, memory, scheduler, backup) |
| `/api/v1/ready` | GET | Read-only | None | Lightweight readiness probe |
| `/api/v1/metrics` | GET | Read-only | None | Aggregate operational metrics |
| `/api/v1/metrics/observability` | GET | Read-only | None | Per-request observability |
| `/api/v1/circuit-breaker/status` | GET | Read-only | None | Circuit breaker state |

### Workflow Runs

| Endpoint | Method | Risk | Auth | Notes |
|---|---|---|---|---|
| `/api/v1/workflow-runs` | GET/POST | Medium | None | List/create runs |
| `/api/v1/workflow-runs/:id` | GET | Read-only | None | Run detail |
| `/api/v1/workflow-runs/:id/tick` | POST | Medium | None | Advance run (multiple executor modes) |
| `/api/v1/workflow-runs/:id/resume` | POST | Medium | None | Resume paused run |
| `/api/v1/workflow-runs/:id/cancel` | POST | Medium | None | Cancel run |

### Plans

| Endpoint | Method | Risk | Auth | Notes |
|---|---|---|---|---|
| `/api/v1/plans` | GET/POST | Medium | None | List/create plans |
| `/api/v1/plans/:id` | GET | Read-only | None | Plan detail |

### Supervised Patch

| Endpoint | Method | Risk | Auth | Notes |
|---|---|---|---|---|
| `/api/v1/supervised-patch/workspaces` | GET/POST | Medium | None | Workspace lifecycle |
| `/api/v1/supervised-patch/artifacts` | GET | Read-only | None | Patch artifacts |
| `.../capture`, `.../export` | POST | Medium | None | Capture and export patches |

### Ops / Admin

| Endpoint | Method | Risk | Auth | Notes |
|---|---|---|---|---|
| `/api/v1/team` | GET/POST | Medium | Configured | Team member management |
| `/api/v1/keys` | GET/POST | High | Configured | API key management |
| `/api/v1/backups` | GET/POST | High | Configured | Backup management |
| `/api/v1/export` | GET | Read-only | None | Data export |
| `/api/v1/import` | POST | Medium | None | Data import |
| `/api/v1/decisions` | GET | Read-only | None | Orchestration decisions |
| `/api/v1/queue/status` | GET | Read-only | None | Queue status |
| `/api/v1/executor-pool` | GET | Read-only | None | Executor pool status |
| `/api/v1/storage/integrity` | GET | Read-only | None | Storage integrity check |

---

## 7. Storage Architecture

### LocalProductStore

The storage layer is `LocalProductStore` in `engine/src/storage/local_product_store/`. It supports two backends:

| Backend | Status | Configuration |
|---|---|---|
| **SQLite** | Default, primary | WAL mode, optional encryption via `ACP_DB_ENCRYPTION_KEY` |
| **PostgreSQL** | Opt-in, feature-gated | `#[cfg(feature = "pg")]`, requires `ACP_DATABASE_URL` |

### Schema (26+ tables)

| Table | Purpose |
|---|---|
| `dispatch_history` | All dispatch records with tier, cost, latency, outcome |
| `local_config` | System configuration key-value store |
| `team_members` | Team member records with roles |
| `api_key_metadata` | API key records with scopes and expiry |
| `audit_log` | Full audit trail of all mutations |
| `provider_audit_events` | Provider call audit events |
| `workflow_plans` | Workflow plan definitions |
| `workflow_runs` | Workflow run state and metadata |
| `workflow_run_nodes` | DAG nodes for workflow runs |
| `workflow_run_edges` | DAG edges for workflow runs |
| `workflow_run_events` | Events emitted during workflow execution |
| `workflow_run_approvals` | Approval records for workflow runs |
| `supervised_patch_workspaces` | Patch workspace lifecycle |
| `supervised_patch_artifacts` | Captured patch artifacts |
| `scheduler_feedback` | Scheduler feedback records |
| `agent_profiles` | Agent profile definitions |
| `tool_capabilities` | Tool capability declarations |
| `tool_allowlists` | Tool allowlists per agent |
| `tool_hooks` | Pre/post hooks for tools |
| `orchestration_decisions` | Orchestration decision records with confidence and signals |
| `controlled_loop_policy_proposals` | Policy proposal lifecycle (create/approve/reject/deactivate/rollback) |
| `controlled_loop_policy_snapshots` | Pre-mutation policy snapshots with SHA-256 safety hashes |

### Migration posture

- Schema versioned via numbered migrations in `engine/src/storage/local_product_store/migrations.rs`
- Current version: v13
- v12 adds `controlled_loop_policy_proposals`
- v13 adds `controlled_loop_policy_snapshots`
- PostgreSQL DDL supports fresh stores; parity with SQLite maintained
- Migrations require explicit human approval

---

## 8. Policy Control Loop

### Components

| Component | Module | Purpose |
|---|---|---|
| `PolicyProposer` | `engine/src/feedback/policy_proposer.rs` | Generates `ProposalCandidate` from patterns and simulation |
| `ProposalValidator` | `engine/src/feedback/proposal_validator.rs` | Validates proposal safety and evidence |
| `ProposalSerializer` | `engine/src/feedback/proposal_serializer.rs` | Serializes candidates to API/proposal format |
| `AutoAdjustmentPolicy` | `engine/src/feedback/auto_adjustment_policy.rs` | Evaluates candidate eligibility (confidence ≥ 0.85) |
| `AutoAdjustmentGuard` | `engine/src/feedback/auto_adjustment_guard.rs` | Enforces disabled/dry-run/active modes |
| `PolicySnapshotPreview` | `engine/src/feedback/policy_snapshot.rs` | Read-only deterministic preview |
| `PolicySnapshotRecord` | `engine/src/feedback/policy_snapshot.rs` | Persisted pre-mutation snapshot with SHA-256 hash |

### Flow

```
PatternDetector detects pattern
    │
    ▼
PolicySimulator simulates alternative policy
    │
    ▼
PolicyProposer generates ProposalCandidate
    │  - SafetyFlags: 7 invariants
    │  - CandidateEvidence: trace/pattern/simulation links
    │
    ▼
ProposalValidator validates
    │
    ▼
AutoAdjustmentPolicy.evaluate(candidate)
    │  - confidence ≥ 0.85?
    │  - source = "pattern_detector" or "simulation"?
    │  - safety flags all true?
    │  - target tier safe?
    │  - evidence quality sufficient?
    │
    ▼
AutoAdjustmentGuard.from_env()
    │  - disabled? → block
    │  - dry_run? → read-only preview only
    │  - active? → allow apply (with auth + confirmation)
    │
    ▼
PolicySnapshotRecord persisted
    │  - pre-mutation policy state
    │  - rollback target
    │  - evidence IDs
    │  - SHA-256 safety hash
    │
    ▼
POST /auto-adjustments/apply
    │  - team:admin auth
    │  - confirm_auto_adjustment=true
    │  - one candidate per request
    │  - no duplicate re-entry
    │  - no same-policy-key conflict
    │
    ▼
active_routing_policy() updated
    │
    ▼
Audit event recorded
```

### Hard boundaries

| Boundary | Rule |
|---|---|
| Safe tier-map override only | Cannot change provider, auth, security, deploy, or constraint settings |
| No provider/CLI/auth/security/deploy expansion | Policy changes limited to tier maps |
| No target repo writes | Policy cannot enable target repo writes |
| No release/tag/deploy | Policy cannot trigger releases |
| No background scheduling | No daemon or cron performs auto-adjustment |
| No batch auto-apply | One candidate per request, one active adjustment per policy key |
| No daemonized auto-adjustment loop | All adjustments are request-driven |

### Rollback safety

- Rollback requires `team:admin` auth + `confirm_auto_adjustment_rollback=true`
- Validates SHA-256 safety hash of persisted snapshot
- Rejects corrupted snapshots, non-active status, missing snapshots, stale proposal state
- Exact policy restoration from snapshot

---

## 9. Safety and Governance Boundaries

### Default posture

| Feature | Default state | Enable mechanism |
|---|---|---|
| Dispatch | **ON** | Always available |
| Provider execution | **OFF** | `ACP_ENABLE_PROVIDER_EXECUTION=1` |
| CLI execution | **OFF** | `ACP_ENABLE_CLI_EXECUTION=1` |
| Auto-adjustment | **OFF** | `ACP_ENABLE_AUTO_ADJUSTMENT=1` |
| Auto-adjustment dry-run | **OFF** | `ACP_ENABLE_AUTO_ADJUSTMENT=1` + `ACP_AUTO_ADJUSTMENT_DRY_RUN=1` |
| Auto-adjustment active | **OFF** | `ACP_ENABLE_AUTO_ADJUSTMENT=1` + `ACP_AUTO_ADJUSTMENT_ACTIVE=1` + dry-run unset |
| DB encryption | **OFF** | `ACP_DB_ENCRYPTION_KEY` set |
| Cost gates | **OFF** | `CostGateConfig::from_env()` active |

### Environment variable gates

| Variable | Purpose |
|---|---|
| `ACP_ENABLE_PROVIDER_EXECUTION` | Gates provider API calls |
| `ACP_ENABLE_CLI_EXECUTION` | Gates CLI executor invocation |
| `ACP_ENABLE_AUTO_ADJUSTMENT` | Gates auto-adjustment subsystem |
| `ACP_AUTO_ADJUSTMENT_DRY_RUN` | Enables dry-run mode (read-only) |
| `ACP_AUTO_ADJUSTMENT_ACTIVE` | Enables active mode (requires also enable) |
| `ACP_DB_ENCRYPTION_KEY` | Enables SQLite encryption at rest |
| `ACP_DATABASE_URL` | PostgreSQL connection (opt-in) |
| `ACP_HEALTH_ALERT_WEBHOOK_URL` | Health alert webhook |
| `ACP_CONTEXT_ASSEMBLY_ENABLED` | Gates context pack assembly |

### Auth requirements

| Action | Auth level |
|---|---|
| Basic dispatch | None |
| Policy proposal CRUD | None (create/list) |
| Policy approve/reject/deactivate/rollback | `team:admin` + configured auth |
| Auto-adjustment apply | `team:admin` + `confirm_auto_adjustment=true` |
| Auto-adjustment rollback | `team:admin` + `confirm_auto_adjustment_rollback=true` |
| Team management | Configured auth |
| API key management | Configured auth |
| Backup management | Configured auth |

### Governance rules

- **No direct main push.** All changes via PR.
- **PR review and CI expectations.** All CI jobs must pass before merge.
- **Human owner remains final decision maker** for high-risk changes (auth, security, provider, deploy, DB, release).
- **Snapshot-before-mutation.** Policy snapshots persisted before any auto-adjustment.
- **Rollback validation.** Hash-verified rollback to exact pre-mutation state.
- **Audit requirement.** All mutations recorded in audit log.

---

## 10. Executor and Provider Boundary

### ACP_EXECUTION_MODE

The `HybridExecutor` selects execution backend based on tier and complexity:

| Condition | Executor selected |
|---|---|
| Tier = `claude_code_cli` or `codex_cli` AND CLI registered | CLI executor |
| Complexity < threshold AND no `no_provider_call` constraint | Provider executor |
| Complexity ≥ threshold | CLI executor (claude_code_cli preferred, then codex_cli) |
| Fallback | Provider (if no constraint) → Noop default |

### Tick-level executor modes

When advancing workflow runs via `POST /workflow-runs/:id/tick`, the `executor` field selects the backend:

| Executor value | Backend | Gate |
|---|---|---|
| `"command"` | `CommandNodeExecutor` | None |
| `"fail"` | `FailNodeExecutor` | None |
| `"dynamic"` / `"dynamic_noop"` / `"dynamic_workflow"` | `DynamicWorkflowController` | None |
| `"claude_code_cli"` | `CliNodeExecutor` | `ACP_ENABLE_CLI_EXECUTION=1` |
| `"codex_cli"` | `CliNodeExecutor` | `ACP_ENABLE_CLI_EXECUTION=1` |
| (default) | Noop | None |

### Constraints

| Constraint | Enforcement |
|---|---|
| `no_provider_call` | Prevents provider executor selection |
| Provider execution gate | `ACP_ENABLE_PROVIDER_EXECUTION=1` required |
| CLI execution gate | `ACP_ENABLE_CLI_EXECUTION=1` required |
| Human review | Some tasks require human review regardless of executor |
| Cost complexity | High-complexity tasks prefer CLI executors |

### What is not allowed by default

- Real model-provider API calls (requires `ACP_ENABLE_PROVIDER_EXECUTION=1`)
- CLI executor invocation (requires `ACP_ENABLE_CLI_EXECUTION=1`)
- Autonomous code execution without human review
- Target repository writes from executor

---

## 11. Dashboard / SDK Boundary

### Dashboard (`dashboard/`)

- TypeScript SPA built with Bun + React
- Served via `build_axum_router_with_dashboard` (static SPA fallback)
- Provides operator visibility into dispatches, workflow runs, feedback, decisions, costs
- Dynamic Regulator subsection shows dispatch metrics
- **Not wired:** Auto-Adjustments tab (intentionally out of Phase 5 scope)

### TypeScript SDK (`sdk/typescript/`)

- Type-safe client for all API endpoints
- Includes methods for dispatch, feedback, simulation, proposals, workflow runs
- **Not wired:** Auto-adjustment endpoint methods (intentionally out of Phase 5 scope)

### Python SDK (`sdk/python/`)

- REST client for the API
- Pure stdlib, zero runtime dependencies
- Includes methods for dispatch, feedback, simulation, proposals

### What is NOT wired

- Dashboard Auto-Adjustments tab
- TypeScript SDK auto-adjustment methods
- Python SDK auto-adjustment methods
- Dashboard controls for active auto-adjustment
- SDK methods for auto-adjustment apply/rollback

---

## 12. Dormant / Reference-Only Modules

| Module | Path | Status | Why it matters | Activation requirement |
|---|---|---|---|---|
| PostgreSQL backend | `engine/src/storage/local_product_store/pg_backend.rs` | Feature-gated | Optional high-availability storage | `--features pg` + `ACP_DATABASE_URL` |
| CLI executors | `engine/src/cli/` | Env-gated | Claude Code / Codex execution | `ACP_ENABLE_CLI_EXECUTION=1` |
| Provider executor | `engine/src/provider/` | Env-gated | Real model API calls | `ACP_ENABLE_PROVIDER_EXECUTION=1` |
| Shadow router | `engine/src/feedback/shadow_router.rs` | Observational only | What-if routing analysis | Already active (read-only) |
| Circuit breaker | `engine/src/infrastructure/circuit_breaker.rs` | Active but runtime-dependent | Provider failure protection | Activates on provider failures |
| Health webhook | (via `ACP_HEALTH_ALERT_WEBHOOK_URL`) | Env-gated | Degradation alerting | Set webhook URL |
| DB encryption | (via `ACP_DB_ENCRYPTION_KEY`) | Env-gated | SQLite encryption at rest | Set encryption key |

### What NOT to do with dormant modules

- **Do not wire dormant modules** without explicit approval and Phase 6 scope
- **Do not remove dormant/reference modules** — they serve as implementation reference
- **Do not activate env-gated features** without understanding security implications
- **Do not enable PostgreSQL** without running integration tests (`ACP_TEST_DATABASE_URL`)
- **Do not enable provider execution** without provider credentials configured

---

## 13. Testing and CI Architecture

### Validation layers

| Layer | Command | What it protects |
|---|---|---|
| **Rust tests** | `cargo test -p engine` | Current engine Rust unit and integration test suite |
| **Rust formatting** | `cargo fmt --check` | Consistent code style |
| **Rust lints** | `cargo clippy -p engine --all-targets -- -D warnings` | Correctness and style warnings |
| **TypeScript tests** | `bun test` in `sdk/typescript/` | TypeScript SDK correctness |
| **TypeScript build** | `bun run build` in `sdk/typescript/` | SDK compilation |
| **Dashboard lint** | `bun run lint` in `dashboard/` | Dashboard code quality |
| **Dashboard build** | `bun run build` in `dashboard/` | Dashboard compilation |
| **Python SDK tests** | `uv run --no-project python -m pytest` in `sdk/python/` | Python SDK correctness |
| **Handoff guard** | `uv run --no-project python scripts/check_agent_handoff.py` | Entry document consistency |
| **Secret scan** | `uv run --no-project python scripts/acp_secret_scan.py` | No committed secrets |
| **Wire codegen drift** | `bash scripts/check_wire_codegen_drift.sh` | Wire types match codegen |
| **Rust-TS cutover** | `bash scripts/verify_rust_typescript_stack.sh` | Stack integration |
| **Docker build** | `docker compose build` | Container build |
| **Native runtime** | `scripts/smoke_native_runtime.py` | End-to-end smoke test |
| **PG integration** | `cargo test -p engine --features pg-tests` | PostgreSQL parity |
| **Git diff** | `git diff --check` | No unintended whitespace changes |

### CI jobs (`.github/workflows/tests.yml`)

| Job | Scope |
|---|---|
| `python-tests` | Wire codegen, security baseline, utility tests, Python SDK |
| `rust-tests` | `cargo test`, `cargo fmt`, `cargo clippy` |
| `pg-integration-tests` | PostgreSQL 16 integration (requires `ACP_TEST_DATABASE_URL`) |
| `typescript-tests` | TS SDK tests, SDK build, dashboard lint + typecheck + build |
| `native-runtime` | Dashboard build, native engine build, smoke test |
| `rust-typescript-cutover` | Stack integration verification |
| `docker-build` | Docker compose build |

### Release CI (`.github/workflows/release.yml`)

- Triggered on `v*` tags
- Cross-platform binary builds (x86_64/aarch64 Linux, x86_64/aarch64 macOS)
- Packages binary + dashboard + `.env.example` into tarball with SHA-256
- Creates GitHub Release

---

## 14. Operational Playbooks

| Playbook | Location | Purpose |
|---|---|---|
| Phase 5 Active Trial | `docs/PHASE5_ACTIVE_TRIAL_PLAYBOOK.md` | Step-by-step drill for active auto-adjustment apply/rollback |
| Phase 5 Auto-Adjustment Audit | `docs/PHASE5_AUTO_ADJUSTMENT_AUDIT.md` | Audit details for Phase 5 implementation |
| Real-World Testing | `docs/REAL_WORLD_TESTING_PLAYBOOK.md` | PR flow, auto-merge, CI fix, pilot tasks |
| Runbook | `docs/RUNBOOK.md` | Operational runbook for system management |
| Phase 0-5 Completion Matrix | `docs/DYNAMIC_REGULATOR_PHASE_0_5_COMPLETION_MATRIX.md` | Detailed phase-by-phase completion evidence |

### Key operational rules

- **Before any active trial:** Verify clean main, green CI, handoff guard, secret scan, no pending high-risk PRs
- **During active trial:** Use isolated SQLite trial DB, follow step-by-step curl commands, verify all audit events
- **If trial fails:** Immediately disable active mode, do not proceed to seal
- **Rollback order:** Runtime rollback endpoint first, then code revert (PR #39 → #38 → #37)
- **Never revert blindly**

---

## 15. Architecture Risks and Open Questions

### Current risks

| Risk | Severity | Description |
|---|---|---|
| Docs/state drift | Medium | Documentation may diverge from code as changes accumulate |
| Phase 6 scope creep | Medium | Without clear boundaries, Phase 6 work could exceed safe scope |
| PostgreSQL active trial | Low | Not run (`ACP_TEST_DATABASE_URL` not set); parity unverified in production-like conditions |
| Dashboard/SDK mismatch | Low | Auto-adjustment endpoints exist but dashboard/SDK not wired |
| Dormant module activation | Low | Future agents might accidentally wire dormant modules |
| Policy mutation safety | Medium | Auto-adjustment changes routing policy; hash-validated rollback mitigates |
| Human approval burden | Medium | Every policy change requires admin review; could bottleneck high-volume scenarios |
| Migration parity risk | Low | SQLite and PostgreSQL schemas must stay in sync |
| Phase 0 observability gaps | Low | Phase 0 is PARTIAL: Phase 6 partially closed structured logging and per-decision observability gaps. Remaining gaps: aggregation, dashboard/reporting visibility, production operational dashboards |
| Candidate staleness | Low | Generated candidates have no timestamp; staleness is evidence-based |

### Open questions

1. Should Phase 6 include dashboard visibility for auto-adjustment state?
2. Is PostgreSQL active trial required before Phase 6 work begins?
3. Should auto-rollback on degradation be implemented (currently requires future approval)?
4. What is the right cadence for human approval of policy proposals in production-like usage?
5. Should remaining Phase 0 gaps (aggregation, dashboard visibility, production dashboards) be prioritized for Phase 7?

---

## 16. Phase 6: Operational Readiness and Observability — DONE

Phase 6 is COMPLETE (PRs #42–#47, completed 2026-06-12).

**Implemented:**
- Structured operational logs for dispatch/regulator decisions (tracing crate, correlation model)
- Per-decision observability for routing/policy/auto-adjustment paths
- Read-only operator visibility for regulator state (`GET /api/v1/regulator/state`)
- PostgreSQL active-trial status documented as BLOCKED (`ACP_TEST_DATABASE_URL` not available)
- Docs/architecture drift checks (schema version cross-check between `migrations.rs` and `ARCHITECTURE_BOOK.md`)
- Documentation consistency verified across Architecture Book, CURRENT_STATUS, NEXT_DECISION, DOCS_INVENTORY

**Not implemented (deferred to future phases):**
- Dashboard regulator visibility (Auto-Adjustments tab, policy lifecycle visualization)
- Production trial automation (automated active trial scripts)
- PostgreSQL active trial execution (blocked on `ACP_TEST_DATABASE_URL`)
- Policy lifecycle reporting (proposal approval/rejection rates)
- Phase 0 completion (structured logging infrastructure)

---

## Evidence Sources

This architecture book was generated from the following repository files:

- `docs/CURRENT_STATUS.md` — current state, verification, test counts
- `docs/NEXT_DECISION.md` — single forward plan, safety gates
- `docs/DYNAMIC_REGULATOR_PHASE_0_5_COMPLETION_MATRIX.md` — phase-by-phase evidence
- `docs/PHASE5_AUTO_ADJUSTMENT_AUDIT.md` — Phase 5 audit details
- `docs/PHASE5_ACTIVE_TRIAL_PLAYBOOK.md` — active trial drill and signoff
- `docs/MODULE_MAP.md` — module ownership and reachability
- `docs/DOCS_INVENTORY.md` — documentation classification
- `docs/SESSION_START_HERE.md` — project identity and milestones
- `engine/src/http_server/routes.rs` — API route table
- `engine/src/http_server/handlers/` — handler implementations
- `engine/src/storage/local_product_store/` — storage schema and methods
- `engine/src/feedback/` — feedback pipeline modules
- `engine/src/workflow/` — workflow controller and components
- `engine/src/executor/` — executor selection logic
- `scripts/check_agent_handoff.py` — handoff guard logic
- `scripts/acp_secret_scan.py` — secret scan logic
- `.github/workflows/tests.yml` — CI job definitions
- `.github/workflows/release.yml` — release CI
