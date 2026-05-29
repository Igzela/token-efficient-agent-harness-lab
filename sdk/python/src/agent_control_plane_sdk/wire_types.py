from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Literal, TypedDict

RequestSource = Literal["cli", "api", "dashboard", "agent", "workflow", "test_fixture"]

ModelTier = Literal["cheap_executor", "balanced_worker", "strong_planner", "verifier", "advisor"]
TaskDomain = Literal["code", "docs", "config", "infra", "math", "architecture", "repo_ops", "governance", "other"]
TaskIntent = Literal["generate", "review", "debug", "summarize", "audit", "plan", "refactor", "compare", "explain", "classify"]
RiskFlag = Literal["target_write", "provider_call", "sandbox_execution", "deployment", "secret_handling", "destructive_operation", "long_context", "high_uncertainty"]
QualityRequirement = Literal["draft", "standard", "high", "critical"]
RiskLevel = Literal["low", "medium", "high", "critical"]
ConfidenceLabel = Literal["low", "medium", "high"]
EvidencePolarity = Literal["positive", "negative"]
EvidenceSource = Literal["raw_request", "repo_context", "user_constraints", "target_metadata"]
ExpectedQualityBand = Literal["low", "medium", "high", "unknown"]
DecisionStatus = Literal["decided", "needs_approval", "blocked", "diagnostic_only"]
GateSeverity = Literal["info", "warning", "block", "critical"]
ExecutorType = Literal["noop", "mock", "manual", "provider"]
ExecutionStatus = Literal["not_executed", "preview_generated", "mock_completed", "manual_pending", "manual_completed", "failed"]
EvaluationStatus = Literal["pass", "fail", "needs_human_review", "not_evaluated"]
CheckStatus = Literal["pass", "fail", "warning", "skipped"]
FinalStatus = Literal["dispatched", "executing", "completed", "failed", "escalated", "cancelled", "not_executed", "manual_pending"]

@dataclass(frozen=True)
class DispatchRequest:
    raw_request: str
    request_source: RequestSource = "api"
    schema_version: Literal["dispatch_request.v1"] = "dispatch_request.v1"

    def to_json(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "raw_request": self.raw_request,
            "request_source": self.request_source,
        }

class Evidence(TypedDict):
    feature: str
    text: str
    span: tuple[int, int]
    polarity: EvidencePolarity
    source: EvidenceSource
    rule_id: str | None
    confidence: float
    negation_scope: str | None

class TaskAnalysis(TypedDict):
    schema_version: Literal["task_analysis.v1"]
    analysis_id: str
    raw_request_snapshot: str
    request_source: RequestSource
    primary_task_type: str
    task_domain: TaskDomain
    task_intent: TaskIntent
    risk_flags: list[RiskFlag]
    complexity_score: float
    cognitive_complexity: float
    context_complexity: float
    execution_risk: float
    ambiguity_score: float
    required_capabilities: list[str]
    context_budget_estimate: int
    execution_budget_estimate: int
    quality_requirement: QualityRequirement
    risk_level: RiskLevel
    confidence: float
    confidence_label: ConfidenceLabel
    uncertainty_reason: list[str]
    safe_default: str
    escalation_trigger: str | None
    positive_evidence: list[Evidence]
    negative_evidence: list[Evidence]
    features_detected: dict[str, Any]
    analysis_method: Literal["rule_only"]
    created_at: str

class BudgetReservation(TypedDict):
    schema_version: Literal["budget_reservation.v1"]
    reservation_id: str
    decision_id: str
    currency: str
    pricing_snapshot_id: str | None
    pre_budget: int
    reserved_input_tokens: int
    reserved_output_tokens: int
    reserved_total_tokens: int
    reserved_cost: float
    budget_policy_id: str | None
    budget_gate: str | None
    status: str
    actual_usage_ref: str | None
    budget_delta: int | None
    budget_violation: bool
    created_at: str
    updated_at: str
    expires_at: str | None

class ExecutionGate(TypedDict):
    gate_id: str
    gate_type: str
    severity: GateSeverity
    reason: str
    evidence_refs: list[str]
    clearance_required: str
    cleared: bool
    cleared_by: str | None
    cleared_at: str | None

class ShadowRoute(TypedDict):
    tier: ModelTier
    profile_id: str | None
    reason: str
    admission_scope: str
    estimated_cost: float | None
    expected_tradeoff: str

class RejectedCandidate(TypedDict):
    tier: ModelTier
    profile_id: str | None
    reason: str
    constraint_failed: str | None
    estimated_cost: float | None

class DispatchDecision(TypedDict):
    schema_version: Literal["dispatch_decision.v1"]
    decision_id: str
    analysis_id: str
    analysis_snapshot: dict[str, Any]
    selected_tier: ModelTier
    selected_profile_id: str | None
    fallback_tier: ModelTier
    fallback_profile_id: str | None
    shadow_routes: list[ShadowRoute]
    hard_constraints: list[str]
    rejected_candidates: list[RejectedCandidate]
    no_shadow_route_reason: str | None
    max_input_tokens: int
    max_output_tokens: int
    routing_reason: str
    quality_requirement: QualityRequirement
    expected_quality_band: ExpectedQualityBand
    confidence: float
    confidence_label: ConfidenceLabel
    budget_reservation: BudgetReservation
    execution_policy: dict[str, Any]
    execution_gates: list[ExecutionGate]
    routing_mode: str
    routing_experiment_id: str | None
    decision_status: DecisionStatus
    created_at: str

class ExecutionResult(TypedDict):
    schema_version: Literal["execution_result.v1"]
    result_id: str
    dispatch_id: str
    decision_id: str
    executor_type: ExecutorType
    status: ExecutionStatus
    output: str | None
    prompt_pack: dict[str, Any] | None
    input_tokens: int | None
    output_tokens: int | None
    estimated_cost: float | None
    latency_ms: int | None
    error_domain: str | None
    error_message: str | None
    provider_request_id: str | None
    attempt_number: int | None
    finish_reason: str | None
    usage_source: str | None
    created_at: str

class EvaluationCheck(TypedDict):
    check_id: str
    name: str
    status: CheckStatus
    reason: str

class EvaluationResult(TypedDict):
    schema_version: Literal["evaluation_result.v1"]
    evaluation_id: str
    dispatch_id: str
    decision_id: str
    execution_result_id: str
    status: EvaluationStatus
    checks: list[EvaluationCheck]
    quality_score: float | None
    requires_retry: bool
    retry_reason: str | None
    created_at: str

class DispatchRecord(TypedDict):
    schema_version: Literal["dispatch_record.v1"]
    dispatch_id: str
    request_snapshot: str
    task_analysis_id: str
    decision_id: str
    execution_result_id: str | None
    evaluation_result_id: str | None
    usage_ledger_row_id: str | None
    budget_reservation_id: str | None
    final_status: FinalStatus
    created_at: str
    updated_at: str

class DispatchBundle(TypedDict):
    record: DispatchRecord
    analysis: TaskAnalysis
    decision: DispatchDecision
    execution_result: ExecutionResult
    evaluation_result: EvaluationResult

class LocalTierCost(TypedDict):
    selected_tier: str
    dispatch_count: int
    reserved_cost: float
    estimated_cost_usd: float
    input_tokens: int
    output_tokens: int

class LocalDailyCost(TypedDict):
    date: str
    dispatch_count: int
    reserved_cost: float
    estimated_cost_usd: float

class LocalCostSummary(TypedDict):
    schema_version: Literal["local_cost_summary.v2"]
    currency: str
    dispatch_count: int
    total_reserved_cost: float
    total_estimated_cost_usd: float
    total_input_tokens: int
    total_output_tokens: int
    cost_utilization: float
    by_tier: list[LocalTierCost]
    daily: list[LocalDailyCost]

class LocalDispatchCostRow(TypedDict):
    history_id: int
    dispatch_id: str
    created_at: str
    selected_tier: str
    reserved_cost: float
    input_tokens: int
    output_tokens: int
    estimated_cost_usd: float
    executor_type: str
    latency_ms: int | None

class LocalDispatchCostDetail(TypedDict):
    schema_version: Literal["local_dispatch_cost_detail.v1"]
    dispatches: list[LocalDispatchCostRow]

class ApiStatus(TypedDict, total=False):
    schema_version: Literal["axum_api.v1"]
    status: str
    tenant_id: str
