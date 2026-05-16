# Advisor Broker Spec

## Purpose

Implement the Advisor Protocol lifecycle — preflight, correction, arbitration, and risk scan — as a structured interface that can be backed by a stub or a real model.

## Data Structures

### AdvisorContextPack

```python
@dataclass(frozen=True)
class AdvisorContextPack:
    task_id: str
    call_type: str  # preflight | correction | arbitration | risk_scan
    task_spec: dict[str, Any]
    completion: dict[str, Any] | None
    handoff_pack: dict[str, Any] | None
    run_log_text: str | None
    failure_code: str | None
    project_context: dict[str, Any] | None
```

### AdvisorResponse

```python
@dataclass(frozen=True)
class AdvisorResponse:
    call_type: str
    diagnosis: str
    recommended_action: str
    do_not_do: str
    confidence: float  # 0.0 - 1.0
    token_usage: int
    provider: str  # "stub" | provider name
    raw_response: dict[str, Any] | None = None
```

### AdvisorBudget

```python
@dataclass(frozen=True)
class AdvisorBudget:
    max_tokens: int
    max_calls_per_task: int
    current_calls: int = 0
    current_tokens: int = 0
```

## APIs

### AdvisorProvider Protocol

```python
class AdvisorProvider(Protocol):
    def invoke(self, context: AdvisorContextPack, budget: AdvisorBudget) -> AdvisorResponse
```

### StubAdvisorProvider

Deterministic stub. Returns fixed responses based on context fields.

- Preflight: always "go" with confidence 0.9
- Correction: returns generic fix guidance based on failure_code
- Arbitration: returns "pass" if score >= 0.6, "fail" otherwise
- Risk Scan: returns risk_level based on requested_action

### AdvisorBroker

```python
class AdvisorBroker:
    def __init__(self, provider: AdvisorProvider, budget: AdvisorBudget)
    def preflight(context: AdvisorContextPack) -> AdvisorResponse
    def correction(context: AdvisorContextPack) -> AdvisorResponse
    def arbitration(context: AdvisorContextPack) -> AdvisorResponse
    def risk_scan(context: AdvisorContextPack) -> AdvisorResponse
```

### AdvisorProtocolValidator

```python
class AdvisorProtocolValidator:
    def validate_response(response: AdvisorResponse) -> ValidationResult
    def validate_context_pack(pack: AdvisorContextPack) -> ValidationResult
    def validate_budget(budget: AdvisorBudget) -> ValidationResult
```

## Call Types

| Call Type | When | Input | Output |
|-----------|------|-------|--------|
| Preflight | Before task execution starts | task_spec, project_context | go / no-go / modify_scope |
| Correction | After failure, before retry | failure_code, run_log, completion | fix guidance |
| Arbitration | When quality gate is ambiguous | all evidence | pass / fail / escalate |
| Risk Scan | Before high-risk action | approval_request, affected_files | risk_level, mitigation |

## Token Budget Rules

- `max_tokens`: Maximum tokens per advisor call (default: 2000)
- `max_calls_per_task`: Maximum advisor invocations per task (default: 3)
- `current_calls` / `current_tokens`: Tracked per task lifecycle
- Budget exceeded: Return structured error, do NOT make model call
- Budget is per-task, not per-run

## Failure Behavior

- Budget exceeded returns a budget_exceeded error, not a model call
- Missing context fields produce a structured error response, not a crash
- Stub provider never fails unexpectedly
- Confidence is clamped 0.0 to 1.0

## Dependencies

- `TaskRecordBundle` from `task_records.py`
- `CANONICAL_FAILURE_CODES` from `validators.py`
- `FinalGateDecision` from `final_gate.py`
- `ValidationResult` from `validators.py`
