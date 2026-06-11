# Dynamic Global Regulator — Real-World Testing Roadmap

Status: **ACTIVE — Real-World Testing Mode**

Created: 2026-06-11 | Revised: 2026-06-11

This document defines the roadmap for the Dynamic Global Regulator, validated through real-world testing. The regulator is not a distant planning exercise — it is the active direction, validated through real tasks, real branches, real commits, real PRs, real CI, and gated auto-merge.

This is **controlled autonomy**, not unrestricted autonomy. Safety gates remain mandatory. The human operator approves high-risk decisions; the system handles low-risk work autonomously.

---

## 1. Current System Baseline

### What exists today

The dispatch kernel is a deterministic, rule-based pipeline:

```
Request → TaskAnalyzer → ModelSelector → BudgetManager → DispatchDecision → Executor → Evaluator → Ledger
```

**Dispatch Kernel (Phases 1–7, STABLE)**
- `RuleBasedTaskAnalyzer`: static keyword/phrase/multiplier maps for domain, intent, complexity, risk, confidence, budgets
- `ModelSelector`: fixed tier mapping table (`code_generate`→`codex_cli`, `code_review`→`balanced_worker`, etc.) with complexity-based escalation (score ≥ 0.7 escalates to `claude_code_cli`)
- `BudgetManager`: token/cost budget reservation
- `EvaluationStub`: output checks (non-empty, substantial, no-error), pass/fail/needs_human_review classification
- `Quality Retry`: auto-upgrades tier on failure (cheap→balanced→claude_code_cli), constraint-aware, records upgraded tier in ledger
- `DispatchLedger`: event-sourced audit trail

**Dynamic Workflow (Batches 1–7, COMPLETE)**
- `DynamicWorkflowController`: observe→tick→evaluate→mutate loop
- Graph mutation: failed nodes trigger DAG modification (add fix/test nodes)
- Feedback-driven routing, dynamic decomposition (trigger-based), agent profiles, tool registry
- Scheduler dynamic mode integration

**HybridExecutor (PR #25, COMPLETE)**
- `ACP_EXECUTION_MODE`: `off` (default/noop), `provider`, `cli`, `auto` (hybrid)
- Complexity-based routing: low→Provider API, high→CLI executor
- Constraint guards, auth gates, quality retry with tier upgrade

**Macro-Orchestrator (Phases 1–5, COMPLETE)**
- `OrchestrationDecision` records on all tick paths
- `ExecutorPool` with acquire/release, failure scoring
- Queue/priority/backpressure
- Decision trace/explainability
- Ops soak drill

### Current limits

| Dimension | Limit |
|-----------|-------|
| Task analysis | Static keyword maps, no learning from history |
| Tier routing | Fixed mapping table, complexity escalation is threshold-only |
| Decomposition | Single-layer, no recursive sub-task generation |
| Cross-node context | Each node executes independently; no output→input propagation |
| Feedback | Records outcomes, diagnostic only; does not modify future routing |
| Resource view | Per-pool, no cross-run global awareness |
| Adaptation | FeedbackIntegrator + AutoPolicies exist but produce recommendations only |

---

## 2. Target Concept

The Dynamic Global Regulator is a **policy and control layer** that sits above the existing dispatch kernel and dynamic workflow executor. It observes system-wide state, makes routing/resource/context decisions, learns from outcomes, and proposes (or, under strict guards, enforces) policy adjustments.

### Scope of regulation

| Domain | What the regulator controls |
|--------|----------------------------|
| Task analysis | Confidence calibration, complexity re-estimation from historical outcomes |
| Model/tier routing | Tier selection weighted by historical cost-of-pass, success rate, latency |
| Context assembly | Cross-node output propagation, context budget allocation, relevance filtering |
| Executor pools | Global capacity view, load-aware scheduling, failure-rate-based pool rebalancing |
| Workflow DAG mutation | Mutation strategy selection, depth limits, recovery pattern recognition |
| Feedback ledger | Replayable run traces, outcome attribution, pattern detection |
| Cost/budget control | Dynamic budget reallocation, cost forecasting, threshold adjustment |
| Approval/audit | Human-in-the-loop gates, audit evidence enrichment, rollback triggers |

### What it is NOT

- Not an autonomous agent — it does not execute tasks
- Not a model provider — it routes to existing executors
- Not a replacement for the dispatch kernel — it enriches decisions made by the kernel
- Not unrestricted — all high-risk changes require human approval

---

## 3. Real-World Testing Mode

The regulator will be validated through real work, not synthetic benchmarks. Every phase must prove itself on real tasks, real branches, real commits, real PRs, and real CI.

### Allowed by default (under safety gates)

| Action | Gate |
|--------|------|
| Branch creation | Any task that needs isolation |
| Target repo edits through branch + PR workflow | CI must pass before merge |
| Commits | Must include meaningful change, pass fmt/clippy |
| PR creation | Auto-created for any non-trivial change |
| CI triggering and CI repair | CI failures must be fixed before merge |
| Docs/tests/small code fixes | Low-risk, auto-merge eligible |
| Dynamic workflow adding fix/test nodes during real task runs | Graph mutation within existing bounds |
| Low-risk auto-merge after CI green | See Auto-Merge Policy below |

### Requires safety gate / explicit approval

| Action | Requirement |
|--------|-------------|
| Provider/CLI execution boundary expansion | Explicit user approval |
| Auth/security boundary changes | Explicit user approval |
| Database migrations | Explicit user approval |
| Release/tag/deploy | Explicit user approval |
| Active YAML/rubric/policy mutation | Explicit user approval |
| Destructive or irreversible operations | Explicit user approval |

---

## 4. Safety Gates That Remain Mandatory

These gates are non-negotiable and apply to all phases:

1. **No secrets committed** — API keys, tokens, passwords must never appear in tracked files. `scripts/acp_secret_scan.py` enforced.
2. **No merge on failing CI** — all CI jobs must pass before any merge.
3. **No unlogged execution** — every dispatch, execution, and policy change must be recorded in the ledger.
4. **Rollback path required** — every change must have a documented or automated rollback path.
5. **Provider execution remains explicit/env-gated** — `ACP_ENABLE_PROVIDER_EXECUTION=1` required. Default remains off.
6. **CLI execution remains explicit/env-gated** — `ACP_ENABLE_CLI_EXECUTION=1` required. Default remains off.
7. **No automatic release/tag/deploy in Phase 1** — releases require explicit human approval.
8. **High-risk changes require human approval** — auth, security, database, provider, deploy, and policy boundary changes.
9. **Active YAML/rubric/policy mutation requires explicit approval** — CI config, governance rules, and routing policy are not auto-modifiable.
10. **Destructive or irreversible operations require human approval** — data deletion, schema drops, credential rotation.

---

## 5. Auto-Merge Policy

### Allowed auto-merge

A PR may be auto-merged when ALL of the following are true:

- Docs-only, tests-only, CI correctness fix, or small low-risk code fix
- All CI jobs pass (green)
- No secrets/auth/provider/deploy/release/db migration changes
- Clear rollback path (git revert is sufficient)
- No active YAML/rubric/policy mutation
- No destructive or irreversible operations

### Not allowed auto-merge

A PR must NOT be auto-merged when ANY of the following apply:

- Auth or security boundary changes
- Provider/CLI execution boundary expansion
- Database migrations
- Destructive operations (data deletion, schema drops)
- Release, tag, or deploy
- Active policy, rubric, or YAML mutation
- Failing or missing CI
- Unclear rollback path

### Auto-merge decision flow

```
PR created
  → CI runs
  → CI green?
    → No: fix and retry
    → Yes: check change classification
      → Low-risk (docs/tests/CI fix/small code): auto-merge
      → High-risk (auth/security/provider/db/deploy/policy): require human approval
```

---

## 6. Control Loop

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│   ┌─────────┐    ┌─────────┐    ┌──────┐    ┌──────────┐   │
│   │ OBSERVE │───▶│ DECIDE  │───▶│ ACT  │───▶│ EVALUATE │   │
│   └─────────┘    └─────────┘    └──────┘    └──────────┘   │
│        ▲                                         │          │
│        │           ┌───────────────┐             │          │
│        └───────────│ LEARN/RECOMMEND│◀────────────┘          │
│                    └───────┬───────┘                         │
│                            │                                 │
│                    ┌───────▼───────┐                         │
│                    │   GOVERN      │                         │
│                    │ (safety gate) │                         │
│                    └───────────────┘                         │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

**Phase progression:**

| Loop phase | Early phases (0–3) | Middle phases (4–5) | Late phases (6–7) |
|------------|-------------------|--------------------|--------------------|
| Observe | Collect metrics, build replayable traces | Same + pattern detection | Same + global resource view |
| Decide | Rule-based (existing kernel) | Rule-based + shadow recommendations | Rule-based + approved policy adjustments |
| Act | Execute via existing dispatch pipeline | Same | Same + resource-aware scheduling |
| Evaluate | Record outcomes in ledger | Same + outcome attribution | Same + recursive verification |
| Learn/Recommend | Log-only shadow proposals | Human-approved policy proposals | Limited auto-adjustment under guards |
| Govern | Safety gates enforced | Safety gates enforced | Safety gates + auto-rollback triggers |

**Key principle:** Safety gates are enforced at every phase. The difference between phases is the *scope of autonomous action*, not the *presence of safety controls*.

---

## 7. Required Subsystems

### 7.1 Context Continuity Layer

**Purpose:** Propagate outputs from completed nodes as inputs/context for downstream nodes.

**Components:**
- `ContextAssembler`: collects outputs from predecessor nodes, applies relevance filtering, fits within context budget
- `ContextBridge`: maps node output fields to successor node input fields based on DAG edges
- `ContextBudgetAllocator`: distributes total context budget across active nodes proportionally to complexity/relevance

**Current gap:** `ContextPack` has budget validation but no cross-node assembly. Each node runs in isolation.

### 7.2 Feedback Ledger (Enhanced)

**Purpose:** Transform the existing dispatch ledger into a replayable, queryable feedback store.

**Components:**
- `RunTraceRecorder`: captures full decision→execution→evaluation chain per run with timestamps
- `OutcomeAttributor`: attributes success/failure to specific decisions (tier, context, executor)
- `PatternDetector`: identifies recurring failure patterns (e.g., "cheap tier fails on architecture tasks")
- `CostOfPassCalculator`: computes actual cost-per-success per task class/tier combination

**Current gap:** Ledger records events but does not attribute outcomes to decisions or detect patterns.

### 7.3 Policy Simulation / Shadow Routing

**Purpose:** Run "what-if" simulations of alternative routing decisions without affecting live traffic.

**Components:**
- `ShadowRouter`: for each real dispatch, computes what the regulator *would* have chosen
- `PolicySimulator`: replays historical runs through candidate policies, measures outcome delta
- `SimulationReport`: compares policy A vs policy B on cost, success rate, latency, human review rate

**Current gap:** No shadow routing exists. `ModelSelector` makes one decision with no alternative comparison.

### 7.4 Adaptive Policy Proposal Engine

**Purpose:** Generate policy change proposals based on feedback analysis.

**Components:**
- `PolicyProposer`: reads feedback patterns, generates tier-weight/context-budget/retry-threshold proposals
- `ProposalValidator`: checks proposals against safety constraints
- `ProposalSerializer`: produces human-readable proposal documents with evidence and expected impact

**Current gap:** `AutoDowngradePolicy`/`AutoUpgradePolicy` produce binary recommendations; no structured proposal system.

### 7.5 Global Resource View

**Purpose:** Cross-run awareness of executor pool state, queue depth, and system load.

**Components:**
- `ResourceMonitor`: aggregates executor pool metrics across all active runs
- `LoadBalancer`: distributes work based on pool capacity, failure rates, and cooldown state
- `CapacityForecast`: predicts resource exhaustion based on current queue depth and arrival rate

**Current gap:** `ExecutorPool` is per-pool; no cross-pool or cross-run visibility.

### 7.6 Recursive Planning Layer

**Purpose:** Multi-level task decomposition — broad tasks decompose into sub-tasks, which decompose further.

**Components:**
- `RecursiveDecomposer`: extends `Decomposer` trait with configurable max depth
- `DecompositionBudget`: allocates planning budget across depth levels
- `AbstractionLevel`: tags nodes with abstraction depth to prevent infinite recursion

**Current gap:** `TaskDecomposer` produces single-layer flat graphs. `DynamicDecomposer` triggers are stub-level.

### 7.7 Governance / Approval / Rollback Layer

**Purpose:** Human-in-the-loop controls for policy changes, with rollback capability.

**Components:**
- `PolicyApprovalGate`: requires human sign-off before any policy change takes effect
- `RollbackManager`: snapshots policy state before changes, enables one-click rollback
- `AuditTrail`: records all policy proposals, approvals, rejections, and rollbacks with evidence

**Current gap:** No policy mutation exists, so no governance/rollback is needed yet. This subsystem must be built *before* any automatic adjustment capability.

---

## 8. Phased Roadmap

### Phase 0: Baseline Documentation and Observability

**Goal:** Establish the measurement foundation — know what we're measuring before changing anything.

**Implementation scope:**
- Document current dispatch pipeline decision points and their inputs/outputs
- Add structured logging for: tier selection rationale, complexity score, constraint matches, retry triggers, evaluation outcomes
- Create `docs/DISPATCH_OBSERVABILITY.md` defining metrics taxonomy
- Add `/api/v1/dispatch-metrics` endpoint exposing aggregated dispatch statistics (by tier, by task class, success rate, cost)
- Dashboard Metrics tab with dispatch outcome distribution

**Non-goals:**
- No routing changes
- No feedback loop closure
- No new executors or providers

**Acceptance tests:**
- Every dispatch decision logs: selected_tier, complexity_score, hard_constraints, retry_count, evaluation_status
- `/api/v1/dispatch-metrics` returns correct aggregates for last N dispatches
- Dashboard renders tier distribution and success rate charts

**Safety gates:**
- Read-only metrics endpoint (no mutation)
- No dispatch behavior change
- Metrics are diagnostic only

**Rollback strategy:** Remove endpoint and dashboard component. No behavioral change to roll back.

---

### Phase 1: ContextPack Cross-Node Assembly

**Goal:** Enable completed nodes to pass outputs as context to downstream nodes.

**Implementation scope:**
- `ContextAssembler` collects outputs from predecessor nodes (by DAG edge)
- `ContextBridge` maps output fields to input fields using edge metadata
- `ContextBudgetAllocator` distributes context budget across active nodes
- Integrate into `WorkflowEngine::tick()` — before executing a node, assemble context from predecessors
- Add context_injection metadata to `WorkflowRunNode` for auditability

**Non-goals:**
- No relevance filtering (use all predecessor output)
- No context compression or summarization
- No learning which context is useful

**Acceptance tests:**
- Node B with edge A→B receives A's output as context input
- Context budget enforcement: if total predecessor output exceeds budget, truncate oldest first
- Context injection recorded in node execution metadata
- No context propagation for nodes with no predecessor edges

**Safety gates:**
- Context injection is additive — does not replace existing node input
- Budget enforcement prevents unbounded context growth
- No provider calls triggered by context content

**Rollback strategy:** Disable `ContextAssembler` integration in `WorkflowEngine::tick()`. Nodes revert to isolated execution.

---

### Phase 2: Feedback Ledger and Replayable Run Traces

**Goal:** Transform the event-sourced ledger into a queryable feedback store with outcome attribution.

**Implementation scope:**
- `RunTraceRecorder` captures decision→execution→evaluation chain as structured trace
- `OutcomeAttributor` links success/failure to: selected_tier, complexity_score, context_size, executor_type
- `CostOfPassCalculator` computes actual cost-per-success per task class and tier
- `/api/v1/feedback/traces` endpoint for querying historical traces
- `/api/v1/feedback/cost-of-pass` endpoint for cost-of-pass statistics
- Dashboard Feedback tab with trace browser and cost-of-pass charts

**Non-goals:**
- No automatic policy adjustment based on feedback
- No pattern detection (Phase 3)
- No "what-if" simulation

**Acceptance tests:**
- After N dispatches, `/api/v1/feedback/traces` returns N traces with decision+execution+evaluation fields
- Cost-of-pass calculation: `total_cost / success_count` per task_class × tier
- Traces are replayable: given the same inputs, the same decision chain is reconstructable
- Outcome attribution correctly links failures to specific decision factors

**Safety gates:**
- Read-only endpoints
- Feedback data does not influence dispatch decisions
- No mutation of active routing

**Rollback strategy:** Remove feedback endpoints. Existing dispatch ledger remains unchanged.

---

### Phase 3: Shadow Adaptive Policy Simulation

**Goal:** Compare "what the regulator would do" against "what the kernel does" without affecting live traffic.

**Implementation scope:**
- `ShadowRouter` computes alternative tier/context decisions for every real dispatch
- `PolicySimulator` replays last N traces through candidate policies
- `SimulationReport` compares: success rate delta, cost delta, latency delta, human review rate delta
- `/api/v1/simulation/report` endpoint
- Dashboard Simulation tab with before/after comparison

**Non-goals:**
- Shadow decisions do NOT replace real decisions
- No automatic policy mutation
- No live traffic routing changes

**Acceptance tests:**
- For every real dispatch, shadow decision is recorded alongside real decision
- Simulation replay produces correct delta metrics
- Shadow routing never influences actual dispatch outcome
- Simulation report clearly labels "shadow" vs "real"

**Safety gates:**
- Shadow routing is fire-and-forget — results are logged, never acted upon
- No code path allows shadow decision to override real decision
- Simulation is read-only analysis

**Rollback strategy:** Remove `ShadowRouter` integration. Dispatch pipeline unchanged.

---

### Phase 4: Human-Approved Policy Proposals

**Goal:** Generate structured policy change proposals that require human approval before activation.

**Implementation scope:**
- `PolicyProposer` reads feedback patterns and shadow simulation results, generates proposals
- `ProposalValidator` checks proposals against safety constraints
- `ProposalSerializer` produces human-readable proposal with evidence, expected impact, and rollback plan
- `/api/v1/proposals` CRUD endpoints (create/list/approve/reject)
- Dashboard Proposals tab with evidence visualization and approve/reject buttons
- Approved proposals modify a `policy_overrides` table in SQLite, read by `ModelSelector` at dispatch time

**Non-goals:**
- No automatic proposal generation on a schedule
- No self-modifying policy — approved proposals are additive overrides only
- No proposal that violates existing hard_constraints

**Acceptance tests:**
- PolicyProposer generates proposal with evidence from feedback traces
- Proposal includes: current policy, proposed change, expected impact, rollback plan
- Approval flow: pending → approved → active (applied to ModelSelector)
- Rejection flow: pending → rejected (no effect)
- Approved override is read by ModelSelector on next dispatch
- Rollback: deactivating override restores original ModelSelector behavior

**Safety gates:**
- All proposals require explicit human approval (HTTP POST with confirmation)
- Proposals cannot violate hard_constraints
- Override is additive — cannot remove existing safety constraints
- Audit trail for all proposal lifecycle events

**Rollback strategy:** Deactivate all overrides in `policy_overrides` table. ModelSelector reverts to default behavior.

---

### Phase 5: Limited Automatic Adjustment Under Strict Guards

**Goal:** Allow a narrow class of policy adjustments to apply automatically, with strict guardrails.

**Implementation scope:**
- Define `AutoAdjustmentPolicy` — whitelist of adjustment types allowed without human approval
- `AutoAdjustmentGuard` enforces: adjustment magnitude limit, rate limit, rollback trigger
- `PolicySnapshot` captures full policy state before each auto-adjustment
- `/api/v1/auto-adjustments` endpoint showing adjustment history and current state
- Dashboard Auto-Adjustments tab with timeline and rollback controls

**Non-goals:**
- No unbounded automatic adjustment
- No adjustment of safety constraints, auth gates, or approval requirements
- No automatic adjustment of provider/CLI execution boundaries
- No self-modifying code or routing algorithm changes

**Acceptance tests:**
- Auto-adjustment fires when trigger condition met
- Adjustment magnitude stays within bounds
- Rate limit enforced
- Rollback trigger: if success rate drops after adjustment, auto-revert
- All adjustments recorded with before/after state and trigger evidence

**Safety gates:**
- Whitelist-only adjustment types
- Magnitude caps
- Rate limits
- Auto-rollback on degradation
- Human override: any adjustment can be manually reverted via API/dashboard
- No adjustment to safety boundaries

**Rollback strategy:** `PolicySnapshot` restore to pre-adjustment state. Disable auto-adjustment feature via `ACP_ENABLE_AUTO_ADJUSTMENT=0`.

---

### Phase 6: Global Resource-Aware Scheduling

**Goal:** Schedule dispatches based on system-wide resource state, not per-pool decisions.

**Implementation scope:**
- `ResourceMonitor` aggregates executor pool metrics across all active runs
- `LoadBalancer` assigns dispatches to pools based on global capacity view
- `CapacityForecast` predicts resource exhaustion from queue depth and arrival rate
- Backpressure signal integration
- `/api/v1/resources/global` endpoint with system-wide resource view
- Dashboard Resources tab with pool utilization heatmap and capacity forecast

**Non-goals:**
- No new executor types
- No cloud/scale-out resource provisioning
- No priority preemption

**Acceptance tests:**
- Global resource view correctly aggregates pool states across concurrent runs
- Load balancer distributes work proportionally to available capacity
- Backpressure activates when global utilization exceeds threshold
- Capacity forecast predicts exhaustion within ±20% accuracy

**Safety gates:**
- Backpressure only delays dispatches, never drops them
- No dispatch is rejected due to resource constraints
- Pool rebalancing cannot move dispatches to pools that violate hard_constraints

**Rollback strategy:** Revert to per-pool `ExecutorPool` scheduling. Remove global resource aggregation.

---

### Phase 7: Recursive Decomposition and Planning

**Goal:** Enable multi-level task decomposition.

**Implementation scope:**
- `RecursiveDecomposer` extends `Decomposer` trait with configurable max depth
- `DecompositionBudget` allocates planning budget across depth levels
- `AbstractionLevel` tags nodes with depth to prevent infinite recursion
- Integration with `DynamicDecomposer`: when a node fails, the decomposer can propose sub-decomposition
- `DecompositionVerifier` checks that leaf nodes are concrete enough for execution

**Non-goals:**
- No LLM-based planning (decomposition remains rule-based)
- No unbounded recursion (hard depth limit, default 3)
- No decomposition of already-executing nodes

**Acceptance tests:**
- Broad task decomposes into 2+ sub-tasks, each sub-task decomposes further (depth 2)
- Depth limit enforced
- Decomposition budget prevents excessive planning cost
- Leaf nodes pass concrete-execution check
- Failed sub-task triggers re-decomposition at its level, not parent level

**Safety gates:**
- Hard depth limit (configurable, default 3, max 5)
- Decomposition budget cap
- No decomposition of safety-critical nodes
- Recursive decomposition requires explicit opt-in (`ACP_ENABLE_RECURSIVE_DECOMPOSITION=1`)

**Rollback strategy:** Disable recursive decomposition. `DynamicDecomposer` reverts to single-layer proposals.

---

## 9. Safety Boundaries

### What is NOT approved

This plan does NOT approve:

- Default-on provider/model calls (provider execution remains `ACP_ENABLE_PROVIDER_EXECUTION=1` gated)
- Target repo writes outside branch + PR workflow
- Sandbox/process/container/VM expansion
- Autonomous workers without safety gates
- Deploy/merge/release controls without human approval
- Active YAML/rubric/policy mutation without explicit approval
- Bypassing human approval for safety-critical decisions

### Invariant

The human operator remains the final decision-maker for all high-risk policy changes, execution authorization, and safety boundary modifications. Low-risk work (docs, tests, CI fixes, small code fixes) may proceed autonomously under the auto-merge policy.

---

## 10. Dependency Graph

```
Phase 0 (Observability)
    │
    ▼
Phase 1 (Context Assembly) ──────────────────────┐
    │                                             │
    ▼                                             │
Phase 2 (Feedback Ledger)                         │
    │                                             │
    ▼                                             │
Phase 3 (Shadow Simulation)                       │
    │                                             │
    ▼                                             │
Phase 4 (Human-Approved Proposals)                │
    │                                             │
    ▼                                             ▼
Phase 5 (Limited Auto-Adjustment)    Phase 6 (Global Resource View)
    │                                             │
    └──────────────┬──────────────────────────────┘
                   │
                   ▼
         Phase 7 (Recursive Decomposition)
```

Phases 0–4 are strictly sequential. Phases 5 and 6 can proceed in parallel after Phase 4. Phase 7 depends on both 5 and 6.

---

## 11. Open Questions

1. **Phase 5 scope:** Which adjustment types should be whitelisted for automatic application? Tier weight changes are low-risk; context budget changes are medium-risk.

2. **Phase 6 interaction with existing backpressure:** The current backpressure is single-queue. Global resource-aware scheduling requires rethinking the queue model.

3. **Phase 7 depth limit:** Default 3, max 5. Is 3 sufficient for real-world tasks? Need empirical data from Phase 2 feedback ledger.

4. **Cross-run context:** Should context from a completed run be available to a different run?

5. **Provider cost forecasting:** Phase 6 capacity forecasting needs cost projections. Should cost-of-pass data from Phase 2 feed into capacity planning?
