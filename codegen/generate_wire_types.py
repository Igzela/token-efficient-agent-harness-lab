#!/usr/bin/env python3
"""Generate SDK wire type stubs from wire_contract/v1 schemas."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCHEMA_DIR = ROOT / "wire_contract" / "v1"
TS_OUT = ROOT / "sdk" / "typescript" / "src" / "wire-types.ts"
PY_OUT = ROOT / "sdk" / "python" / "src" / "agent_control_plane_sdk" / "wire_types.py"
RUST_OUT = ROOT / "engine" / "src" / "wire_types.rs"

SCHEMA_FILES = [
    "dispatch_request.schema.json",
    "task_analysis.schema.json",
    "dispatch_decision.schema.json",
    "execution_result.schema.json",
    "evaluation_result.schema.json",
    "dispatch_bundle.schema.json",
]


def load_schema(name: str) -> dict:
    return json.loads((SCHEMA_DIR / name).read_text(encoding="utf-8"))


def load_all_schemas() -> dict[str, dict]:
    return {name: load_schema(name) for name in SCHEMA_FILES}


def ts_union(values: list[str]) -> str:
    return " | ".join('"' + v + '"' for v in values)


def py_literal(values: list[str]) -> str:
    return "Literal[" + ", ".join('"' + v + '"' for v in values) + "]"


def render_ts(schemas: dict[str, dict]) -> str:
    lines: list[str] = []

    request_sources = schemas["dispatch_request.schema.json"]["properties"]["request_source"]["enum"]
    model_tiers = ["cheap_executor", "balanced_worker", "strong_planner", "verifier", "advisor"]
    task_domains = ["code", "docs", "config", "infra", "math", "architecture", "repo_ops", "governance", "other"]
    task_intents = ["generate", "review", "debug", "summarize", "audit", "plan", "refactor", "compare", "explain", "classify"]
    risk_flags = ["target_write", "provider_call", "sandbox_execution", "deployment", "secret_handling", "destructive_operation", "long_context", "high_uncertainty"]
    quality_reqs = ["draft", "standard", "high", "critical"]
    risk_levels = ["low", "medium", "high", "critical"]
    confidence_labels = ["low", "medium", "high"]
    evidence_polarities = ["positive", "negative"]
    evidence_sources = ["raw_request", "repo_context", "user_constraints", "target_metadata"]
    expected_quality_bands = ["low", "medium", "high", "unknown"]
    decision_statuses = ["decided", "needs_approval", "blocked", "diagnostic_only"]
    gate_severities = ["info", "warning", "block", "critical"]
    executor_types = ["noop", "mock", "manual", "provider"]
    execution_statuses = ["not_executed", "preview_generated", "mock_completed", "manual_pending", "manual_completed", "failed"]
    evaluation_statuses = ["pass", "fail", "needs_human_review", "not_evaluated"]
    check_statuses = ["pass", "fail", "warning", "skipped"]
    final_statuses = ["dispatched", "executing", "completed", "failed", "escalated", "cancelled", "not_executed", "manual_pending"]

    lines.append("type RequestSource = " + ts_union(request_sources) + ";")
    lines.append("")
    lines.append("type ModelTier = " + ts_union(model_tiers) + ";")
    lines.append("type TaskDomain = " + ts_union(task_domains) + ";")
    lines.append("type TaskIntent = " + ts_union(task_intents) + ";")
    lines.append("type RiskFlag = " + ts_union(risk_flags) + ";")
    lines.append("type QualityRequirement = " + ts_union(quality_reqs) + ";")
    lines.append("type RiskLevel = " + ts_union(risk_levels) + ";")
    lines.append("type ConfidenceLabel = " + ts_union(confidence_labels) + ";")
    lines.append("type EvidencePolarity = " + ts_union(evidence_polarities) + ";")
    lines.append("type EvidenceSource = " + ts_union(evidence_sources) + ";")
    lines.append("type ExpectedQualityBand = " + ts_union(expected_quality_bands) + ";")
    lines.append("type DecisionStatus = " + ts_union(decision_statuses) + ";")
    lines.append("type GateSeverity = " + ts_union(gate_severities) + ";")
    lines.append("type ExecutorType = " + ts_union(executor_types) + ";")
    lines.append("type ExecutionStatus = " + ts_union(execution_statuses) + ";")
    lines.append("type EvaluationStatus = " + ts_union(evaluation_statuses) + ";")
    lines.append("type CheckStatus = " + ts_union(check_statuses) + ";")
    lines.append("type FinalStatus = " + ts_union(final_statuses) + ";")
    lines.append("")

    interfaces = [
        ("Evidence", ["feature: string;", "text: string;", "span: [number, number];", "polarity: EvidencePolarity;", "source: EvidenceSource;", "rule_id: string | null;", "confidence: number;", "negation_scope: string | null;"]),
        ("TaskAnalysis", ['schema_version: "task_analysis.v1";', "analysis_id: string;", "raw_request_snapshot: string;", "request_source: RequestSource;", "primary_task_type: string;", "task_domain: TaskDomain;", "task_intent: TaskIntent;", "risk_flags: RiskFlag[];", "complexity_score: number;", "cognitive_complexity: number;", "context_complexity: number;", "execution_risk: number;", "ambiguity_score: number;", "required_capabilities: string[];", "context_budget_estimate: number;", "execution_budget_estimate: number;", "quality_requirement: QualityRequirement;", "risk_level: RiskLevel;", "confidence: number;", "confidence_label: ConfidenceLabel;", "uncertainty_reason: string[];", "safe_default: string;", "escalation_trigger: string | null;", "positive_evidence: Evidence[];", "negative_evidence: Evidence[];", "features_detected: Record<string, unknown>;", 'analysis_method: "rule_only";', "created_at: string;"]),
        ("BudgetReservation", ['schema_version: "budget_reservation.v1";', "reservation_id: string;", "decision_id: string;", "currency: string;", "pricing_snapshot_id: string | null;", "pre_budget: number;", "reserved_input_tokens: number;", "reserved_output_tokens: number;", "reserved_total_tokens: number;", "reserved_cost: number;", "budget_policy_id: string | null;", "budget_gate: string | null;", "status: string;", "actual_usage_ref: string | null;", "budget_delta: number | null;", "budget_violation: boolean;", "created_at: string;", "updated_at: string;", "expires_at: string | null;"]),
        ("ExecutionGate", ["gate_id: string;", "gate_type: string;", "severity: GateSeverity;", "reason: string;", "evidence_refs: string[];", "clearance_required: string;", "cleared: boolean;", "cleared_by: string | null;", "cleared_at: string | null;"]),
        ("ShadowRoute", ["tier: ModelTier;", "profile_id: string | null;", "reason: string;", "admission_scope: string;", "estimated_cost: number | null;", "expected_tradeoff: string;"]),
        ("RejectedCandidate", ["tier: ModelTier;", "profile_id: string | null;", "reason: string;", "constraint_failed: string | null;", "estimated_cost: number | null;"]),
        ("DispatchDecision", ['schema_version: "dispatch_decision.v1";', "decision_id: string;", "analysis_id: string;", "analysis_snapshot: Record<string, unknown>;", "selected_tier: ModelTier;", "selected_profile_id: string | null;", "fallback_tier: ModelTier;", "fallback_profile_id: string | null;", "shadow_routes: ShadowRoute[];", "hard_constraints: string[];", "rejected_candidates: RejectedCandidate[];", "no_shadow_route_reason: string | null;", "max_input_tokens: number;", "max_output_tokens: number;", "routing_reason: string;", "quality_requirement: QualityRequirement;", "expected_quality_band: ExpectedQualityBand;", "confidence: number;", "confidence_label: ConfidenceLabel;", "budget_reservation: BudgetReservation;", "execution_policy: Record<string, unknown>;", "execution_gates: ExecutionGate[];", "routing_mode: string;", "routing_experiment_id: string | null;", "decision_status: DecisionStatus;", "created_at: string;"]),
        ("ExecutionResult", ['schema_version: "execution_result.v1";', "result_id: string;", "dispatch_id: string;", "decision_id: string;", "executor_type: ExecutorType;", "status: ExecutionStatus;", "output: string | null;", "prompt_pack: Record<string, unknown> | null;", "input_tokens: number | null;", "output_tokens: number | null;", "estimated_cost: number | null;", "latency_ms: number | null;", "error_domain: string | null;", "error_message: string | null;", "provider_request_id: string | null;", "attempt_number: number | null;", "finish_reason: string | null;", "usage_source: string | null;", "created_at: string;"]),
        ("EvaluationCheck", ["check_id: string;", "name: string;", "status: CheckStatus;", "reason: string;"]),
        ("EvaluationResult", ['schema_version: "evaluation_result.v1";', "evaluation_id: string;", "dispatch_id: string;", "decision_id: string;", "execution_result_id: string;", "status: EvaluationStatus;", "checks: EvaluationCheck[];", "quality_score: number | null;", "requires_retry: boolean;", "retry_reason: string | null;", "created_at: string;"]),
        ("DispatchRecord", ['schema_version: "dispatch_record.v1";', "dispatch_id: string;", "request_snapshot: string;", "task_analysis_id: string;", "decision_id: string;", "execution_result_id: string | null;", "evaluation_result_id: string | null;", "usage_ledger_row_id: string | null;", "budget_reservation_id: string | null;", "final_status: FinalStatus;", "created_at: string;", "updated_at: string;"]),
        ("DispatchBundle", ["record: DispatchRecord;", "analysis: TaskAnalysis;", "decision: DispatchDecision;", "execution_result: ExecutionResult;", "evaluation_result: EvaluationResult;"]),
        ("DispatchRequest", ['schema_version?: "dispatch_request.v1";', "raw_request: string;", "request_source: RequestSource;"]),
        ("ApiStatus", ['schema_version: "axum_api.v1";', "status: string;", "tenant_id?: string;"]),
    ]

    for name, fields in interfaces:
        lines.append("export interface " + name + " {")
        for f in fields:
            lines.append("  " + f)
        lines.append("}")
        lines.append("")

    return "\n".join(lines)


def render_python(schemas: dict[str, dict]) -> str:
    lines: list[str] = []

    lines.append("from __future__ import annotations")
    lines.append("")
    lines.append("from dataclasses import dataclass")
    lines.append("from typing import Any, Literal, TypedDict")
    lines.append("")

    request_sources = schemas["dispatch_request.schema.json"]["properties"]["request_source"]["enum"]
    model_tiers = ["cheap_executor", "balanced_worker", "strong_planner", "verifier", "advisor"]
    task_domains = ["code", "docs", "config", "infra", "math", "architecture", "repo_ops", "governance", "other"]
    task_intents = ["generate", "review", "debug", "summarize", "audit", "plan", "refactor", "compare", "explain", "classify"]
    risk_flags = ["target_write", "provider_call", "sandbox_execution", "deployment", "secret_handling", "destructive_operation", "long_context", "high_uncertainty"]
    quality_reqs = ["draft", "standard", "high", "critical"]
    risk_levels = ["low", "medium", "high", "critical"]
    confidence_labels = ["low", "medium", "high"]
    evidence_polarities = ["positive", "negative"]
    evidence_sources = ["raw_request", "repo_context", "user_constraints", "target_metadata"]
    expected_quality_bands = ["low", "medium", "high", "unknown"]
    decision_statuses = ["decided", "needs_approval", "blocked", "diagnostic_only"]
    gate_severities = ["info", "warning", "block", "critical"]
    executor_types = ["noop", "mock", "manual", "provider"]
    execution_statuses = ["not_executed", "preview_generated", "mock_completed", "manual_pending", "manual_completed", "failed"]
    evaluation_statuses = ["pass", "fail", "needs_human_review", "not_evaluated"]
    check_statuses = ["pass", "fail", "warning", "skipped"]
    final_statuses = ["dispatched", "executing", "completed", "failed", "escalated", "cancelled", "not_executed", "manual_pending"]

    lines.append("RequestSource = " + py_literal(request_sources))
    lines.append("")
    lines.append("ModelTier = " + py_literal(model_tiers))
    lines.append("TaskDomain = " + py_literal(task_domains))
    lines.append("TaskIntent = " + py_literal(task_intents))
    lines.append("RiskFlag = " + py_literal(risk_flags))
    lines.append("QualityRequirement = " + py_literal(quality_reqs))
    lines.append("RiskLevel = " + py_literal(risk_levels))
    lines.append("ConfidenceLabel = " + py_literal(confidence_labels))
    lines.append("EvidencePolarity = " + py_literal(evidence_polarities))
    lines.append("EvidenceSource = " + py_literal(evidence_sources))
    lines.append("ExpectedQualityBand = " + py_literal(expected_quality_bands))
    lines.append("DecisionStatus = " + py_literal(decision_statuses))
    lines.append("GateSeverity = " + py_literal(gate_severities))
    lines.append("ExecutorType = " + py_literal(executor_types))
    lines.append("ExecutionStatus = " + py_literal(execution_statuses))
    lines.append("EvaluationStatus = " + py_literal(evaluation_statuses))
    lines.append("CheckStatus = " + py_literal(check_statuses))
    lines.append("FinalStatus = " + py_literal(final_statuses))
    lines.append("")

    lines.append("@dataclass(frozen=True)")
    lines.append("class DispatchRequest:")
    lines.append("    raw_request: str")
    lines.append('    request_source: RequestSource = "api"')
    lines.append('    schema_version: Literal["dispatch_request.v1"] = "dispatch_request.v1"')
    lines.append("")
    lines.append("    def to_json(self) -> dict[str, Any]:")
    lines.append("        return {")
    lines.append('            "schema_version": self.schema_version,')
    lines.append('            "raw_request": self.raw_request,')
    lines.append('            "request_source": self.request_source,')
    lines.append("        }")
    lines.append("")

    typed_dicts = [
        ("Evidence", [("feature", "str"), ("text", "str"), ("span", "tuple[int, int]"), ("polarity", "EvidencePolarity"), ("source", "EvidenceSource"), ("rule_id", "str | None"), ("confidence", "float"), ("negation_scope", "str | None")]),
        ("TaskAnalysis", [("schema_version", 'Literal["task_analysis.v1"]'), ("analysis_id", "str"), ("raw_request_snapshot", "str"), ("request_source", "RequestSource"), ("primary_task_type", "str"), ("task_domain", "TaskDomain"), ("task_intent", "TaskIntent"), ("risk_flags", "list[RiskFlag]"), ("complexity_score", "float"), ("cognitive_complexity", "float"), ("context_complexity", "float"), ("execution_risk", "float"), ("ambiguity_score", "float"), ("required_capabilities", "list[str]"), ("context_budget_estimate", "int"), ("execution_budget_estimate", "int"), ("quality_requirement", "QualityRequirement"), ("risk_level", "RiskLevel"), ("confidence", "float"), ("confidence_label", "ConfidenceLabel"), ("uncertainty_reason", "list[str]"), ("safe_default", "str"), ("escalation_trigger", "str | None"), ("positive_evidence", "list[Evidence]"), ("negative_evidence", "list[Evidence]"), ("features_detected", "dict[str, Any]"), ("analysis_method", 'Literal["rule_only"]'), ("created_at", "str")]),
        ("BudgetReservation", [("schema_version", 'Literal["budget_reservation.v1"]'), ("reservation_id", "str"), ("decision_id", "str"), ("currency", "str"), ("pricing_snapshot_id", "str | None"), ("pre_budget", "int"), ("reserved_input_tokens", "int"), ("reserved_output_tokens", "int"), ("reserved_total_tokens", "int"), ("reserved_cost", "float"), ("budget_policy_id", "str | None"), ("budget_gate", "str | None"), ("status", "str"), ("actual_usage_ref", "str | None"), ("budget_delta", "int | None"), ("budget_violation", "bool"), ("created_at", "str"), ("updated_at", "str"), ("expires_at", "str | None")]),
        ("ExecutionGate", [("gate_id", "str"), ("gate_type", "str"), ("severity", "GateSeverity"), ("reason", "str"), ("evidence_refs", "list[str]"), ("clearance_required", "str"), ("cleared", "bool"), ("cleared_by", "str | None"), ("cleared_at", "str | None")]),
        ("ShadowRoute", [("tier", "ModelTier"), ("profile_id", "str | None"), ("reason", "str"), ("admission_scope", "str"), ("estimated_cost", "float | None"), ("expected_tradeoff", "str")]),
        ("RejectedCandidate", [("tier", "ModelTier"), ("profile_id", "str | None"), ("reason", "str"), ("constraint_failed", "str | None"), ("estimated_cost", "float | None")]),
        ("DispatchDecision", [("schema_version", 'Literal["dispatch_decision.v1"]'), ("decision_id", "str"), ("analysis_id", "str"), ("analysis_snapshot", "dict[str, Any]"), ("selected_tier", "ModelTier"), ("selected_profile_id", "str | None"), ("fallback_tier", "ModelTier"), ("fallback_profile_id", "str | None"), ("shadow_routes", "list[ShadowRoute]"), ("hard_constraints", "list[str]"), ("rejected_candidates", "list[RejectedCandidate]"), ("no_shadow_route_reason", "str | None"), ("max_input_tokens", "int"), ("max_output_tokens", "int"), ("routing_reason", "str"), ("quality_requirement", "QualityRequirement"), ("expected_quality_band", "ExpectedQualityBand"), ("confidence", "float"), ("confidence_label", "ConfidenceLabel"), ("budget_reservation", "BudgetReservation"), ("execution_policy", "dict[str, Any]"), ("execution_gates", "list[ExecutionGate]"), ("routing_mode", "str"), ("routing_experiment_id", "str | None"), ("decision_status", "DecisionStatus"), ("created_at", "str")]),
        ("ExecutionResult", [("schema_version", 'Literal["execution_result.v1"]'), ("result_id", "str"), ("dispatch_id", "str"), ("decision_id", "str"), ("executor_type", "ExecutorType"), ("status", "ExecutionStatus"), ("output", "str | None"), ("prompt_pack", "dict[str, Any] | None"), ("input_tokens", "int | None"), ("output_tokens", "int | None"), ("estimated_cost", "float | None"), ("latency_ms", "int | None"), ("error_domain", "str | None"), ("error_message", "str | None"), ("provider_request_id", "str | None"), ("attempt_number", "int | None"), ("finish_reason", "str | None"), ("usage_source", "str | None"), ("created_at", "str")]),
        ("EvaluationCheck", [("check_id", "str"), ("name", "str"), ("status", "CheckStatus"), ("reason", "str")]),
        ("EvaluationResult", [("schema_version", 'Literal["evaluation_result.v1"]'), ("evaluation_id", "str"), ("dispatch_id", "str"), ("decision_id", "str"), ("execution_result_id", "str"), ("status", "EvaluationStatus"), ("checks", "list[EvaluationCheck]"), ("quality_score", "float | None"), ("requires_retry", "bool"), ("retry_reason", "str | None"), ("created_at", "str")]),
        ("DispatchRecord", [("schema_version", 'Literal["dispatch_record.v1"]'), ("dispatch_id", "str"), ("request_snapshot", "str"), ("task_analysis_id", "str"), ("decision_id", "str"), ("execution_result_id", "str | None"), ("evaluation_result_id", "str | None"), ("usage_ledger_row_id", "str | None"), ("budget_reservation_id", "str | None"), ("final_status", "FinalStatus"), ("created_at", "str"), ("updated_at", "str")]),
        ("DispatchBundle", [("record", "DispatchRecord"), ("analysis", "TaskAnalysis"), ("decision", "DispatchDecision"), ("execution_result", "ExecutionResult"), ("evaluation_result", "EvaluationResult")]),
        ("ApiStatus", [("schema_version", 'Literal["axum_api.v1"]'), ("status", "str"), ("tenant_id", "str")]),
    ]

    for name, fields in typed_dicts:
        if name == "ApiStatus":
            lines.append("class ApiStatus(TypedDict, total=False):")
        else:
            lines.append("class " + name + "(TypedDict):")
        for fname, ftype in fields:
            lines.append("    " + fname + ": " + ftype)
        lines.append("")

    return "\n".join(lines)


def render_rust(schemas: dict[str, dict]) -> str:
    request_sources = schemas["dispatch_request.schema.json"]["properties"]["request_source"]["enum"]
    request_source_variants = [
        ("Cli", "cli"),
        ("Api", "api"),
        ("Dashboard", "dashboard"),
        ("Agent", "agent"),
        ("Workflow", "workflow"),
        ("TestFixture", "test_fixture"),
    ]
    assert sorted(value for _, value in request_source_variants) == sorted(request_sources)

    lines: list[str] = [
        "use serde::{Deserialize, Serialize};",
        "use serde_json::Value;",
        "",
        "#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]",
        "#[serde(rename_all = \"snake_case\")]",
        "pub enum RequestSource {",
    ]
    for variant, _value in request_source_variants:
        lines.append(f"    {variant},")
    lines.extend(
        [
            "}",
            "",
            "#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]",
            "pub struct DispatchRequest {",
            "    pub schema_version: String,",
            "    pub raw_request: String,",
            "    pub request_source: RequestSource,",
            "}",
            "",
            "#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]",
            "pub struct DispatchBundleValue {",
            "    pub record: Value,",
            "    pub analysis: Value,",
            "    pub decision: Value,",
            "    pub execution_result: Value,",
            "    pub evaluation_result: Value,",
            "}",
            "",
            "#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]",
            "pub struct ApiStatus {",
            "    pub schema_version: String,",
            "    pub status: String,",
            "    pub tenant_id: Option<String>,",
            "}",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> int:
    schemas = load_all_schemas()

    ts_out = render_ts(schemas)
    TS_OUT.write_text(ts_out, encoding="utf-8")
    print(f"wrote {TS_OUT.relative_to(ROOT)} ({len(ts_out)} bytes)")

    py_out = render_python(schemas)
    PY_OUT.write_text(py_out, encoding="utf-8")
    print(f"wrote {PY_OUT.relative_to(ROOT)} ({len(py_out)} bytes)")

    rust_out = render_rust(schemas)
    RUST_OUT.write_text(rust_out, encoding="utf-8")
    print(f"wrote {RUST_OUT.relative_to(ROOT)} ({len(rust_out)} bytes)")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
