# Dispatcher Kernel v0 Architecture

> Schema version: dispatcher_kernel.v0
> Status: Converged design, ready for implementation
> Produced by: Claude Code <-> GPT collaborative architecture discussion (3 rounds)

---

## 1. Vision

Dispatcher Kernel v0 is the first step from "research harness with control plane" to "global system dispatcher."

**v0 goal:** Given any task request, produce an auditable dispatch decision and complete one closed-loop cycle via noop/mock/manual executor — without calling real model providers.

**Success criteria (20 diverse task fixtures):**
1. Output stable TaskAnalysis
2. Correctly identify task_domain / task_intent / risk_flags
3. Produce 0-1 four-dimensional complexity scores
4. Select reasonable model tier
5. Reserve budget
6. Generate dispatch decision + dispatch record
7. Update dispatch ledger with mock/manual result
8. Escalate or request human input when confidence is low
9. Never violate execution boundaries (no provider calls, no sandbox, no target writes)

---

## 2. Architecture Overview

```
user_request
    │
    ▼
┌─────────────────┐
│  TaskAnalyzer    │  (RuleBasedTaskAnalyzer v0)
│  task_analyzer.py│
└────────┬────────┘
         │ TaskAnalysis
         ▼
┌─────────────────┐
│  ModelSelector   │  (deterministic, shadow dual-track)
│ model_selector.py│
└────────┬────────┘
         │ (selected_tier, fallback_tier, shadow_routes)
         ▼
┌─────────────────┐
│  BudgetManager   │  (reserve / track / release)
│ budget_manager.py│
└────────┬────────┘
         │ BudgetReservation
         ▼
┌─────────────────┐
│ DispatchEngine   │  (orchestrator)
│dispatch_engine.py│
└────────┬────────┘
         │ DispatchDecision
         ▼
┌─────────────────┐
│ ExecutorAdapter  │  (noop / mock / manual)
│executor_adapter.py│
└────────┬────────┘
         │ ExecutionResult
         ▼
┌─────────────────┐
│ EvaluationStub   │  (schema_validity / rule_check)
│evaluation_stub.py│
└────────┬────────┘
         │ EvaluationResult
         ▼
┌─────────────────┐
│  DispatchLedger  │  (DispatchRecord — full chain)
│dispatch_ledger.py│
└─────────────────┘
```

---

## 3. Module Structure

```
src/harness_core/dispatch/
├── __init__.py
├── task_analyzer.py       # RuleBasedTaskAnalyzer
├── model_selector.py      # ModelSelector + shadow dual-track
├── budget_manager.py      # BudgetManager
├── dispatch_decision.py   # DispatchDecision + BudgetReservation schemas
├── dispatch_engine.py     # DispatchEngine orchestrator
├── executor_adapter.py    # NoopExecutor, MockExecutor, ManualExecutor
├── evaluation_stub.py     # EvaluationStub (schema_validity, rule_check)
└── dispatch_ledger.py     # DispatchRecord, DispatchLedger

tests/
├── test_task_analyzer.py
├── test_model_selector.py
├── test_budget_manager.py
├── test_dispatch_engine.py
├── test_executor_adapter.py
├── test_evaluation_stub.py
└── test_dispatch_ledger.py

docs/dispatch/
└── DISPATCHER_KERNEL_V0_ARCHITECTURE.md  (this file)
```

---

## 4. Schemas

### 4.1 TaskAnalysis

Schema version: `task_analysis.v1`

```python
@dataclass
class Evidence:
    feature: str          # e.g. "deployment_term", "no_execution_boundary"
    text: str             # matched text
    span: tuple[int, int] # (start, end) in raw_request

@dataclass
class TaskAnalysis:
    analysis_id: str
    raw_request: str

    # Dual-axis classification
    primary_task_type: str      # compat: code_review / doc_summary / architecture / ...
    task_domain: str            # code / docs / config / infra / math / architecture / repo_ops / governance / other
    task_intent: str            # generate / review / debug / summarize / audit / plan / refactor / compare / explain / classify
    risk_flags: list[str]       # target_write / provider_call / sandbox_execution / deployment / secret_handling / destructive_operation / long_context / high_uncertainty

    # Four-dimensional complexity
    complexity_score: float         # weighted composite, 0-1
    cognitive_complexity: float     # reasoning / architecture / math / debug difficulty
    context_complexity: float       # input length / file count / cross-module / multi-repo
    execution_risk: float           # write / deploy / sandbox / provider / secret
    ambiguity_score: float          # unclear requirements / conflicting goals / missing constraints

    # Capabilities and budgets
    required_capabilities: list[str]   # reasoning / tool_use / long_context / code_output / json_strict / creative
    context_budget_estimate: int       # tokens
    execution_budget_estimate: int     # tokens
    quality_requirement: str           # draft / standard / high / critical

    # Confidence and escalation
    confidence: float                  # 0-1
    uncertainty_reason: list[str]
    safe_default: str                  # use_cheap_first / use_balanced / require_human / escalate_to_advisor
    escalation_trigger: str | None

    # Evidence (negation-aware)
    positive_evidence: list[Evidence]
    negative_evidence: list[Evidence]
    features_detected: dict
    analysis_method: str               # rule_only (v0), cheap_model (v1), advisor_model (v2)

    created_at: str
```

**Domain enums:**

```python
TASK_DOMAINS = ("code", "docs", "config", "infra", "math", "architecture", "repo_ops", "governance", "other")
TASK_INTENTS = ("generate", "review", "debug", "summarize", "audit", "plan", "refactor", "compare", "explain", "classify")
RISK_FLAGS = ("target_write", "provider_call", "sandbox_execution", "deployment", "secret_handling", "destructive_operation", "long_context", "high_uncertainty")
```

### 4.2 DispatchDecision

Schema version: `dispatch_decision.v1`

```python
@dataclass
class ShadowRoute:
    tier: str
    profile_id: str | None
    reason: str
    admission_scope: str = "diagnostic"

@dataclass
class BudgetReservation:
    reservation_id: str
    pre_budget: float              # remaining budget before reservation
    reserved_amount: float
    actual_usage: float | None     # filled after execution
    budget_delta: float | None     # actual - reserved
    budget_violation: bool
    status: str                    # reserved / consumed / released / violated / expired
    currency: str = "usd"          # usd / tokens

@dataclass
class DispatchDecision:
    decision_id: str

    # Reference to analysis (not deep embedding)
    analysis_id: str
    analysis_snapshot: dict        # frozen copy of TaskAnalysis fields

    # Model selection
    selected_tier: str             # from model_profiles.TIERS
    selected_profile_id: str | None
    fallback_tier: str
    fallback_profile_id: str | None
    shadow_routes: list[ShadowRoute]

    # Token/output limits
    max_input_tokens: int
    max_output_tokens: int

    # Routing rationale
    routing_reason: str
    expected_cost: float
    quality_requirement: str       # from TaskAnalysis.quality_requirement (not a float)
    confidence: float

    # Budget
    budget_reservation: BudgetReservation

    # Execution config
    executor_type: str             # noop / mock / manual / provider(disabled in v0)
    execution_gates: list[str]     # simplified gates (see section 6)

    created_at: str
```

### 4.3 ExecutionResult

Schema version: `execution_result.v1`

```python
@dataclass
class ExecutionResult:
    result_id: str
    decision_id: str
    executor_type: str             # noop / mock / manual / provider
    status: str                    # success / failure / timeout / budget_exceeded / human_required / planned
    output: str | None
    input_tokens: int
    output_tokens: int
    estimated_cost: float
    latency_ms: int
    quality_score: float | None
    error: dict | None             # error_taxonomy.ErrorRecord shape
    created_at: str
```

### 4.4 EvaluationResult

Schema version: `evaluation_result.v1`

```python
@dataclass
class EvaluationResult:
    evaluation_id: str
    decision_id: str
    result_id: str
    checks: dict                  # {"schema_validity": bool, "rule_check": bool, "human_review_required": bool}
    passed: bool
    failure_reasons: list[str]
    created_at: str
```

### 4.5 DispatchRecord

Schema version: `dispatch_record.v1`

```python
@dataclass
class DispatchRecord:
    dispatch_id: str
    request_snapshot: str
    task_analysis_id: str
    decision_id: str
    execution_result_id: str | None
    evaluation_result_id: str | None
    usage_ledger_row_id: str | None
    final_status: str              # dispatched / executing / completed / failed / escalated / cancelled
    created_at: str
    updated_at: str
```

---

## 5. Component Details

### 5.1 RuleBasedTaskAnalyzer (task_analyzer.py)

**v0: Pure rules, no model calls, deterministic, testable, replayable.**

Classification logic:
- **task_domain**: keyword matching on domain-specific terms
- **task_intent**: verb/action detection (generate, review, debug, etc.)
- **risk_flags**: negation-aware keyword detection

Complexity formula (v0):

```
cognitive_complexity =
    0.15 base
    + 0.20 if architecture/debug/math domain
    + 0.15 if asks for tradeoff/design
    + 0.10 if asks for multi-step plan
    + 0.10 if correctness-critical terms appear

context_complexity =
    min(0.40, input_chars / 12000)
    + 0.10 * code_block_count (capped at 0.30)
    + 0.15 if repo_context present
    + 0.15 if multi-file / cross-module terms appear

execution_risk =
    0.25 if target_write intent
    + 0.25 if deployment/sandbox/process
    + 0.20 if provider/credential/network
    + 0.20 if destructive operation
    (capped at 1.0)

ambiguity_score =
    0.20 if request lacks explicit output format
    + 0.20 if broad goal with no constraints
    + 0.20 if conflicting signals
    + 0.20 if "best/optimal/production-ready" without acceptance criteria

complexity_score =
    0.35 * cognitive_complexity
    + 0.25 * context_complexity
    + 0.25 * execution_risk
    + 0.15 * ambiguity_score
```

Negation-awareness (critical):
- "do not execute" → NOT a risk flag
- "read-only audit" → NOT a write operation
- "no sandbox" → NOT a sandbox execution
- "without provider calls" → NOT a provider call

Evidence tracking:
- `positive_evidence`: risk phrases that actually appear as intended actions
- `negative_evidence`: risk phrases that appear inside safety boundaries
- `features_detected`: raw signals for debugging

### 5.2 ModelSelector (model_selector.py)

**Dual-track: active decision + shadow diagnostic.**

Selection logic:
1. Look up RoutingPolicy.tier_map for task_domain + task_intent
2. If confidence < threshold → use safe_default
3. Match required_capabilities against profile attributes (tool_strictness, json_tolerance, reasoning_effort)
4. Filter profiles exceeding budget
5. If historical cost_of_pass data exists → prefer lowest cost_of_pass in matching tier
6. Generate shadow_routes for audit (cheaper + more expensive alternatives)

### 5.3 BudgetManager (budget_manager.py)

**Separate from usage_ledger. Reservation = "allowed max", usage_ledger = "actual spent".**

Operations:
- `reserve(analysis, selected_profile, budget_policy) → BudgetReservation`
- `release(reservation_id)` — when execution is skipped
- `consume(reservation_id, actual_usage)` — after execution
- `check_violation(reservation_id) → bool`

BudgetReservation states:
- `reserved` — budget held, execution pending
- `consumed` — execution completed, actual usage recorded
- `released` — execution skipped, budget freed
- `violated` — actual usage exceeded reservation
- `expired` — reservation timed out

### 5.4 DispatchEngine (dispatch_engine.py)

**Orchestrator. Composes all components into the dispatch loop.**

```python
class DispatchEngine:
    def __init__(self, task_analyzer, model_selector, budget_manager, executor, evaluator, ledger):
        ...

    def dispatch(self, raw_request: str, budget_policy: dict, repo_context: dict | None = None, executor_type: str = "noop") -> DispatchRecord:
        # 1. Analyze task
        analysis = self.task_analyzer.analyze(raw_request, repo_context)

        # 2. Select model
        selected, fallback, shadow_routes, reason = self.model_selector.select(analysis, budget_policy)

        # 3. Reserve budget
        reservation = self.budget_manager.reserve(analysis, selected, budget_policy)

        # 4. Generate decision
        decision = DispatchDecision(...)

        # 5. Write dispatch record
        record = self.ledger.create_record(decision)

        # 6. Execute (noop by default)
        exec_result = self.executor.execute(decision, {"raw_request": raw_request})
        record.execution_result_id = exec_result.result_id

        # 7. Evaluate
        eval_result = self.evaluator.evaluate(decision, exec_result)
        record.evaluation_result_id = eval_result.evaluation_id

        # 8. Update ledger
        record.final_status = "completed" if eval_result.passed else "failed"
        self.ledger.update_record(record)

        return record
```

### 5.5 ExecutorAdapter (executor_adapter.py)

**v0 executors:**

| Type | Status | Behavior |
|------|--------|----------|
| `noop` | enabled by default | Returns `status="planned"`, no execution |
| `mock` | enabled by explicit param | Returns deterministic fake output |
| `manual` | enabled by explicit param | Generates prompt pack, waits for human paste |
| `provider` | reserved enum, disabled in v0 | Future: real model calls |

### 5.6 EvaluationStub (evaluation_stub.py)

**v0 checks:**

| Check | Type | Description |
|-------|------|-------------|
| `schema_validity` | deterministic | Output matches expected schema |
| `rule_check` | deterministic | No boundary violations detected |
| `human_review_required` | deterministic | High risk or low confidence → needs human |

### 5.7 DispatchLedger (dispatch_ledger.py)

**Storage: JSONL file (`data/dispatch_ledger.jsonl`)**

Each line is a full DispatchRecord snapshot. Enables:
- Replay of any dispatch decision
- Cost_of_pass aggregation (links to usage_ledger via usage_ledger_row_id)
- Audit trail for governance

---

## 6. Execution Gates (Simplified v0)

v0 uses simplified gates that can map to the full governance 5-gate system later:

| Gate | Condition | Maps to governance |
|------|-----------|-------------------|
| `budget_gate` | budget_reservation.violation | — |
| `risk_gate` | high/critical risk or destructive/provider/deploy flags | scope_gate |
| `boundary_gate` | provider/sandbox/target_write detected | scope_gate |
| `confidence_gate` | confidence < 0.5 | evidence_gate |
| `manual_review_gate` | human_review_required | approval_gate |
| `provider_disabled_gate` | executor_type=provider blocked in v0 | — |
| `sandbox_disabled_gate` | sandbox execution blocked | — |
| `target_write_gate` | target repo mutation detected | scope_gate |

---

## 7. API Endpoints

### POST /api/dispatch

Create a dispatch decision (default: noop execution).

```json
// Request
{
  "request": "review this deployment config for security issues",
  "repo_id": "infra-config-lab",
  "budget_policy": {"max_cost_usd": 0.50, "max_tokens": 50000},
  "executor_type": "noop"  // optional, default "noop"
}

// Response
{
  "dispatch_id": "disp_abc123",
  "decision": { ... DispatchDecision ... },
  "execution_result": { ... ExecutionResult ... },
  "evaluation_result": { ... EvaluationResult ... }
}
```

### POST /api/dispatch/{dispatch_id}/execute

Execute a pending dispatch with a specific executor.

```json
// Request
{
  "executor_type": "mock",
  "manual_output": "..."  // only for manual executor
}

// Response
{
  "execution_result": { ... ExecutionResult ... },
  "evaluation_result": { ... EvaluationResult ... }
}
```

### GET /api/dispatch/{dispatch_id}

Get full dispatch record chain.

### GET /api/dispatch/ledger

List dispatch records with filters.

---

## 8. Integration with Existing Components

| Existing Component | Relationship |
|---|---|
| `model_profiles.py` | ModelSelector reads ModelHarnessProfile, uses TIERS enum |
| `usage_ledger.py` | DispatchRecord references usage_ledger_row_id after execution |
| `shadow_routing.py` | Shadow routes generated by ModelSelector, stored in DispatchDecision |
| `routing.py` | RoutingPolicy.tier_map consulted by ModelSelector |
| `resource_planner.py` | Marked as "existing planning subsystem / compatible adapter" — not replaced |
| `governance.py` | Simplified gates map to full governance gates for future upgrade |
| `error_taxonomy.py` | ExecutionResult.error uses ErrorRecord shape |
| `plan_triage.py` | Unchanged, remains for portfolio-level triage |
| `model_gateway.py` | StubModelProvider used by MockExecutor |
| `kernel.py` | Unchanged, remains for event store coordination |

---

## 9. Test Strategy

Each module has dedicated unit tests. Integration tests cover the full dispatch loop.

**Unit tests:**
- `test_task_analyzer.py`: 20+ fixture requests, verify domain/intent/risk_flags/complexity/evidence
- `test_model_selector.py`: tier selection logic, shadow route generation, budget filtering
- `test_budget_manager.py`: reserve/consume/release/violate lifecycle
- `test_dispatch_engine.py`: full loop with noop/mock executors
- `test_executor_adapter.py`: each executor type
- `test_evaluation_stub.py`: schema_validity, rule_check, human_review_required
- `test_dispatch_ledger.py`: record creation, update, query

**Integration tests:**
- Full dispatch loop: request → analysis → decision → mock execution → evaluation → ledger
- Edge cases: low confidence escalation, budget violation, boundary detection
- Negation-aware evidence: "do not deploy" should NOT trigger deployment risk_flag

---

## 10. What v0 Does NOT Do

- No real model/provider calls (provider executor disabled)
- No sandbox/process/VM execution
- No target repo writes
- No autonomous workers
- No concurrent dispatch management
- No dynamic budget adjustment based on real usage
- No LLM-based quality evaluation
- No adaptive routing based on historical performance

These are all v1+ features that build on the v0 decision loop.

---

## 11. File Inventory

| File | Type | Description |
|------|------|-------------|
| `src/harness_core/dispatch/__init__.py` | NEW | Package init |
| `src/harness_core/dispatch/task_analyzer.py` | NEW | RuleBasedTaskAnalyzer |
| `src/harness_core/dispatch/model_selector.py` | NEW | ModelSelector + shadow dual-track |
| `src/harness_core/dispatch/budget_manager.py` | NEW | BudgetManager |
| `src/harness_core/dispatch/dispatch_decision.py` | NEW | Schemas: DispatchDecision, BudgetReservation, ShadowRoute |
| `src/harness_core/dispatch/dispatch_engine.py` | NEW | DispatchEngine orchestrator |
| `src/harness_core/dispatch/executor_adapter.py` | NEW | NoopExecutor, MockExecutor, ManualExecutor |
| `src/harness_core/dispatch/evaluation_stub.py` | NEW | EvaluationStub |
| `src/harness_core/dispatch/dispatch_ledger.py` | NEW | DispatchRecord, DispatchLedger |
| `tests/test_task_analyzer.py` | NEW | TaskAnalyzer tests |
| `tests/test_model_selector.py` | NEW | ModelSelector tests |
| `tests/test_budget_manager.py` | NEW | BudgetManager tests |
| `tests/test_dispatch_engine.py` | NEW | DispatchEngine integration tests |
| `tests/test_executor_adapter.py` | NEW | Executor tests |
| `tests/test_evaluation_stub.py` | NEW | Evaluation tests |
| `tests/test_dispatch_ledger.py` | NEW | Ledger tests |
| `docs/dispatch/DISPATCHER_KERNEL_V0_ARCHITECTURE.md` | NEW | This file |
