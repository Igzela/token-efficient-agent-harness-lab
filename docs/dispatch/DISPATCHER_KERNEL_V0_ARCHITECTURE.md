# Token-Efficient Agent Harness: Global System Dispatcher Architecture Book

> Schema version: architecture_book.v1
> Status: Draft architecture book (Claude Code <-> GPT collaborative)
> Coverage: Phase 0 baseline, Phase 1 detailed, Phase 2-7 outlined
> Scope: Full vision from Phase 0 (current) through Phase 7 (ecosystem)

---

## 0. Document Orientation

### 0.1 Purpose

This document is the master architecture book for the Token-Efficient Agent Harness project. It defines the complete journey from the current control-plane harness to a full Global System Dispatcher/Orchestrator.

It is NOT a v0 implementation document. It defines the full phase roadmap. Phase 1 is implementation-ready; Phase 2–7 contain actionable outlines that require phase-specific expansion before implementation.

### 0.2 Current Project Status

Phase 0 is complete. The harness has:
- 5 managed target repos (alters-lab, hermes-gateway-lab, simple-api-lab, cli-tool-lab, infra-config-lab)
- 914 passing tests
- Full audit, plan generation, triage, governance, error taxonomy
- Local app server with 13+ API endpoints
- Target onboarding protocol validated across API/CLI/infra project types

### 0.3 Relationship to Existing Documents

| Document | Relationship |
|----------|-------------|
| AGENTS.md | Agent behavior rules |
| docs/ROADMAP.md | High-level roadmap |
| docs/CURRENT_STATUS.md | Current state snapshot |
| docs/NEXT_DECISION.md | Immediate next decision |
| This document | Master architecture, all phases |

### 0.4 Terminology

- **Dispatcher**: The system that analyzes tasks, selects models, manages budgets, and orchestrates execution
- **Control Plane**: Governance, audit, triage, error taxonomy (Phase 0)
- **Dispatcher Kernel**: Task analysis + dispatch decision loop (Phase 1)
- **Execution Plane**: Provider adapters, sandbox, workers (Phase 3+)
- **Evaluation Plane**: Quality assessment, cost-of-pass, feedback loops
- **Cost-of-pass**: Total cost divided by successful completions for a task group

### 0.5 Phase Maturity Levels

| Level | Meaning |
|-------|---------|
| Experimental | Design only, no implementation |
| Alpha | Core implementation, limited fixtures |
| Beta | Comprehensive tests, real fixtures |
| Stable | Production-ready for its scope |
| Deprecated | Superseded by later phase |

### 0.6 Global Boundaries

**Research system** (current): Local, deterministic, non-executable, auditable
**Dispatcher** (Phase 1): Non-executable dispatch loop (analyze, decide, record)
**Dispatcher** (Phase 2): Manual external execution bridge (human as executor)
**Dispatcher** (Phase 3-4): Provider execution + adaptive routing
**Dispatcher** (Phase 5): Multi-agent execution orchestration
**Production system** (Phase 6+): Service-grade, multi-tenant, observable

### 0.7 Completeness Matrix

| Phase | Goal | Architecture | Schema | Tasks | Tests | Status |
|-------|------|-------------|--------|-------|-------|--------|
| 0 | complete | partial | existing | complete | complete | baseline |
| 1 | complete | detailed | detailed | detailed | detailed | stable |
| 2 | complete | detailed | detailed | detailed | detailed | stable |
| 3 | complete | detailed | detailed | detailed | detailed | stable |
| 4 | complete | detailed | detailed | detailed | detailed | stable |
| 5 | complete | detailed | detailed | detailed | detailed | stable |
| 6 | defined | outlined | outlined | outlined | outlined | needs expansion |
| 7 | defined | outlined | outlined | outlined | outlined | needs expansion |

---

## 1. Vision: Global System Dispatcher / Orchestrator

### 1.1 Ultimate Goal

Build a system that can:
- Receive task requests from any source (CLI, API, agent, workflow)
- Analyze task characteristics and complexity
- Select the optimal model/strategy from available options
- Dynamically manage token budgets and costs
- Execute tasks and evaluate quality
- Adjust strategy based on result feedback
- Maintain complete audit evidence chains and safety boundaries

### 1.2 Problem Being Solved

Current AI agent workflows are ad-hoc: each agent hardcodes its model choice, has no budget awareness, no quality feedback, and no audit trail. The harness solves this by providing a centralized dispatch layer that makes these decisions systematically.

### 1.3 Core Capability Loop

```
Request → Analyze → Select → Budget → Execute → Evaluate → Learn → Adjust
```

### 1.4 Differentiation from Agent Frameworks

| Aspect | Agent Framework | This System |
|--------|----------------|-------------|
| Model selection | Hardcoded or simple | Data-driven, budget-aware |
| Cost tracking | None or per-call | Aggregated cost-of-pass |
| Quality feedback | Manual | Automated + human-in-loop |
| Audit trail | Logs | Structured evidence chain |
| Routing | Static | Adaptive based on history |

### 1.5 Relationship to Local Harness

The local harness (dashboard, app server) is the development interface. The dispatcher is the runtime engine. They share schemas and data stores but serve different purposes.

### 1.6 Long-term Success Criteria

1. For any task type, the system can select a model that achieves acceptable quality at minimum cost
2. Budget is enforced before execution, not after
3. Every dispatch decision is auditable and replayable
4. Quality feedback continuously improves routing decisions
5. Safety boundaries are never violated automatically

### 1.7 Global Safety Principles

1. **Evidence before action**: Every decision must have traceable evidence
2. **Budget before execution**: Reserve before you spend
3. **Human authority**: Humans can override any automated decision
4. **Boundary enforcement**: Provider/sandbox/target-write gates are hard blocks
5. **Replay capability**: Any dispatch can be replayed from its record

### 1.8 Global Non-Goals

The system must never become:
- An autonomous unrestricted executor (every execution needs a dispatch decision)
- A replacement for human approval on high-risk operations
- A universal model benchmark tool
- A credential manager (credentials are referenced, never stored)
- A target repo mutation engine (writes always require explicit approval)
- A production multi-tenant SaaS before Phase 6

### 1.9 Global Failure Modes

| Failure Mode | Detection | Mitigation |
|-------------|-----------|------------|
| Wrong model selected | Quality evaluation, cost-of-pass tracking | Fallback tier, shadow routing comparison |
| Budget exceeded | BudgetReservation status check | Pre-execution reservation, hard limits |
| Provider failed | Error taxonomy classification | Retry policy, fallback provider |
| Quality evaluator wrong | Human review calibration | LLM judge confidence threshold |
| Adaptive routing learned bad strategy | A/B experiment validation | Promotion gate, rollback plan |
| Agent loop escalates cost | Per-agent budget cap | Multi-agent budget management |
| Target repo mutation without approval | Governance gate check | target_write_gate hard block |
| Audit trail incomplete | DispatchRecord completeness check | Mandatory ledger write |

### 1.10 Global Safety Invariants

These are testable assertions that must hold at all times:

1. No provider call may occur without a DispatchDecision
2. No target write may occur without target_write_gate clearance
3. No adaptive route may become active without admitted evidence threshold
4. Every execution result must link to a DispatchRecord
5. BudgetReservation must exist before execution begins
6. Every DispatchDecision must include either at least one shadow route or a no_shadow_route_reason
7. No secret may appear in logs, audit events, or dispatch records
8. Human override must be possible at any stage of the dispatch pipeline

---

## 2. Overall Architecture

### 2.1 Layered Architecture Overview

```
┌─────────────────────────────────────────────┐
│           API / UI / SDK Plane              │  Phase 6-7
├─────────────────────────────────────────────┤
│        Governance & Safety Plane            │  Phase 0 (done)
├─────────────────────────────────────────────┤
│        Memory & Feedback Plane              │  Phase 4
├─────────────────────────────────────────────┤
│        Budget & Cost Plane                  │  Phase 1-3
├─────────────────────────────────────────────┤
│        Evaluation Plane                     │  Phase 1-4
├─────────────────────────────────────────────┤
│        Execution Plane                      │  Phase 2-5
├─────────────────────────────────────────────┤
│        Dispatcher Kernel                    │  Phase 1
├─────────────────────────────────────────────┤
│        Control Plane                        │  Phase 0 (done)
└─────────────────────────────────────────────┘
```

### 2.2 Control Plane (Phase 0 - Done)

Components: model_profiles, usage_ledger, shadow_routing, plan_triage, governance, error_taxonomy, resource_planner, kernel

### 2.3 Dispatcher Kernel (Phase 1)

Components: TaskAnalyzer, ModelSelector, BudgetManager, DispatchDecision, DispatchEngine, DispatchLedger

### 2.4 Execution Plane (Phase 2-5)

Components: NoopExecutor, MockExecutor, ManualExecutor, ProviderAdapter, SandboxAdapter, AgentOrchestrator

### 2.5 Evaluation Plane (Phase 1-4)

Components: EvaluationStub, LLMJudge, CostOfPassAggregator, QualityFeedbackLoop

### 2.6 Budget & Cost Plane (Phase 1-3)

Components: BudgetManager, BudgetReservation, UsageLedger bridge, CostOfPass analytics

### 2.7 Memory & Feedback Plane (Phase 4)

Components: Historical routing data, Cost-of-pass trends, Model performance profiles, Adaptive routing policies

### 2.8 Governance & Safety Plane (Phase 0 - Done)

Components: 5-gate pipeline, evidence packs, approval records, rollback plans

### 2.9 API / UI / SDK Plane (Phase 6-7)

Components: FastAPI server, Dashboard, CLI, SDK, Plugin system

### 2.10 End-to-End Data Flow

```
User Request
    │
    ▼
TaskAnalyzer ──→ TaskAnalysis
    │
    ▼
ModelSelector ──→ RouteDecision + ShadowRoutes
    │
    ▼
BudgetManager ──→ BudgetReservation
    │
    ▼
DispatchEngine ──→ DispatchDecision
    │
    ▼
ExecutorAdapter ──→ ExecutionResult
    │
    ▼
EvaluationStub ──→ EvaluationResult
    │
    ▼
DispatchLedger ──→ DispatchRecord ──→ UsageLedger bridge
```

### 2.11 End-to-End State Machine

```
[Request] → analyzing → analyzed → selecting → selected → budgeting → budgeted
    → dispatching → dispatched → executing → executed → evaluating → evaluated
    → [completed | failed | escalated | cancelled]
```

### 2.12 Global Schema Index

See Section 12 for complete schema registry.

---

## 3. Phase 0: Completed Control Plane

### 3.1 Completed Capabilities

| Component | File | Status |
|-----------|------|--------|
| Model profiles | model_profiles.py | Stable |
| Usage ledger | usage_ledger.py | Stable |
| Shadow routing | model_profiles.py | Stable |
| Routing experiments | routing.py | Stable |
| Plan triage | plan_triage.py | Stable |
| Error taxonomy | error_taxonomy.py | Stable |
| Governance | governance.py | Stable |
| Resource planner | resource_planner.py | Stable |
| App API | app_api.py | Stable |
| Kernel | kernel.py | Stable |
| Target onboarding | docs/onboarding/ | Stable |

### 3.2 Component Inventory

| Component | Purpose | Inputs | Outputs | Schemas | Reused by Phase |
|-----------|---------|--------|---------|---------|----------------|
| model_profiles.py | Model tier/profile metadata | Profile fixture | ModelHarnessProfile, ShadowRoutingRecommendation | model_harness_profile.v1 | Phase 1-4 |
| usage_ledger.py | Actual usage records | Execution result | UsageLedgerRow, CostOfPassAggregate | usage_ledger.v1 | Phase 1-4 |
| routing.py | Routing policy + experiments | Task type, policy config | RoutingPolicy, experiment results | routing_policy.v1 | Phase 1-4 |
| plan_triage.py | Deterministic plan triage | Plan portfolio | Triage result | plan_triage.v1 | Phase 1 optional |
| error_taxonomy.py | Error classification | Error context | ErrorDomain (10 domains) | error_taxonomy.v1 | All phases |
| governance.py | 5-gate activation pipeline | Activation request | Gate results | governance.v1 | All phases |
| resource_planner.py | Non-executable planning | PlanningTask | ResourcePlan | resource_plan.v1 | Phase 1 optional downstream |
| model_gateway.py | Model capability registry | Capability query | ModelCapabilityRegistry, StubModelProvider | model_gateway.v1 | Phase 3+ |
| kernel.py | Event store + projections | Events | Projections | kernel.v1 | All phases |
| app_api.py | Local API endpoints | HTTP requests | JSON responses | app_api.v1 | Phase 1 extends |

### 3.3 Phase 0 Evidence Index

| Trial | Document | Key Outcome |
|-------|----------|-------------|
| Trial 0 | docs/trials/ | Initial harness validation |
| Trial 1 | docs/trials/ | Multi-repo audit capability |
| Trial 2 | docs/trials/ | Plan generation and triage |
| Trial 3 | docs/trials/ | Multi-repo generalization across 5 target repos |

### 3.6 Phase 0 Capability Boundary

**Can do:**
- Audit target repos
- Onboard non-managed repos
- Generate non-executable plans
- Triage plan portfolios
- Track model profiles and costs
- Classify errors
- Enforce governance gates

**Cannot do:**
- Analyze arbitrary task requests
- Select models for tasks
- Execute tasks (even mock)
- Track real-time budget consumption
- Evaluate execution quality
- Adjust strategy based on feedback

### 3.7 Phase 0 Legacy Issues

- resource_planner is planning-only, not dispatch
- shadow_routing is advisory, not active
- app_api is function-based, not service-grade
- No dispatch decision loop exists

---

## 4. Phase 1: Dispatcher Kernel v0

### 4.1 Goal

Build an offline, deterministic, auditable dispatcher kernel that receives raw task requests, analyzes task characteristics, selects model tiers, reserves token/cost budgets, generates DispatchDecision, and forms complete DispatchRecord through noop/mock/manual executor.

### 4.2 Success Criteria

For at least 20 fixture requests, the system can stably:
1. Identify task_domain, task_intent, primary_task_type
2. Identify risk_flags including negated-risk semantics
3. Output four-dimensional complexity (cognitive/context/execution/ambiguity)
4. Synthesize 0-1 complexity_score
5. Estimate context_budget and execution_budget
6. Select selected_tier based on model_profiles and routing policy
7. Generate fallback_tier and diagnostic shadow_routes
8. Generate BudgetReservation
9. Generate DispatchDecision
10. Generate ExecutionResult via noop/mock/manual executor
11. Generate EvaluationResult via EvaluationStub
12. Write DispatchRecord
13. Bridge to UsageLedgerRow when appropriate
14. Trigger gates or escalation for low-confidence, high-risk, budget-constrained tasks
15. Never call real providers, execute sandbox, write target repos, or start autonomous workers

### 4.3 Core Components

New subpackage: `src/harness_core/dispatch/`

```
__init__.py
task_analyzer.py       # RuleBasedTaskAnalyzer
model_selector.py      # ModelSelector + shadow dual-track
budget_manager.py      # BudgetManager
dispatch_decision.py   # DispatchDecision + BudgetReservation schemas
dispatch_engine.py     # DispatchEngine orchestrator
executor_adapter.py    # NoopExecutor, MockExecutor, ManualExecutor
evaluation_stub.py     # EvaluationStub
dispatch_ledger.py     # DispatchRecord, DispatchLedger
```

### 4.4 TaskAnalyzer

RuleBasedTaskAnalyzer: pure rules, deterministic, testable, no model calls.

**Input:** raw_request, optional repo_context, optional user_constraints, optional target_repo_metadata

**Output:** TaskAnalysis

**Responsibilities:**
- What task type? What domain? What intent?
- How complex? Where are the risks?
- What capabilities needed? What budget?
- What confidence? What safe default when low confidence?

**Negation-aware detection:** "no target repo writes" must NOT be identified as target_write intent.

### 4.5 TaskAnalysis Schema

```yaml
schema_version: "task_analysis.v1"
analysis_id: str
raw_request_snapshot: str
request_source: cli | api | dashboard | agent | workflow | test_fixture
primary_task_type: str
task_domain: code | docs | config | infra | math | architecture | repo_ops | governance | other
task_intent: generate | review | debug | summarize | audit | plan | refactor | compare | explain | classify
risk_flags: [target_write, provider_call, sandbox_execution, deployment, secret_handling, destructive_operation, long_context, high_uncertainty]
complexity_score: float  # weighted composite
cognitive_complexity: float
context_complexity: float
execution_risk: float
ambiguity_score: float
required_capabilities: [str]
context_budget_estimate: int
execution_budget_estimate: int
quality_requirement: draft | standard | high | critical
risk_level: low | medium | high | critical
confidence: float
confidence_label: low | medium | high
uncertainty_reason: [str]
safe_default: str
escalation_trigger: str | null
positive_evidence: [Evidence]
negative_evidence: [Evidence]
features_detected: dict
analysis_method: "rule_only"
created_at: str

Evidence:
  feature: str
  text: str
  span: [int, int]
  polarity: positive | negative
  source: raw_request | repo_context | user_constraints | target_metadata
  rule_id: str | null
  confidence: float
  negation_scope: str | null  # explains why negative evidence was NOT applied
```

### 4.6 Complexity Score v0

All sub-scores in 0-1 range. Weights sum to 1.0.

```
complexity_score =
    0.35 * cognitive_complexity
  + 0.25 * context_complexity
  + 0.25 * execution_risk
  + 0.15 * ambiguity_score
```

Four dimensions:
- **cognitive_complexity**: reasoning, architecture, math, debug, tradeoff, multi-step planning
- **context_complexity**: input length, code blocks, repo context, multi-file, cross-module
- **execution_risk**: target write, provider, sandbox, deployment, secret, destructive
- **ambiguity_score**: missing output format, broad goals, conflicting signals, missing acceptance criteria

### 4.7 ModelSelector

Active decision maker. Parallel with shadow routing, not replacement.

**Selection priority:**
1. Hard constraints: context window, required capability, forbidden tools, governance boundary
2. task_domain / task_intent / risk_flags → routing policy
3. Budget policy
4. Confidence / ambiguity
5. Historical cost_of_pass (only if sample threshold met)

### 4.8 ShadowRoutes

Diagnostic/counterfactual, not active decision.

```yaml
tier: str
profile_id: str | null
reason: str
admission_scope: "diagnostic"
estimated_cost: float | null
expected_tradeoff: str
```

### 4.9 BudgetManager

Pre-execution budget reservation. Does not record actual spend (that's usage_ledger).

**Operations:**
- Generate BudgetReservation
- Check max_input_tokens / max_output_tokens
- Estimate expected_cost from selected tier
- Mark if approval needed
- Reconcile reservation vs actual after execution

### 4.10 BudgetReservation Schema

```yaml
schema_version: "budget_reservation.v1"
reservation_id: str
decision_id: str
currency: str  # e.g. "USD", "token"
pricing_snapshot_id: str | null  # Phase 3: locks pricing at decision time
pre_budget: int
reserved_input_tokens: int
reserved_output_tokens: int
reserved_total_tokens: int
reserved_cost: float
budget_policy_id: str | null
budget_gate: str | null
status: reserved | consumed | released | violated | expired
actual_usage_ref: str | null
budget_delta: int | null
budget_violation: bool
created_at: str
updated_at: str
expires_at: str | null
```

### 4.11 DispatchDecision Schema

Lightweight. References analysis_id + snapshot, not deep embedding.

```yaml
schema_version: "dispatch_decision.v1"
decision_id: str
analysis_id: str
analysis_snapshot: dict
selected_tier: str
selected_profile_id: str | null
fallback_tier: str
fallback_profile_id: str | null
shadow_routes: [ShadowRoute]
hard_constraints: [str]  # e.g. ["no_provider_call", "no_target_write"]
rejected_candidates: [RejectedCandidate] | null
no_shadow_route_reason: str | null
max_input_tokens: int
max_output_tokens: int
routing_reason: str
quality_requirement: draft | standard | high | critical
expected_quality_band: low | medium | high | unknown
confidence: float
confidence_label: low | medium | high
budget_reservation: BudgetReservation
execution_policy:
  executor_type: noop | mock | manual | provider
  execution_allowed: bool
  requires_human_review: bool
  max_retries: int
execution_gates: [ExecutionGate]
decision_status: decided | needs_approval | blocked | diagnostic_only
created_at: str

ExecutionGate:
  gate_id: str
  gate_type: budget | risk | boundary | confidence | manual_review | provider_disabled | sandbox_disabled | target_write
  severity: info | warning | block | critical
  reason: str
  evidence_refs: [str]
  clearance_required: none | human | governance | policy
  cleared: bool
  cleared_by: str | null
  cleared_at: str | null

RejectedCandidate:
  tier: str
  profile_id: str | null
  reason: str
  constraint_failed: str | null
  estimated_cost: float | null
```

### 4.12 Execution Gates

Lightweight dispatch gates (not full governance pipeline):

| Gate | Condition |
|------|-----------|
| budget_gate | Budget exceeded or approval needed |
| risk_gate | high/critical risk or destructive/provider/deploy |
| boundary_gate | provider/sandbox/target_write detected |
| confidence_gate | confidence < 0.5 |
| manual_review_gate | human_review_required |
| provider_disabled_gate | provider blocked in v0 |
| sandbox_disabled_gate | sandbox blocked |
| target_write_gate | target repo mutation detected |

Dispatch gates can upgrade to governance but cannot replace it.

### 4.13 DispatchEngine

Orchestrator combining all components.

**Main entry:** `dispatch(raw_request, options) -> DispatchRecord`

**Flow:**
1. Analyze request
2. Select model tier
3. Reserve budget
4. Create dispatch decision
5. Create dispatch record
6. Execute (noop/mock/manual)
7. Evaluate result
8. Update dispatch ledger

### 4.14 ExecutorAdapter

| Type | v0 Status | Behavior |
|------|-----------|----------|
| noop | enabled, default | Returns planned/not_executed |
| mock | enabled, explicit | Deterministic fake output |
| manual | enabled, explicit | Generates prompt pack for human |
| provider | reserved enum, disabled | Future: real model calls |

### 4.15 ExecutionResult Schema

```yaml
schema_version: "execution_result.v1"
result_id: str
dispatch_id: str
decision_id: str
executor_type: noop | mock | manual | provider
status: not_executed | preview_generated | mock_completed | manual_pending | manual_completed | failed
output: str | null
prompt_pack: dict | null
input_tokens: int | null
output_tokens: int | null
estimated_cost: float | null
latency_ms: int | null
error_domain: str | null
error_message: str | null
# Phase 3 nullable fields (declared early for forward compatibility)
provider_request_id: str | null
attempt_number: int | null
finish_reason: str | null
usage_source: estimated | provider_reported | tokenizer_estimated | null
created_at: str
```

### 4.16 ManualExecutor

Phase 2 bridge, but contract defined in Phase 1.

**Output:** prompt_pack, recommended_model_tier, budget_limit, expected_output_schema, evaluation_checklist, pasteback_instructions

### 4.17 EvaluationStub

Basic checks only, no quality judgment:
- schema_validity
- boundary_compliance
- output_present
- error_free
- human_review_required

### 4.18 EvaluationResult Schema

```yaml
schema_version: "evaluation_result.v1"
evaluation_id: str
dispatch_id: str
decision_id: str
execution_result_id: str
status: pass | fail | needs_human_review | not_evaluated
checks: [EvaluationCheck]
quality_score: float | null
requires_retry: bool
retry_reason: str | null
created_at: str

EvaluationCheck:
  check_id: str
  name: str
  status: pass | fail | warning | skipped
  reason: str
```

### 4.19 DispatchLedger

First-class citizen. Without ledger, no replayable dispatcher.

### 4.20 DispatchRecord Schema

```yaml
schema_version: "dispatch_record.v1"
dispatch_id: str
request_snapshot: str
task_analysis_id: str
decision_id: str
execution_result_id: str | null
evaluation_result_id: str | null
usage_ledger_row_id: str | null
budget_reservation_id: str | null
final_status: dispatched | executing | completed | failed | escalated | cancelled | not_executed
created_at: str
updated_at: str
```

### 4.21 UsageLedger Bridge

Only write to usage_ledger when execution_result has actual or estimated usage.

**Relationships:**
- BudgetReservation = pre-execution control
- UsageLedgerRow = post-execution fact
- CostOfPassAggregate = historical analytics

### 4.22 ResourcePlanner Relationship

ResourcePlanner is NOT legacy.

```
DispatchEngine = upstream dispatch decision layer
ResourcePlanner = existing downstream non-executable planning subsystem
```

DispatchEngine can call ResourcePlanner when selected_strategy needs a plan.

### 4.23 API Surface v0

| Endpoint | Purpose |
|----------|---------|
| POST /api/dispatches | Create dispatch decision |
| GET /api/dispatches/{dispatch_id} | Get dispatch record |
| POST /api/dispatches/{dispatch_id}/preview-execution | Preview execution |
| POST /api/dispatches/{dispatch_id}/manual-result | Submit manual result (Phase 2) |
| GET /api/dispatch-ledger | List dispatch records |

Note: `/api/dispatches` (plural) avoids path conflict with `/api/dispatch-ledger`. Phase 1 API is local app API / library wrapper, not production service API (Phase 6).

### 4.24 Fixture Set

20 fixture requests covering:
1. Low-risk summary
2. Document audit
3. Code review
4. Code generation
5. Debug
6. Architecture design
7. Math reasoning
8. Config review
9. Infra/deploy review
10. Provider boundary
11. Target write intent
12. Secret handling
13. Long context
14. Ambiguous request
15. Conflicting constraints
16. Read-only high-risk topic
17. Negated no-write
18. Negated no-execute
19. Budget-constrained task
20. High-quality critical task

### 4.25 Test Strategy

| Test File | Coverage |
|-----------|----------|
| test_dispatch_task_analyzer.py | Classification, risk flags, negation, confidence, complexity |
| test_dispatch_model_selector.py | Tier choice, fallback, shadow routes, budget filtering |
| test_dispatch_budget_manager.py | Reservation, violation, release, reconcile |
| test_dispatch_decision.py | Schema validation, gate generation, decision status |
| test_dispatch_executor_adapter.py | Noop/mock/manual result contracts |
| test_dispatch_evaluation_stub.py | Schema validity, boundary compliance, human review |
| test_dispatch_ledger.py | Record creation, update, lookup, replay |
| test_dispatch_engine.py | End-to-end fixture dispatch |

### 4.26 Phase 1 Does NOT Do

- Real provider/model calls
- Sandbox/process/container/VM execution
- Autonomous workers
- Target repo writes
- Real automatic retry
- Real automatic upgrade/downgrade execution
- LLM judge
- Production API
- Multi-tenant
- Persistent database

### 4.27 Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| TaskAnalyzer misclassification | 20+ fixtures, positive/negative evidence, low-confidence gate |
| complexity_score false precision | Four-dimensional scores + evidence, not just total |
| ModelSelector over-trusting cost_of_pass | Sample threshold |
| Executor name misleading | v0 default noop, provider disabled |
| BudgetReservation vs usage_ledger confusion | Pre-execution vs post-execution separation |
| Governance vs dispatch gate confusion | Lightweight gate + escalation mapping |
| v0 bloats into real execution | Explicit Phase 1 prohibitions |

### 4.28 Effort Level

Medium-large but controllable. Main complexity is fixtures and semantic stability, not code volume. Stabilize schemas, 20 fixtures, and expected decisions first.

---

## 5. Phase 2: Manual Execution Bridge

### 5.1 Goal

Connect the dispatcher to real human execution flow without provider integration. Human is the executor; the system generates prompts, collects results, evaluates, and records.

### 5.2 Success Criteria

- Dispatcher generates prompt packs with recommended model tier, token budget, expected schema, evaluation checklist
- Human manually calls model and pastes output back
- System evaluates pasted output and records in ledger
- Accumulates real cost-of-pass data from manual executions
- At least 10 manual pasteback executions recorded with valid ledger entries

### 5.3 Dependency from Phase 1

Requires: DispatchDecision, ExecutionResult (noop/manual), DispatchLedger, EvaluationStub all stable.

### 5.4 Core Components

| Component | Purpose |
|-----------|---------|
| PromptPackGenerator | Generate prompt packs from DispatchDecision |
| PastebackParser | Parse and validate human-pasted output |
| ManualEvaluator | Evaluate pasted output against checklist |
| ManualUsageBridge | Write manual usage estimates to UsageLedger |
| CostOfPassAccumulator | Aggregate cost-of-pass from manual runs |

### 5.5 Schemas

**PromptPack**
```yaml
schema_version: "prompt_pack.v1"
prompt_pack_id: str
dispatch_id: str
recommended_model_tier: str
recommended_profile_id: str | null
system_prompt: str
user_prompt: str
context_pack_refs: [str]
max_input_tokens: int
max_output_tokens: int
expected_output_schema: dict | null
forbidden_outputs: [str]
evaluation_checklist: [str]
pasteback_instructions: str
created_at: str
```

**PastebackSubmission**
```yaml
schema_version: "pasteback_submission.v1"
submission_id: str
dispatch_id: str
submitted_by: str
model_used: str | null
provider_used: str | null
raw_output: str
output_hash: str
claimed_input_tokens: int | null
claimed_output_tokens: int | null
claimed_cost: float | null
submitted_at: str
```

**ManualExecutionSession**
```yaml
schema_version: "manual_execution_session.v1"
session_id: str
dispatch_id: str
prompt_pack_id: str
status: created | prompt_generated | human_executing | result_submitted | evaluated | recorded
submission_id: str | null
evaluation_id: str | null
created_at: str
updated_at: str
```

### 5.6 Implementation Tasks

| Task | Description |
|------|-------------|
| P2-T1 | PromptPack schema + PromptPackGenerator |
| P2-T2 | ManualExecutionSession store |
| P2-T3 | PastebackParser (validate structure, hash, estimate tokens) |
| P2-T4 | ManualEvaluator (check against evaluation_checklist) |
| P2-T5 | UsageLedger manual bridge (write from PastebackSubmission) |
| P2-T6 | Cost-of-pass aggregation for manual executions |
| P2-T7 | Dashboard/API manual flow endpoints |
| P2-T8 | Security tests: no provider calls possible |

### 5.7 Test Strategy

- Prompt pack deterministic fixture tests (same dispatch → same prompt pack)
- Pasteback malformed output tests (invalid JSON, missing fields, too long)
- Manual usage estimate tests
- Human review required tests (high-risk tasks → manual_review_gate)
- Ledger write tests (manual result → UsageLedgerRow)
- No-provider-call guard tests (ensure no provider adapter invoked)

### 5.8 Promotion Gate

Phase 2 is complete when:
- 10+ manual pasteback executions recorded
- Ledger entries valid and linked to DispatchRecords
- Cost-of-pass aggregation produces meaningful data
- No provider calls detected in any test

### 5.9 Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Human pastes garbage output | PastebackParser validation, malformed output rejection |
| Inaccurate token/cost estimates | Mark as estimated, compare with provider-reported in Phase 3 |
| Low adoption (too much manual work) | Dashboard UX, pasteback instructions clarity |

### 5.10 Boundary

No provider integration. No automatic execution. Human is the executor. No real token counting (estimates only).

---

## 6. Phase 3: Real Provider Integration

### 6.1 Goal

Connect to real model providers with full audit, budget enforcement, and safety boundaries.

### 6.2 Success Criteria

- Can call OpenAI-compatible, Anthropic, and local model providers
- Credential boundary enforced (no secret leakage)
- Real token/cost tracking in usage_ledger
- Timeout, retry, fallback working
- LLM judge for quality evaluation (may be Phase 3B if scope requires)
- Provider audit events recorded

### 6.3 Dependency from Phase 2

Requires: Manual execution bridge stable, cost-of-pass data from manual runs available, PromptPack/Pasteback schemas validated.

### 6.4 Core Components

| Component | Purpose |
|-----------|---------|
| ProviderAdapter (abstract) | Provider-agnostic execution interface |
| OpenAICompatibleProvider | OpenAI API compatible provider adapter |
| AnthropicProvider | Anthropic API provider adapter |
| LocalModelProvider | Local/self-hosted model adapter |
| CredentialBoundary | Secret storage, redaction, scope enforcement |
| ProviderAuditRecorder | Record provider call events (no secrets) |
| RealCostCalculator | Compute real costs from provider usage reports |
| RetryFallbackManager | Timeout, retry with budget check, fallback routing |
| LLMJudge | Quality evaluation using model-as-judge |

### 6.5 Schemas

**ProviderConfig**
```yaml
schema_version: "provider_config.v1"
provider_id: str
provider_type: openai_compatible | anthropic | local
base_url: str
model_id: str
credential_ref: str  # reference, never the secret itself
timeout_ms: int
max_retries: int
rate_limit_policy_id: str | null
enabled: bool
created_at: str
```

**CredentialRef**
```yaml
schema_version: "credential_ref.v1"
credential_ref_id: str
storage_backend: env | file | keyring | vault
redacted_display: str  # e.g. "sk-***abc"
scope: str  # e.g. "provider:openai"
created_at: str
```

**ProviderAuditEvent**
```yaml
schema_version: "provider_audit_event.v1"
event_id: str
dispatch_id: str
provider_id: str
event_type: request_sent | response_received | error | timeout | retry | fallback
input_token_count: int | null
output_token_count: int | null
cost: float | null
currency: str | null
latency_ms: int | null
error_domain: str | null
# NEVER contains: raw prompt, raw response, secrets
redaction_status: redacted | not_applicable
created_at: str
```

**RetryPolicy**
```yaml
schema_version: "retry_policy.v1"
policy_id: str
max_retries: int
backoff_strategy: linear | exponential | none
base_delay_ms: int
max_delay_ms: int
retryable_error_domains: [str]
budget_check_per_retry: bool
```

### 6.6 Execution Boundaries (Hard Rules)

1. No provider call without DispatchDecision
2. No provider call if provider_disabled_gate uncleared
3. No raw prompt logging unless explicitly configured and redacted
4. No automatic retry loop without budget re-check
5. No LLM judge on private content unless allowed by policy
6. No secret in logs, audit events, or dispatch records

### 6.7 Provider Failure Taxonomy

Extends error_taxonomy.py with provider-specific domains:

| Domain | Examples |
|--------|---------|
| provider_auth | invalid key, expired token, scope denied |
| provider_rate_limit | RPM limit, TPM limit, concurrent limit |
| provider_timeout | connect timeout, read timeout, total timeout |
| provider_content_filter | safety filter, policy violation |
| provider_capacity | model overloaded, queue full |
| provider_response | malformed response, unexpected format |

### 6.8 LLM Judge Considerations

LLM Judge is complex enough to warrant separate treatment:
- Requires its own model call (costs tokens)
- Needs calibration against human judgments
- Must handle private content safely
- Should have confidence threshold for auto-accept

Recommendation: Phase 3A = provider execution, Phase 3B = LLM judge. Or at minimum, LLM judge gets its own gate.

### 6.9 Implementation Tasks

| Task | Description |
|------|-------------|
| P3-T1 | ProviderAdapter abstract interface |
| P3-T2 | OpenAICompatibleProvider implementation |
| P3-T3 | AnthropicProvider implementation |
| P3-T4 | LocalModelProvider implementation |
| P3-T5 | CredentialBoundary (env/file/keyring backends) |
| P3-T6 | Secret redaction pipeline |
| P3-T7 | ProviderAuditRecorder |
| P3-T8 | RealCostCalculator (pricing model) |
| P3-T9 | RetryPolicy + RetryFallbackManager |
| P3-T10 | Provider failure taxonomy integration |
| P3-T11 | LLMJudge (basic, with calibration) |
| P3-T12 | Real UsageLedger bridge from provider usage |
| P3-T13 | Security tests: no secret leakage |

### 6.10 Test Strategy

- Fake provider adapter tests (deterministic responses)
- Credential redaction tests (no secret in any output)
- Timeout tests (simulate slow providers)
- Retry budget exhaustion tests
- Fallback route tests (primary fails → fallback succeeds)
- Provider error taxonomy mapping tests
- Cost calculation tests (known input → expected cost)
- No-secret-in-logs tests
- LLM judge calibration tests

### 6.11 Promotion Gate

Phase 3 is complete when:
- Provider dry-run tests pass for all provider types
- One live smoke test with redaction and cost tracking passes
- Credential redaction tests pass (zero secret leakage)
- Retry/fallback works under simulated failure
- UsageLedger has real provider usage entries

### 6.12 Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Secret leakage in logs/audit | Mandatory redaction pipeline, security regression tests |
| Provider API changes | Abstract adapter interface, version pinning |
| Cost overrun with real providers | BudgetReservation enforced before every call |
| LLM judge bias | Calibration against human judgments, confidence threshold |
| Rate limiting cascades | Exponential backoff, per-provider rate limit policy |

### 6.13 Boundary

Provider calls are gated by dispatch decision. Budget enforced before execution. All calls audited. No raw prompts/responses in audit events. No autonomous retry without budget re-check.

---

## 7. Phase 4: Adaptive Routing

### 7.1 Goal

Upgrade routing from static rules to history-driven adaptive routing with evidence-based promotion.

### 7.2 Success Criteria

- System selects models based on historical cost-of-pass
- Shadow routing can promote to active routing with evidence threshold
- A/B routing experiments work
- Automatic downgrade/upgrade based on quality feedback
- Adaptive routing beats static baseline without quality regression

### 7.3 Dependency from Phase 3

Requires: Provider execution stable, real cost/quality data flowing into UsageLedger, EvaluationStub upgraded with real quality checks.

### 7.4 Core Components

| Component | Purpose |
|-----------|---------|
| CostOfPassRouter | Route based on historical cost-of-pass data |
| DynamicTierSelector | Select tier dynamically per task group |
| RoutingExperimentManager | A/B routing experiments with statistical rigor |
| PromotionGate | Gate shadow → active promotion with evidence threshold |
| AutoDowngradePolicy | Downgrade when quality risk is low |
| AutoUpgradePolicy | Upgrade when uncertainty/failure/critical task |
| FeedbackIntegrator | Feed quality results back into routing decisions |

### 7.5 Key Schemas

**RoutingExperiment**
```yaml
schema_version: "routing_experiment.v1"
experiment_id: str
name: str
task_group: str
arms: [RoutingArm]
status: created | running | concluded | rolled_back
start_time: str
end_time: str | null
conclusion: str | null
```

**RoutingArm**
```yaml
arm_id: str
experiment_id: str
tier: str
profile_id: str | null
traffic_weight: float
observations: [RoutingObservation]
```

**RoutingObservation**
```yaml
observation_id: str
arm_id: str
dispatch_id: str
task_domain: str
task_intent: str
selected_tier: str
baseline_tier: str | null
quality_score: float | null
cost: float
latency_ms: int
success: bool
failure_domain: str | null
budget_violation: bool
observed_at: str
```

### 7.6 Promotion Gate (Shadow → Active)

Shadow routing can only become active when ALL conditions met:
- minimum_sample_count >= 30 per task group
- quality_non_regression == true (no quality decrease)
- cost_reduction >= configurable threshold
- failure_rate_not_worse than current active routing
- human_review_approval_required for critical changes
- rollback_plan_exists

### 7.7 Cold-Start Strategy

| Scenario | Strategy |
|----------|----------|
| New task group | Static rules from model_profiles |
| New model profile | Shadow-only for minimum_sample_count |
| New provider | Dry-run first, then limited traffic |
| Low sample count | Conservative routing, no adaptation |
| Conflicting historical data | Human review required |
| Model pricing changed | Invalidate cached cost_of_pass, recalculate |
| Model behavior drift | Trigger re-evaluation, compare with baseline |

### 7.8 Auto Downgrade/Upgrade Boundaries

- Can downgrade only if quality risk is low
- Can upgrade if high uncertainty, failure, or critical task
- Cannot auto-upgrade across budget hard limit
- Cannot auto-route to provider not approved by governance

### 7.9 Implementation Tasks

| Task | Description |
|------|-------------|
| P4-T1 | CostOfPassRouter (historical data query) |
| P4-T2 | DynamicTierSelector |
| P4-T3 | RoutingExperimentManager (A/B framework) |
| P4-T4 | PromotionGate with evidence threshold |
| P4-T5 | AutoDowngradePolicy |
| P4-T6 | AutoUpgradePolicy |
| P4-T7 | FeedbackIntegrator (quality → routing) |
| P4-T8 | Cold-start strategy implementation |
| P4-T9 | Statistical evaluation tests |

### 7.10 Test Strategy

- A/B experiment simulation tests
- Promotion gate tests (insufficient evidence → blocked)
- Cold-start tests (new task group → static fallback)
- Auto downgrade/upgrade boundary tests
- Rollback tests (bad routing → revert)
- Statistical significance tests

### 7.11 Promotion Gate

Phase 4 is complete when:
- Adaptive routing beats static baseline in A/B test
- Promotion gate correctly blocks insufficient evidence
- Rollback works when routing degrades
- Cold-start strategy handles all scenarios

### 7.12 Boundary

Adaptive routing requires sufficient historical data. Cold-start uses static rules. Promotion requires admitted evidence, not just diagnostic. Rollback plan mandatory for any routing change.

---

## 8. Phase 5: Multi-Agent Orchestration

### 8.1 Goal

Extend single-task dispatch to multi-agent workflows with decomposition, parallel execution, and conflict resolution.

### 8.2 Success Criteria

- Tasks can be decomposed into sub-tasks with dependency graph
- Multiple agents can work in parallel
- Results are aggregated with conflict resolution
- Human approval checkpoints exist at critical junctions
- Multi-agent budget management prevents runaway costs
- Workflow state machine covers all terminal states

### 8.3 Dependency from Phase 3+4

Requires: Provider execution (Phase 3) + adaptive routing (Phase 4) both mature. Budget enforcement, evaluation, governance, and audit all stable.

### 8.4 Core Components

| Component | Purpose |
|-----------|---------|
| TaskDecomposer | Break task into sub-tasks with dependencies |
| AgentRoleRegistry | Define agent roles and capabilities |
| WorkflowEngine | Manage workflow state machine and execution |
| WorkQueue | Queue and dispatch sub-tasks to agents |
| DependencyResolver | Resolve dependency graph, determine execution order |
| ResultAggregator | Aggregate sub-task results into final result |
| ConflictResolver | Detect and resolve conflicts between agent outputs |
| HumanApprovalGate | Checkpoint for human approval at critical points |
| MultiAgentBudgetManager | Global + per-agent + per-node budget enforcement |

### 8.5 Key Schemas

**WorkflowGraph**
```yaml
schema_version: "workflow_graph.v1"
workflow_id: str
dispatch_id: str
nodes: [WorkflowNode]
edges: [WorkflowEdge]
status: created | decomposed | running | aggregating | completed | failed | cancelled
created_at: str
updated_at: str
```

**WorkflowNode**
```yaml
node_id: str
workflow_id: str
task_type: str
assigned_agent_id: str | null
status: queued | running | waiting_dependency | waiting_human | completed | failed
input_refs: [str]
output_ref: str | null
budget: BudgetReservation | null
created_at: str
```

**AgentMessage**
```yaml
schema_version: "agent_message.v1"
message_id: str
from_agent_id: str
to_agent_id: str
workflow_id: str
message_type: task_assign | result_report | conflict_request | approval_request
payload: dict
created_at: str
```

**ConflictRecord**
```yaml
schema_version: "conflict_record.v1"
conflict_id: str
workflow_id: str
conflict_type: same_file | contradictory_answer | partial_failure | agent_timeout | quality_disagreement
involved_nodes: [str]
resolution_strategy: human_arbitration | latest_wins | highest_quality | merge
resolution_result: str | null
resolved_at: str | null
```

### 8.6 Workflow State Machine

```
workflow_created → decomposed → queued → running
    → waiting_dependency → running
    → waiting_human → running
    → aggregating → [completed | failed | cancelled]
    → conflict_detected → resolved → aggregating
```

### 8.7 Conflict Resolution Rules

| Conflict Type | Default Strategy | Override |
|--------------|-----------------|----------|
| Same file edit | Human arbitration | Merge if compatible |
| Contradictory answer | Highest quality score | Human review |
| Partial failure | Retry failed node | Skip if non-critical |
| Agent timeout | Retry with backoff | Human escalation |
| Budget exhaustion | Cancel remaining | Human override |
| Quality disagreement | Human arbitration | LLM judge tiebreak |

### 8.8 Multi-Agent Budget Model

- Global workflow budget (total cap)
- Per-agent budget (individual agent cap)
- Per-node budget (sub-task cap)
- Parallel cost cap (max concurrent spend)
- Overrun strategy: cancel | escalate | human_override

### 8.9 Implementation Tasks

| Task | Description |
|------|-------------|
| P5-T1 | TaskDecomposer |
| P5-T2 | AgentRoleRegistry |
| P5-T3 | WorkflowEngine + state machine |
| P5-T4 | WorkQueue |
| P5-T5 | DependencyResolver |
| P5-T6 | ResultAggregator |
| P5-T7 | ConflictResolver + resolution strategies |
| P5-T8 | HumanApprovalGate |
| P5-T9 | MultiAgentBudgetManager |
| P5-T10 | AgentMessage protocol |
| P5-T11 | Multi-agent simulation tests |

### 8.10 Test Strategy

- Task decomposition tests (known input → expected graph)
- Workflow state machine tests (all transitions)
- Conflict resolution tests (each conflict type)
- Budget exhaustion tests (per-agent and global)
- Human approval checkpoint tests
- Parallel execution simulation tests

### 8.11 Promotion Gate

Phase 5 is complete when:
- Workflow engine handles all state transitions
- Conflict resolution works for all conflict types
- Multi-agent budget prevents runaway costs
- Human approval checkpoints function correctly
- At least 3 multi-agent workflows complete successfully

### 8.12 Boundary

Multi-agent orchestration depends on mature budget, evaluation, governance, and audit from earlier phases. No autonomous agent spawning without dispatch decision.

---

## 9. Phase 6: Production Hardening

### 9.1 Goal

Move from local research dispatcher to service-grade system with auth, multi-tenancy, observability, and durable storage.

### 9.2 Success Criteria

- FastAPI/uvicorn serving with API versioning
- SQLite for local, PostgreSQL for production
- Authentication (API key) and authorization (scoped permissions)
- Multi-tenant boundaries enforced
- Observability (metrics, structured logs, request tracing)
- Rate limiting per tenant/API key
- Backup/restore operational
- Security review passed

### 9.3 Dependency from Previous Phases

Phase 6 is split into two sub-phases:
- **Phase 6A**: Local durable API/storage hardening — can start after Phase 1/2 stable
- **Phase 6B**: Production multi-tenant hardening — requires Phase 5 orchestration mature

Production hardening is not feature innovation.

### 9.4 Deployment Modes

| Mode | Storage | Auth | Tenancy | Use Case |
|------|---------|------|---------|----------|
| local-single-user | JSON/JSONL | none | single | Development, experimentation |
| local-team | SQLite | API key | single | Small team, shared machine |
| production | PostgreSQL | API key + scopes | multi-tenant | Production deployment |

### 9.5 Core Components

| Component | Purpose |
|-----------|---------|
| FastAPIService | Production API server with versioning |
| AuthMiddleware | API key validation, scope enforcement |
| TenantResolver | Resolve tenant from request, enforce isolation |
| StorageMigrator | JSON/JSONL → SQLite → PostgreSQL migration |
| ObservabilityStack | Structured logging, metrics, request tracing |
| RateLimiter | Per-tenant, per-API-key rate limiting |
| BackupManager | Scheduled backups, restore capability |
| HealthChecker | Health/readiness endpoints |

### 9.6 Key Schemas

**Tenant**
```yaml
schema_version: "tenant.v1"
tenant_id: str
name: str
storage_backend: sqlite | postgresql
api_keys: [APIKeyRef]
rate_limit_policy: str
created_at: str
```

**APIKey**
```yaml
schema_version: "api_key.v1"
key_id: str
tenant_id: str
key_hash: str  # never store raw key
scopes: [str]  # e.g. ["dispatch:read", "dispatch:write", "ledger:read"]
rate_limit_override: str | null
expires_at: str | null
created_at: str
```

### 9.7 Data Migration Strategy

| From | To | Strategy |
|------|-----|----------|
| JSON/JSONL | SQLite | Batch import, validate schema, preserve IDs |
| SQLite | PostgreSQL | pgloader or custom migrator, schema versioning |
| Any | Any | Schema version field in every record, migrator naming convention |

Migration rules:
- Every schema has `schema_version` field
- Migrators named `{from}_{to}_v{N}.py`
- Backward compatibility window: 1 major version
- Replay compatibility: old dispatch records must be replayable
- Downgrade policy: export to JSON before major migration

### 9.8 Implementation Tasks

| Task | Description |
|------|-------------|
| P6-T1 | FastAPI service with versioned routes |
| P6-T2 | AuthMiddleware (API key validation) |
| P6-T3 | TenantResolver + tenant isolation |
| P6-T4 | SQLite durable local mode |
| P6-T5 | PostgreSQL production mode |
| P6-T6 | JSON/JSONL → SQLite migrator |
| P6-T7 | SQLite → PostgreSQL migrator |
| P6-T8 | ObservabilityStack (structured logging, metrics) |
| P6-T9 | RateLimiter |
| P6-T10 | BackupManager |
| P6-T11 | HealthChecker endpoints |
| P6-T12 | Security review |

### 9.9 Test Strategy

- API versioning tests
- Auth scope enforcement tests
- Tenant isolation tests (cross-tenant data access blocked)
- Migration tests (data integrity preserved)
- Rate limiting tests
- Backup/restore tests
- Health check tests
- Security regression tests

### 9.10 Promotion Gate

Phase 6 is complete when:
- FastAPI service passes smoke tests
- Auth scope enforcement works
- Tenant isolation verified
- Migration from JSON/JSONL to SQLite works
- PostgreSQL mode passes integration tests
- Security review has no critical findings

### 9.11 Boundary

Production hardening is not feature innovation. Only do when routing, execution, evaluation loops are stable. No new dispatch logic in this phase.

---

## 10. Phase 7: Ecosystem

### 10.1 Goal

Make the global dispatcher a platform with plugins, community profiles, CLI/SDK, and benchmarks.

### 10.2 Success Criteria

- Plugin system with permission model and sandboxing
- Community model profiles can be shared and validated
- External tool adapters exist
- Dashboard evolved with full dispatch visualization
- CLI available for common operations
- SDK available for integration
- Benchmark suite exists for model comparison

### 10.3 Dependency from Phase 6

Requires: Production API stable, auth/tenancy working, storage hardened. Ecosystem builds on a stable platform.

### 10.4 Core Components

| Component | Purpose |
|-----------|---------|
| PluginSystem | Plugin loading, permission enforcement, sandboxing |
| PluginRegistry | Discover, install, validate plugins |
| CommunityProfileRegistry | Share and validate model profiles |
| ToolAdapterManager | External tool integration (code execution, search, etc.) |
| DashboardEvolution | Full dispatch visualization, experiment results |
| CLI | Command-line interface for common operations |
| SDK | Python SDK for programmatic integration |
| BenchmarkSuite | Model comparison benchmarks |
| DocumentationSystem | Auto-generated docs from schemas |

### 10.5 Plugin Security Model

**PluginManifest**
```yaml
schema_version: "plugin_manifest.v1"
plugin_id: str
name: str
version: str
author: str
permissions: [str]  # e.g. ["dispatch:read", "provider:execute"]
entrypoints: [str]
compatible_dispatcher_versions: [str]
required_env: [str]
network_access: bool
filesystem_access: bool
signature: str | null
trust_level: community | verified | official
```

Plugin trust levels:
- **community**: Unvetted, sandboxed, limited permissions
- **verified**: Reviewed by maintainers, broader permissions
- **official**: Maintained by project team, full permissions

### 10.6 Implementation Tasks

| Task | Description |
|------|-------------|
| P7-T1 | PluginSystem (loader, permission enforcement) |
| P7-T2 | PluginRegistry (discovery, validation) |
| P7-T3 | CommunityProfileRegistry |
| P7-T4 | ToolAdapterManager |
| P7-T5 | Dashboard evolution |
| P7-T6 | CLI implementation |
| P7-T7 | SDK implementation |
| P7-T8 | BenchmarkSuite |
| P7-T9 | Documentation system |

### 10.7 Promotion Gate

Phase 7 is complete when:
- Plugin system loads and sandboxes plugins correctly
- At least 1 community plugin works end-to-end
- CLI covers core operations
- SDK can create dispatches programmatically
- Benchmark suite compares at least 3 models

### 10.8 Boundary

Ecosystem only makes sense when core dispatcher, provider, adaptive routing, and production boundary are stable. No ecosystem features before Phase 6 production hardening.

---

## 11. Cross-Phase Architecture Decisions

### 11.1 Decision Format

Each decision includes: context, chosen option, alternatives considered, consequences, reversal condition, affected phases.

### 11.2 Decision Table

| Decision | Choice | Alternatives | Consequences | Reversal Condition | Affected Phases |
|----------|--------|-------------|-------------|-------------------|-----------------|
| Python core retained | Superseded by ADR 0001: Rust core migration target | Rewrite in Go/Rust | Existing Python remains reference implementation until parity tests pass | Frozen wire contract and parity coverage become blockers | All |
| Library-first, API wrapper | Library-first | API-first | Better testability, composability | Need standalone service earlier than Phase 6 | All |
| Storage evolution | JSON/JSONL → SQLite → PostgreSQL | Direct PostgreSQL | Match complexity to phase, no premature infra | Data migration becomes blocker | 0→6 |
| Shadow-first routing | Yes | Direct active routing | Evidence before action, safer promotion | Shadow data never useful after N trials | 1→4 |
| Manual bridge before provider | Yes | Direct provider integration | Low-risk validation, accumulates real data | Manual bridge creates no useful cost_of_pass after N trials | 2→3 |
| Safety gate before execution | Yes | Post-execution audit | Budget + boundary enforced before spend | Need real execution data to calibrate gates | All |
| Evidence chain first-class | Yes | Log-only audit | Every decision traceable, replayable | Evidence storage becomes too expensive | All |
| Cost-of-pass as routing feedback | Yes | Quality-only or cost-only | Data-driven model selection, balanced optimization | Insufficient data for meaningful cost_of_pass | 3→4 |
| Human authority model | Humans override always | Full automation | Safety principle, trust building | Human bottleneck blocks all progress | All |
| Target repo write model | Approval required | Auto-write with audit | Governance principle, prevents accidental mutation | Approval overhead too high for routine ops | 3+ |

---

## 12. Global Schema Registry

### 12.1 Registry Format

Each schema entry includes: name, version, phase introduced, owner component, storage, writer components, reader components, lifecycle state.

### 12.2 Phase 0 Schemas (Existing)

| Schema | Version | Owner | Writers | Readers | Lifecycle |
|--------|---------|-------|---------|---------|-----------|
| UsageLedgerRow | usage_ledger.v1 | usage_ledger.py | DispatchLedger, ManualBridge, ProviderAdapter | CostOfPass, Dashboard | stable |
| CostOfPassAggregate | usage_ledger.v1 | usage_ledger.py | CostOfPassAccumulator | ModelSelector, Dashboard | stable |
| ModelHarnessProfile | model_harness_profile.v1 | model_profiles.py | Config | ModelSelector, Gateway | stable |
| ShadowRoutingRecommendation | shadow_routing_recommendation.v1 | model_profiles.py | RoutingEngine | ModelSelector, Dashboard | stable |
| RoutingPolicy | routing_policy.v1 | routing.py | Config | ModelSelector | stable |
| ErrorDomain | error_taxonomy.v1 | error_taxonomy.py | ErrorClassifier | All components | stable |

### 12.3 Phase 1 Schemas (New)

| Schema | Version | Owner | Writers | Readers | Lifecycle |
|--------|---------|-------|---------|---------|-----------|
| TaskAnalysis | task_analysis.v1 | task_analyzer.py | TaskAnalyzer | ModelSelector, BudgetManager, Ledger | new |
| DispatchDecision | dispatch_decision.v1 | dispatch_decision.py | DispatchEngine | Executor, Ledger | new |
| BudgetReservation | budget_reservation.v1 | budget_manager.py | BudgetManager | DispatchEngine, Ledger | new |
| ExecutionResult | execution_result.v1 | executor_adapter.py | ExecutorAdapter | EvaluationStub, Ledger | new |
| EvaluationResult | evaluation_result.v1 | evaluation_stub.py | EvaluationStub | Ledger | new |
| DispatchRecord | dispatch_record.v1 | dispatch_ledger.py | DispatchLedger | Dashboard, API | new |
| Evidence | (embedded) | task_analyzer.py | TaskAnalyzer | TaskAnalysis readers | new |
| ShadowRoute | (embedded) | model_selector.py | ModelSelector | DispatchDecision readers | new |
| RejectedCandidate | (embedded) | model_selector.py | ModelSelector | DispatchDecision readers | new |

### 12.4 Phase 2 Schemas (New)

| Schema | Version | Owner | Writers | Readers | Lifecycle |
|--------|---------|-------|---------|---------|-----------|
| PromptPack | prompt_pack.v1 | prompt_pack_gen.py | PromptPackGenerator | Human, Dashboard | new |
| PastebackSubmission | pasteback_submission.v1 | pasteback_parser.py | Human (via API) | ManualEvaluator, Ledger | new |
| ManualExecutionSession | manual_execution_session.v1 | manual_session.py | ManualBridge | Dashboard | new |

### 12.5 Phase 3 Schemas (New)

| Schema | Version | Owner | Writers | Readers | Lifecycle |
|--------|---------|-------|---------|---------|-----------|
| ProviderConfig | provider_config.v1 | provider_adapter.py | Config | ProviderAdapter | new |
| CredentialRef | credential_ref.v1 | credential_boundary.py | Config | ProviderAdapter | new |
| ProviderAuditEvent | provider_audit_event.v1 | provider_audit.py | ProviderAdapter | Dashboard, Security | new |
| RetryPolicy | retry_policy.v1 | retry_manager.py | Config | RetryFallbackManager | new |

### 12.6 Phase 4-7 Schemas (Provisional — must be expanded before implementation)

| Schema | Version | Phase | Purpose |
|--------|---------|-------|---------|
| RoutingExperiment | routing_experiment.v1 | Phase 4 | A/B experiment definition |
| RoutingArm | (embedded) | Phase 4 | Experiment arm |
| RoutingObservation | routing_observation.v1 | Phase 4 | Per-dispatch observation |
| WorkflowGraph | workflow_graph.v1 | Phase 5 | Multi-agent workflow |
| WorkflowNode | (embedded) | Phase 5 | Workflow sub-task |
| AgentMessage | agent_message.v1 | Phase 5 | Inter-agent communication |
| ConflictRecord | conflict_record.v1 | Phase 5 | Conflict detection/resolution |
| Tenant | tenant.v1 | Phase 6 | Multi-tenant definition |
| APIKey | api_key.v1 | Phase 6 | Auth key management |
| PluginManifest | plugin_manifest.v1 | Phase 7 | Plugin definition |

### 12.7 Schema Evolution Rules

1. Version format: `schema_name.vMAJOR.MINOR` (e.g., `task_analysis.v1.0`)
2. Non-breaking additive changes (nullable fields) → increment minor (v1.0 → v1.1)
3. Breaking changes → increment major (v1.1 → v2.0)
4. Records declare exact schema_version
5. Readers declare supported version range
6. Backward compatibility window: 1 major version
7. Migrators named `{schema}_v{from}_to_v{to}.py`

---

## 13. Testing Strategy Across All Phases

### 13.1 Core Test Strategies

| Strategy | Description | Applicable Phases |
|----------|-------------|-------------------|
| Fixture-driven deterministic | Fixed inputs, expected outputs, replayable | All |
| Golden request set | 20+ diverse requests covering all task types | Phase 1+ |
| Replay tests | Any dispatch record can be replayed identically | Phase 1+ |
| Budget simulation | Simulate budget constraints without real spend | Phase 1+ |
| Provider dry-run | Test provider integration without real API calls | Phase 3+ |
| Manual execution tests | Validate pasteback workflow | Phase 2 |
| LLM judge calibration | Calibrate quality evaluation thresholds | Phase 3+ |
| Multi-agent simulation | Simulate agent interactions without real execution | Phase 5+ |
| Security regression | Ensure safety boundaries never violated | All |
| Production smoke | Basic health checks for service-grade deployment | Phase 6+ |

### 13.2 Phase Test Matrix

| Phase | Required Tests | Pass/Fail Gate |
|-------|---------------|----------------|
| Phase 1 | 20 fixture dispatches, schema validation, gate checks, negation-aware evidence | All fixtures pass deterministically |
| Phase 2 | Prompt pack generation, pasteback validation, manual ledger write, no-provider guard | 10+ manual executions recorded |
| Phase 3 | Provider dry-run, credential redaction, timeout/retry, cost calculation, no-secret-in-logs | Redaction tests pass, one live smoke |
| Phase 4 | A/B experiment simulation, promotion gate, cold-start, rollback | Adaptive beats static baseline |
| Phase 5 | Workflow state machine, conflict resolution, budget exhaustion, human approval | 3+ multi-agent workflows complete |
| Phase 6 | API versioning, auth scope, tenant isolation, migration, rate limiting | Security review passes |
| Phase 7 | Plugin sandbox, CLI commands, SDK integration | 1+ community plugin works |

### 13.3 Golden Fixture Governance

- Golden fixtures stored in `tests/fixtures/dispatch/`
- Each fixture: input request + expected analysis + expected decision + expected gates
- Golden output update requires explicit review (not auto-updated)
- Fixture coverage: Phase 1 minimum 20 representative fixtures; Phase 1 beta 90 cross-product (task_domain × task_intent)

### 13.4 Replay Compatibility

- Any DispatchRecord can be replayed to produce identical DispatchDecision
- Replay uses stored analysis_snapshot, not re-analysis
- Replay tests run on every schema change
- Breaking replay requires explicit migration

---

## 14. Roadmap Summary

### 14.1 Phase Dependency Graph

```
Phase 0 (done) → Phase 1 → Phase 2 → Phase 3 → Phase 4
                                    ↘           ↗
                              Phase 5 (depends on 3+4)
                                        ↓
                                    Phase 6 → Phase 7
```

### 14.2 Detailed Dependencies

| Phase | Can Start After | Blocked By | Parallelizable With |
|-------|----------------|------------|-------------------|
| Phase 1 | Phase 0 complete | — | — |
| Phase 2 | Phase 1 manual executor contract | — | — |
| Phase 3 (dry-run) | Phase 2 schemas stable | — | Phase 2 completion |
| Phase 3 (live) | Phase 2 cost data available | Phase 3 dry-run | — |
| Phase 4 | Phase 3 provider usage data + Phase 2/3 evaluation data | — | — |
| Phase 5 | Phase 1 dispatch record + Phase 3 execution + Phase 4 routing | — | — |
| Phase 6 (API/storage) | Phase 1 stable | — | Phase 2-5 |
| Phase 6 (multi-tenant) | Phase 5 complete | — | — |
| Phase 7 (docs/CLI) | Phase 6 API stable | — | Phase 6 completion |
| Phase 7 (plugins) | Phase 6 production stable | — | — |

### 14.3 Minimum Viable Dispatcher Path

Phase 0 → Phase 1 → Phase 2 → Phase 3

This gives: task analysis → dispatch decision → manual execution → real provider execution

### 14.4 Full Global Dispatcher Path

All phases 0-7

### 14.5 When to Stop

Stop at any phase if:
- The current phase's success criteria are not met
- Risk exceeds benefit
- User does not need further capabilities

### 14.6 When to Promote Phase Maturity

Promote when:
- Phase-specific pass/fail gates from Section 13.2 are met
- All success criteria met
- Tests comprehensive and passing
- No known critical bugs
- Documentation complete

### 14.7 When to Reject a Phase

Reject if:
- Prerequisites not met
- Success criteria unachievable with current technology
- Risk too high for current project stage

---

## 15. Appendices

### 15.1 Glossary

| Term | Definition |
|------|-----------|
| Dispatcher | System that analyzes tasks, selects models, manages budgets, orchestrates execution |
| Control Plane | Governance, audit, triage, error taxonomy (Phase 0) |
| Dispatcher Kernel | Task analysis + dispatch decision loop (Phase 1) |
| Execution Plane | Provider adapters, sandbox, workers (Phase 3+) |
| Evaluation Plane | Quality assessment, cost-of-pass, feedback loops |
| Cost-of-pass | Total cost divided by successful completions for a task group |
| Shadow routing | Diagnostic routing recommendation, not active decision |
| Admitted evidence | Evidence that meets quality threshold for routing decisions |
| Dispatch gate | Lightweight dispatch-time check (not runtime clearance) |
| Promotion gate | Criteria for advancing from one phase to the next |

### 15.2 Task Domain Enum

```
code | docs | config | infra | math | architecture | repo_ops | governance | other
```

### 15.3 Task Intent Enum

```
generate | review | debug | summarize | audit | plan | refactor | compare | explain | classify
```

### 15.4 Risk Flag Enum

```
target_write | provider_call | sandbox_execution | deployment | secret_handling |
destructive_operation | long_context | high_uncertainty
```

### 15.5 Risk Level Enum

```
low | medium | high | critical
```

Derived from risk_flags + task_domain + task_intent combination.

### 15.6 Model Tier Definitions (from model_profiles.py)

| Tier | Purpose | Cost Level | Quality Level |
|------|---------|-----------|---------------|
| cheap_executor | Simple, repetitive tasks | Low | Adequate |
| balanced_worker | General-purpose tasks | Medium | Good |
| strong_planner | Complex reasoning, architecture | High | Excellent |
| verifier | Review, validation tasks | Medium | High accuracy |
| advisor | Strategic guidance, analysis | High | Expert-level |

### 15.7 Execution Gate Definitions

| Gate | Condition | Severity |
|------|-----------|----------|
| budget_gate | Budget exceeded or approval needed | block |
| risk_gate | high/critical risk or destructive/provider/deploy | block |
| boundary_gate | provider/sandbox/target_write detected | block |
| confidence_gate | confidence < 0.5 | warning |
| manual_review_gate | human_review_required | block |
| provider_disabled_gate | provider blocked in current phase | block |
| sandbox_disabled_gate | sandbox blocked | block |
| target_write_gate | target repo mutation detected | block |

### 15.8 Error Domain Definitions (from error_taxonomy.py)

| Domain | Examples |
|--------|---------|
| user_input | malformed request, missing fields, invalid format |
| task_analysis | classification failure, ambiguity unresolved |
| model_selection | no suitable model, capability mismatch |
| budget | reservation exceeded, cost overrun |
| provider_auth | invalid key, expired token |
| provider_rate_limit | RPM/TPM exceeded |
| provider_timeout | connect/read timeout |
| provider_content | safety filter, policy violation |
| execution | runtime error, crash, OOM |
| evaluation | judge failure, schema mismatch |

### 15.9 Example Dispatch Trace

```
[2026-05-27T10:00:00Z] dispatch_created
  dispatch_id: disp_001
  request: "Fix auth.py security issues and commit the changes"

[2026-05-27T10:00:01Z] analysis_complete
  task_domain: code
  task_intent: review
  risk_flags: [target_write, secret_handling]
  complexity_score: 0.72
  confidence: 0.80
  risk_level: high

[2026-05-27T10:00:02Z] decision_made
  selected_tier: strong_planner
  fallback_tier: balanced_worker
  shadow_routes: [{tier: verifier, reason: "review component may need less planning"}]
  execution_gates: [{gate_type: target_write, severity: block}, {gate_type: risk, severity: block}]
  decision_status: needs_approval

[2026-05-27T10:00:03Z] budget_reserved
  reserved_input_tokens: 4000
  reserved_output_tokens: 2000
  reserved_cost: 0.05 USD
  status: reserved

[2026-05-27T10:00:04Z] execution_started
  executor_type: noop
  status: not_executed

[2026-05-27T10:00:05Z] evaluation_complete
  status: not_evaluated
  checks: [output_present: skipped, boundary_compliance: pass]

[2026-05-27T10:00:06Z] dispatch_recorded
  final_status: not_executed
  usage_ledger_row_id: null
```

**Example 2: Low-risk read-only review**
```
[2026-05-27T10:01:00Z] dispatch_created
  dispatch_id: disp_002
  request: "Review auth.py for security issues"

[2026-05-27T10:01:01Z] analysis_complete
  task_domain: code
  task_intent: review
  risk_flags: []
  complexity_score: 0.35
  confidence: 0.90
  risk_level: low

[2026-05-27T10:01:02Z] decision_made
  selected_tier: verifier
  fallback_tier: balanced_worker
  shadow_routes: [{tier: cheap_executor, reason: "simple review may not need verifier"}]
  execution_gates: []
  decision_status: decided

[2026-05-27T10:01:03Z] budget_reserved
  reserved_input_tokens: 2000
  reserved_output_tokens: 1000
  reserved_cost: 0.02 USD
  status: reserved

[2026-05-27T10:01:04Z] dispatch_recorded
  final_status: not_executed
```

### 15.10 Example Cost-of-Pass Report

```
Task Group: code_review (last 30 days)
  Total dispatches: 45
  Successful completions: 38
  Cost-of-pass: $0.12 per successful review

  By tier:
    cheap_executor: 15 dispatches, 10 success, CoP: $0.04
    balanced_worker: 20 dispatches, 18 success, CoP: $0.10
    strong_planner: 10 dispatches, 10 success, CoP: $0.25

  Recommendation: balanced_worker offers best cost/quality tradeoff for code_review
```
