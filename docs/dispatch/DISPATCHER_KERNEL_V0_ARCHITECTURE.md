# Token-Efficient Agent Harness: Global System Dispatcher Architecture Book

> Schema version: architecture_book.v1
> Status: Converged design (Claude Code <-> GPT, 3+ rounds)
> Scope: Full vision from Phase 0 (current) through Phase 7 (ecosystem)

---

## 0. Document Orientation

### 0.1 Purpose

This document is the master architecture book for the Token-Efficient Agent Harness project. It defines the complete journey from the current control-plane harness to a full Global System Dispatcher/Orchestrator.

It is NOT a v0 implementation document. It covers every phase from current state to ultimate vision, with detailed implementation breakdowns for each phase.

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
**Dispatcher** (Phase 1-4): Adds decision-making, budget management, real execution
**Production system** (Phase 6+): Service-grade, multi-tenant, observable

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

### 3.2-3.15 Detailed Component Descriptions

(See existing code documentation for each component)

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
execution_gates: [str]
decision_status: decided | needs_approval | blocked | diagnostic_only
created_at: str
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
| POST /api/dispatch | Create dispatch decision |
| GET /api/dispatch/{dispatch_id} | Get dispatch record |
| POST /api/dispatch/{dispatch_id}/preview-execution | Preview execution |
| POST /api/dispatch/{dispatch_id}/manual-result | Submit manual result |
| GET /api/dispatch/ledger | List dispatch records |

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

Connect the dispatcher to real human execution flow without provider integration.

### 5.2 Success Criteria

- Dispatcher generates prompt packs with recommended model tier, token budget, expected schema, evaluation checklist
- Human manually calls model and pastes output back
- System evaluates pasted output and records in ledger
- Accumulates real cost-of-pass data from manual executions

### 5.3-5.15 Detailed Sections

(To be expanded in next iteration)

**Key components:**
- Prompt Pack Generator
- Pasteback Workflow UI
- Manual ExecutionResult handler
- Human Review Loop
- UsageLedger write from manual results
- Cost-of-pass data accumulation

### 5.15 Boundary

No provider integration. No automatic execution. Human is the executor.

---

## 6. Phase 3: Real Provider Integration

### 6.1 Goal

Connect to real model providers with full audit, budget enforcement, and safety boundaries.

### 6.2 Success Criteria

- Can call OpenAI-compatible, Anthropic, and local model providers
- Credential boundary enforced (no secret leakage)
- Real token/cost tracking in usage_ledger
- Timeout, retry, fallback working
- LLM judge for quality evaluation
- Provider audit events recorded

### 6.3-6.19 Detailed Sections

(To be expanded)

**Key components:**
- Provider Adapter abstraction
- OpenAI-compatible provider
- Anthropic provider
- Local model provider
- Credential Boundary + Secret Redaction
- Provider Audit Events
- Real Cost Tracking
- Timeout/Retry/Fallback
- Provider Failure Taxonomy
- LLM Judge quality evaluation

### 6.19 Boundary

Provider calls are gated by dispatch decision. Budget enforced before execution. All calls audited.

---

## 7. Phase 4: Adaptive Routing

### 7.1 Goal

Upgrade routing from static rules to history-driven adaptive routing.

### 7.2 Success Criteria

- System selects models based on historical cost-of-pass
- Shadow routing can promote to active routing with evidence threshold
- A/B routing experiments work
- Automatic downgrade/upgrade based on quality feedback

### 7.3-7.17 Detailed Sections

(To be expanded)

**Key components:**
- Historical Cost-of-pass Routing
- Dynamic Model Tier Selection
- Shadow → Active Routing promotion
- A/B Routing Experiments
- Routing Policy Lifecycle
- Automatic Downgrade/Upgrade
- Confidence-based Escalation
- Budget-aware Rerouting
- Feedback Loop Integration

### 7.17 Boundary

Adaptive routing requires sufficient historical data. Cold-start uses static rules.

---

## 8. Phase 5: Multi-Agent Orchestration

### 8.1 Goal

Extend single-task dispatch to multi-agent workflows.

### 8.2 Success Criteria

- Tasks can be decomposed into sub-tasks
- Multiple agents can work in parallel
- Results are aggregated with conflict resolution
- Human approval checkpoints exist
- Multi-agent budget management works

### 8.3-8.19 Detailed Sections

(To be expanded)

**Key components:**
- Task Decomposition
- Agent Role Model
- Agent Capability Registry
- Inter-agent Communication Protocol
- Work Queue + Parallel Execution
- Dependency Graph
- Result Aggregation + Conflict Resolution
- Human Approval Checkpoints
- Multi-agent Budget Management

### 8.19 Boundary

Multi-agent orchestration depends on mature budget, evaluation, governance, and audit from earlier phases.

---

## 9. Phase 6: Production Hardening

### 9.1 Goal

Move from local research dispatcher to service-grade system.

### 9.2 Success Criteria

- FastAPI/uvicorn serving
- SQLite for local, PostgreSQL for production
- Authentication and authorization
- Multi-tenant boundaries
- Observability (metrics, tracing, logs)
- Rate limiting
- Backup/restore
- Security review passed

### 9.3-9.21 Detailed Sections

(To be expanded)

**Key components:**
- FastAPI/uvicorn service
- API contract versioning
- SQLite → PostgreSQL migration
- AuthN/AuthZ
- Multi-tenant boundary
- Secrets management
- Observability stack
- Rate limits
- Deployment model
- Backup/restore
- Failure recovery
- Security review

### 9.21 Boundary

Production hardening is not feature innovation. Only do when routing, execution, evaluation loops are stable.

---

## 10. Phase 7: Ecosystem

### 10.1 Goal

Make the global dispatcher a platform, not just a project.

### 10.2 Success Criteria

- Plugin system works
- Community model profiles can be shared
- External tool adapters exist
- Dashboard evolved
- CLI and SDK available
- Benchmark suite exists

### 10.3-10.17 Detailed Sections

(To be expanded)

**Key components:**
- Plugin system
- Community model profiles
- External tool adapters
- Dashboard UI evolution
- CLI
- SDK
- Template repo onboarding
- Benchmark suite
- Marketplace/registry model
- Documentation system

### 10.17 Boundary

Ecosystem only makes sense when core dispatcher, provider, adaptive routing, and production boundary are stable.

---

## 11. Cross-Phase Architecture Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Python core retained | Yes | Existing harness, stable tests, policy+IO+evaluation fit |
| API-first vs library-first | Library-first, API wrapper | Testability, composability |
| Storage evolution | JSON/JSONL → SQLite → PostgreSQL | Match complexity to phase |
| Shadow-first routing | Yes | Evidence before action |
| Manual bridge before provider | Yes | Low-risk validation |
| Safety gate before execution | Yes | Budget + boundary enforced first |
| Evidence chain first-class | Yes | Every decision traceable |
| Cost-of-pass as routing feedback | Yes | Data-driven model selection |
| Human authority model | Humans override always | Safety principle |
| Target repo write model | Approval required | Governance principle |

---

## 12. Global Schema Registry

| Schema | Version | Phase Introduced |
|--------|---------|-----------------|
| TaskAnalysis | task_analysis.v1 | Phase 1 |
| DispatchDecision | dispatch_decision.v1 | Phase 1 |
| BudgetReservation | budget_reservation.v1 | Phase 1 |
| ExecutionResult | execution_result.v1 | Phase 1 |
| EvaluationResult | evaluation_result.v1 | Phase 1 |
| DispatchRecord | dispatch_record.v1 | Phase 1 |
| UsageLedgerRow | usage_ledger.v1 | Phase 0 |
| CostOfPassAggregate | usage_ledger.v1 | Phase 0 |
| ModelHarnessProfile | model_harness_profile.v1 | Phase 0 |
| ShadowRoutingRecommendation | shadow_routing_recommendation.v1 | Phase 0 |
| ProviderAuditEvent | provider_audit_event.v1 | Phase 3 |
| AgentMessage | agent_message.v1 | Phase 5 |
| WorkflowRun | workflow_run.v1 | Phase 5 |
| PluginManifest | plugin_manifest.v1 | Phase 7 |

---

## 13. Testing Strategy Across All Phases

| Strategy | Description |
|----------|-------------|
| Fixture-driven deterministic | Fixed inputs, expected outputs, replayable |
| Golden request set | 20+ diverse requests covering all task types |
| Replay tests | Any dispatch record can be replayed identically |
| Budget simulation | Simulate budget constraints without real spend |
| Provider dry-run | Test provider integration without real API calls |
| Manual execution tests | Validate pasteback workflow |
| LLM judge calibration | Calibrate quality evaluation thresholds |
| Multi-agent simulation | Simulate agent interactions without real execution |
| Security regression | Ensure safety boundaries never violated |
| Production smoke | Basic health checks for service-grade deployment |

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

### 14.2 Minimum Viable Dispatcher Path

Phase 0 → Phase 1 → Phase 2 → Phase 3

This gives: task analysis → dispatch decision → manual execution → real provider execution

### 14.3 Full Global Dispatcher Path

All phases 0-7

### 14.4 When to Stop

Stop at any phase if:
- The current phase's success criteria are not met
- Risk exceeds benefit
- User does not need further capabilities

### 14.5 When to Promote Phase Maturity

Promote when:
- All success criteria met
- Tests comprehensive and passing
- No known critical bugs
- Documentation complete

### 14.6 When to Reject a Phase

Reject if:
- Prerequisites not met
- Success criteria unachievable with current technology
- Risk too high for current project stage

---

## 15. Appendices

### 15.1 Glossary

(To be expanded)

### 15.2-15.10

- State machine enums
- Risk flags
- Model tier definitions
- Error domain definitions
- Gate definitions
- Budget policy examples
- Routing policy examples
- Example dispatch traces
- Example cost-of-pass reports

(To be expanded in next iteration)
