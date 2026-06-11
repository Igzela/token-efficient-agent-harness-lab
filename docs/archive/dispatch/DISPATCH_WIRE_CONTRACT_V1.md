# Dispatch Wire Contract v1

Status: Governed v1 compatibility contract.

Purpose: define the semantic JSON contract shared by the Rust runtime and SDK surfaces.

Machine-readable schemas live in `wire_contract/v1/*.schema.json`.
Normalized language-migration golden fixtures live in `tests/fixtures/dispatch_wire/v1/`.
The Rust parity gate is `cargo test -p engine --test dispatch_parity`.

## Contract Rules

- JSON keys use `snake_case`.
- Objects include all fields emitted by the current Rust serializers.
- Optional values are encoded as `null`, not omitted.
- Tuples and Python lists are encoded as JSON arrays.
- Object key order is not semantic. Parity tests should compare parsed JSON after normalizing dynamic IDs and timestamps.
- Timestamps use the existing Python ISO-8601 string form from `datetime.now(timezone.utc).isoformat()`.
- Schema versions are string constants and must change when an incompatible field change is made.
- Rust parity implementations must not add real provider calls, process execution, target writes, or autonomous workers.

## DispatchRequest v1

Current Python entrypoints accept a dictionary in the SDK and raw parameters in `DispatchEngine.dispatch()`. The frozen wire request is:

```json
{
  "schema_version": "dispatch_request.v1",
  "raw_request": "string, required",
  "request_source": "cli | api | dashboard | agent | workflow | test_fixture"
}
```

Compatibility rule: Python callers that omit `schema_version` remain accepted. `request_source` defaults to `api` in the SDK and `test_fixture` in `DispatchEngine.dispatch()`.

## TaskAnalysis v1

Source: `TaskAnalysis` in `engine/src/task_analyzer/mod.rs`.

Required object fields:

```text
schema_version: "task_analysis.v1"
analysis_id: string
raw_request_snapshot: string
request_source: enum
primary_task_type: string
task_domain: enum
task_intent: enum
risk_flags: array<string>
complexity_score: number
cognitive_complexity: number
context_complexity: number
execution_risk: number
ambiguity_score: number
required_capabilities: array<string>
context_budget_estimate: integer
execution_budget_estimate: integer
quality_requirement: enum
risk_level: enum
confidence: number
confidence_label: "low" | "medium" | "high"
uncertainty_reason: array<string>
safe_default: string
escalation_trigger: string | null
positive_evidence: array<Evidence>
negative_evidence: array<Evidence>
features_detected: object
analysis_method: "rule_only"
created_at: string
```

`Evidence` object fields:

```text
feature: string
text: string
span: array<integer, 2>
polarity: "positive" | "negative"
source: "raw_request" | "repo_context" | "user_constraints" | "target_metadata"
rule_id: string | null
confidence: number
negation_scope: string | null
```

## DispatchDecision v1

Source: `DispatchDecision` in `engine/src/dispatch_decision.rs`.

Required object fields:

```text
schema_version: "dispatch_decision.v1"
decision_id: string
analysis_id: string
analysis_snapshot: TaskAnalysis object
selected_tier: enum
selected_profile_id: string | null
fallback_tier: enum
fallback_profile_id: string | null
shadow_routes: array<ShadowRoute>
hard_constraints: array<string>
rejected_candidates: array<RejectedCandidate>
no_shadow_route_reason: string | null
max_input_tokens: integer
max_output_tokens: integer
routing_reason: string
quality_requirement: enum
expected_quality_band: "low" | "medium" | "high" | "unknown"
confidence: number
confidence_label: "low" | "medium" | "high"
budget_reservation: BudgetReservation
execution_policy: object
execution_gates: array<ExecutionGate>
routing_mode: string
routing_experiment_id: string | null
decision_status: "decided" | "needs_approval" | "blocked" | "diagnostic_only"
created_at: string
```

`BudgetReservation` object fields:

```text
schema_version: "budget_reservation.v1"
reservation_id: string
decision_id: string
currency: string
pricing_snapshot_id: string | null
pre_budget: integer
reserved_input_tokens: integer
reserved_output_tokens: integer
reserved_total_tokens: integer
reserved_cost: number
budget_policy_id: string | null
budget_gate: string | null
status: string
actual_usage_ref: string | null
budget_delta: integer | null
budget_violation: boolean
created_at: string
updated_at: string
expires_at: string | null
```

`ShadowRoute` object fields: `tier`, `profile_id`, `reason`, `admission_scope`, `estimated_cost`, `expected_tradeoff`.

`RejectedCandidate` object fields: `tier`, `profile_id`, `reason`, `constraint_failed`, `estimated_cost`.

`ExecutionGate` object fields: `gate_id`, `gate_type`, `severity`, `reason`, `evidence_refs`, `clearance_required`, `cleared`, `cleared_by`, `cleared_at`.

## ExecutionResult v1

Source: `ExecutionResult` in `engine/src/executor_adapter.rs`.

Required object fields:

```text
schema_version: "execution_result.v1"
result_id: string
dispatch_id: string
decision_id: string
executor_type: "noop" | "mock" | "manual" | "provider" | "claude_code_cli" | "codex_cli"
status: "not_executed" | "preview_generated" | "mock_completed" | "manual_pending" | "manual_completed" | "failed" | "cli_completed" | "provider_completed"
output: string | null
prompt_pack: object | null
input_tokens: integer | null
output_tokens: integer | null
estimated_cost: number | null
latency_ms: integer | null
error_domain: string | null
error_message: string | null
provider_request_id: string | null
attempt_number: integer | null
finish_reason: string | null
usage_source: string | null
created_at: string
```

## EvaluationResult v1

Source: `EvaluationResult` in `engine/src/evaluation_stub.rs`.

Required object fields:

```text
schema_version: "evaluation_result.v1"
evaluation_id: string
dispatch_id: string
decision_id: string
execution_result_id: string
status: "pass" | "fail" | "needs_human_review" | "not_evaluated"
checks: array<EvaluationCheck>
quality_score: number | null
requires_retry: boolean
retry_reason: string | null
created_at: string
```

`EvaluationCheck` object fields: `check_id`, `name`, `status`, `reason`.

## DispatchRecord and Bundle

`DispatchRecord` object fields:

```text
schema_version: "dispatch_record.v1"
dispatch_id: string
request_snapshot: string
task_analysis_id: string
decision_id: string
execution_result_id: string | null
evaluation_result_id: string | null
usage_ledger_row_id: string | null
budget_reservation_id: string | null
final_status: enum
created_at: string
updated_at: string
```

`DispatchBundle` is a JSON object with these keys:

```text
record: DispatchRecord
analysis: TaskAnalysis
decision: DispatchDecision
execution_result: ExecutionResult
evaluation_result: EvaluationResult
```

## First Rust Parity Scope

The first Rust implementation slice may cover only:

- `event_schema`
- `task_analyzer`
- `dispatch_decision`

It must consume or produce the frozen shapes above before API, SDK, dashboard, provider, storage, or deployment work starts.
