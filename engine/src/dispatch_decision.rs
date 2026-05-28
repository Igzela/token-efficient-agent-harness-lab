use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::runtime::FixtureRuntime;
use crate::task_analyzer::analyze;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const BUDGET_RESERVATION_SCHEMA_VERSION: &str = "budget_reservation.v1";
pub const DISPATCH_DECISION_SCHEMA_VERSION: &str = "dispatch_decision.v1";

pub const TASK_DOMAINS: &[&str] = &[
    "code",
    "docs",
    "config",
    "infra",
    "math",
    "architecture",
    "repo_ops",
    "governance",
    "other",
];

pub const TASK_INTENTS: &[&str] = &[
    "generate",
    "review",
    "debug",
    "summarize",
    "audit",
    "plan",
    "refactor",
    "compare",
    "explain",
    "classify",
];

pub const RISK_FLAGS: &[&str] = &[
    "target_write",
    "provider_call",
    "sandbox_execution",
    "deployment",
    "secret_handling",
    "destructive_operation",
    "long_context",
    "high_uncertainty",
];

pub const RISK_LEVELS: &[&str] = &["low", "medium", "high", "critical"];

pub const QUALITY_REQUIREMENTS: &[&str] = &["draft", "standard", "high", "critical"];

pub const MODEL_TIERS: &[&str] = &[
    "cheap_executor",
    "balanced_worker",
    "strong_planner",
    "verifier",
    "advisor",
];

pub const EXECUTION_GATE_TYPES: &[&str] = &[
    "budget",
    "risk",
    "boundary",
    "confidence",
    "manual_review",
    "provider_disabled",
    "sandbox_disabled",
    "target_write",
];

pub const GATE_SEVERITIES: &[&str] = &["info", "warning", "block", "critical"];

pub const CLEARANCE_VALUES: &[&str] = &["none", "human", "governance", "policy"];

pub const DECISION_STATUSES: &[&str] = &["decided", "needs_approval", "blocked", "diagnostic_only"];

pub const EXECUTOR_TYPES: &[&str] = &["noop", "mock", "manual", "provider"];

pub const REQUEST_SOURCES: &[&str] = &[
    "cli",
    "api",
    "dashboard",
    "agent",
    "workflow",
    "test_fixture",
];

pub fn complexity_weights() -> HashMap<&'static str, f64> {
    let mut m = HashMap::with_capacity(4);
    m.insert("cognitive", 0.35);
    m.insert("context", 0.25);
    m.insert("execution_risk", 0.25);
    m.insert("ambiguity", 0.15);
    m
}

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Evidence {
    pub feature: String,
    pub text: String,
    pub span: [i64; 2],
    pub polarity: String,
    pub source: String,
    pub rule_id: Option<String>,
    pub confidence: f64,
    pub negation_scope: Option<String>,
}

impl Default for Evidence {
    fn default() -> Self {
        Self {
            feature: String::new(),
            text: String::new(),
            span: [0, 0],
            polarity: String::new(),
            source: String::new(),
            rule_id: None,
            confidence: 1.0,
            negation_scope: None,
        }
    }
}

impl Evidence {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

// ---------------------------------------------------------------------------
// ShadowRoute
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ShadowRoute {
    pub tier: String,
    pub profile_id: Option<String>,
    pub reason: String,
    pub admission_scope: String,
    pub estimated_cost: Option<f64>,
    pub expected_tradeoff: String,
}

impl Default for ShadowRoute {
    fn default() -> Self {
        Self {
            tier: String::new(),
            profile_id: None,
            reason: String::new(),
            admission_scope: "diagnostic".to_string(),
            estimated_cost: None,
            expected_tradeoff: String::new(),
        }
    }
}

impl ShadowRoute {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

// ---------------------------------------------------------------------------
// BudgetReservation
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetReservation {
    pub schema_version: String,
    pub reservation_id: String,
    pub decision_id: String,
    pub currency: String,
    pub pricing_snapshot_id: Option<String>,
    pub pre_budget: i64,
    pub reserved_input_tokens: i64,
    pub reserved_output_tokens: i64,
    pub reserved_total_tokens: i64,
    pub reserved_cost: f64,
    pub budget_policy_id: Option<String>,
    pub budget_gate: Option<String>,
    pub status: String,
    pub actual_usage_ref: Option<String>,
    pub budget_delta: Option<i64>,
    pub budget_violation: bool,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,
}

impl Default for BudgetReservation {
    fn default() -> Self {
        Self {
            schema_version: BUDGET_RESERVATION_SCHEMA_VERSION.to_string(),
            reservation_id: String::new(),
            decision_id: String::new(),
            currency: String::new(),
            pricing_snapshot_id: None,
            pre_budget: 0,
            reserved_input_tokens: 0,
            reserved_output_tokens: 0,
            reserved_total_tokens: 0,
            reserved_cost: 0.0,
            budget_policy_id: None,
            budget_gate: None,
            status: String::new(),
            actual_usage_ref: None,
            budget_delta: None,
            budget_violation: false,
            created_at: String::new(),
            updated_at: String::new(),
            expires_at: None,
        }
    }
}

impl BudgetReservation {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

// ---------------------------------------------------------------------------
// ExecutionGate
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExecutionGate {
    pub gate_id: String,
    pub gate_type: String,
    pub severity: String,
    pub reason: String,
    pub evidence_refs: Vec<String>,
    pub clearance_required: String,
    pub cleared: bool,
    pub cleared_by: Option<String>,
    pub cleared_at: Option<String>,
}

impl Default for ExecutionGate {
    fn default() -> Self {
        Self {
            gate_id: String::new(),
            gate_type: String::new(),
            severity: String::new(),
            reason: String::new(),
            evidence_refs: Vec::new(),
            clearance_required: "none".to_string(),
            cleared: false,
            cleared_by: None,
            cleared_at: None,
        }
    }
}

impl ExecutionGate {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

// ---------------------------------------------------------------------------
// RejectedCandidate
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RejectedCandidate {
    pub tier: String,
    pub profile_id: Option<String>,
    pub reason: String,
    pub constraint_failed: Option<String>,
    pub estimated_cost: Option<f64>,
}

impl RejectedCandidate {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

// ---------------------------------------------------------------------------
// DispatchDecision
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DispatchDecision {
    pub schema_version: String,
    pub decision_id: String,
    pub analysis_id: String,
    pub analysis_snapshot: serde_json::Value,
    pub selected_tier: String,
    pub selected_profile_id: Option<String>,
    pub fallback_tier: String,
    pub fallback_profile_id: Option<String>,
    pub shadow_routes: Vec<ShadowRoute>,
    pub hard_constraints: Vec<String>,
    pub rejected_candidates: Vec<RejectedCandidate>,
    pub no_shadow_route_reason: Option<String>,
    pub max_input_tokens: i64,
    pub max_output_tokens: i64,
    pub routing_reason: String,
    pub quality_requirement: String,
    pub expected_quality_band: String,
    pub confidence: f64,
    pub confidence_label: String,
    pub budget_reservation: BudgetReservation,
    pub execution_policy: serde_json::Value,
    pub execution_gates: Vec<ExecutionGate>,
    pub routing_mode: String,
    pub routing_experiment_id: Option<String>,
    pub decision_status: String,
    pub created_at: String,
}

impl Default for DispatchDecision {
    fn default() -> Self {
        Self {
            schema_version: DISPATCH_DECISION_SCHEMA_VERSION.to_string(),
            decision_id: String::new(),
            analysis_id: String::new(),
            analysis_snapshot: serde_json::Value::Object(serde_json::Map::new()),
            selected_tier: String::new(),
            selected_profile_id: None,
            fallback_tier: String::new(),
            fallback_profile_id: None,
            shadow_routes: Vec::new(),
            hard_constraints: Vec::new(),
            rejected_candidates: Vec::new(),
            no_shadow_route_reason: None,
            max_input_tokens: 4000,
            max_output_tokens: 3000,
            routing_reason: String::new(),
            quality_requirement: String::new(),
            expected_quality_band: String::new(),
            confidence: 0.0,
            confidence_label: String::new(),
            budget_reservation: BudgetReservation::default(),
            execution_policy: serde_json::Value::Object(serde_json::Map::new()),
            execution_gates: Vec::new(),
            routing_mode: "static".to_string(),
            routing_experiment_id: None,
            decision_status: String::new(),
            created_at: String::new(),
        }
    }
}

impl DispatchDecision {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

pub fn build_dispatch_bundle(raw_request: &str, request_source: &str) -> Value {
    let mut runtime = FixtureRuntime::new();
    let dispatch_id = runtime.id("disp-");
    let decision_id = runtime.id("dec-");
    let analysis = analyze(raw_request, request_source, &mut runtime);

    let (selected_tier, fallback_tier, shadow_routes, rejected_candidates, routing_reason) =
        select_model_tier(&analysis);
    let budget_reservation =
        create_budget_reservation(&decision_id, &analysis, &selected_tier, &mut runtime);
    let execution_policy = build_execution_policy(&analysis);
    let execution_gates = build_execution_gates(
        &analysis,
        &budget_reservation,
        &execution_policy,
        &mut runtime,
    );
    let hard_constraints = derive_hard_constraints(&analysis);
    let decision_status = determine_decision_status(&execution_gates);

    let decision = json!({
        "schema_version": DISPATCH_DECISION_SCHEMA_VERSION,
        "decision_id": decision_id,
        "analysis_id": analysis["analysis_id"].clone(),
        "analysis_snapshot": analysis.clone(),
        "selected_tier": selected_tier,
        "selected_profile_id": null,
        "fallback_tier": fallback_tier,
        "fallback_profile_id": null,
        "shadow_routes": shadow_routes,
        "hard_constraints": hard_constraints,
        "rejected_candidates": rejected_candidates,
        "no_shadow_route_reason": null,
        "max_input_tokens": analysis["context_budget_estimate"].clone(),
        "max_output_tokens": analysis["execution_budget_estimate"].clone(),
        "routing_reason": routing_reason,
        "quality_requirement": analysis["quality_requirement"].clone(),
        "expected_quality_band": quality_band(&selected_tier),
        "confidence": analysis["confidence"].clone(),
        "confidence_label": analysis["confidence_label"].clone(),
        "budget_reservation": budget_reservation,
        "execution_policy": execution_policy,
        "execution_gates": execution_gates,
        "routing_mode": "static",
        "routing_experiment_id": null,
        "decision_status": decision_status,
        "created_at": runtime.now()
    });

    let execution_result = json!({
        "schema_version": "execution_result.v1",
        "result_id": runtime.id("exec-"),
        "dispatch_id": dispatch_id,
        "decision_id": decision["decision_id"].clone(),
        "executor_type": "noop",
        "status": "not_executed",
        "output": null,
        "prompt_pack": null,
        "input_tokens": null,
        "output_tokens": null,
        "estimated_cost": null,
        "latency_ms": null,
        "error_domain": null,
        "error_message": null,
        "provider_request_id": null,
        "attempt_number": null,
        "finish_reason": null,
        "usage_source": null,
        "created_at": runtime.now()
    });
    let evaluation_result = evaluate(&execution_result, &decision, &mut runtime);
    let record = json!({
        "schema_version": "dispatch_record.v1",
        "dispatch_id": dispatch_id,
        "request_snapshot": raw_request,
        "task_analysis_id": analysis["analysis_id"].clone(),
        "decision_id": decision["decision_id"].clone(),
        "execution_result_id": execution_result["result_id"].clone(),
        "evaluation_result_id": evaluation_result["evaluation_id"].clone(),
        "usage_ledger_row_id": null,
        "budget_reservation_id": decision["budget_reservation"]["reservation_id"].clone(),
        "final_status": "not_executed",
        "created_at": runtime.now(),
        "updated_at": runtime.now()
    });

    json!({
        "record": record,
        "analysis": analysis,
        "decision": decision,
        "execution_result": execution_result,
        "evaluation_result": evaluation_result
    })
}

fn select_model_tier(analysis: &Value) -> (String, String, Vec<Value>, Vec<Value>, String) {
    let domain = analysis["task_domain"].as_str().unwrap_or("other");
    let intent = analysis["task_intent"].as_str().unwrap_or("classify");
    let risk_level = analysis["risk_level"].as_str().unwrap_or("low");
    let confidence_label = analysis["confidence_label"].as_str().unwrap_or("high");
    let context_budget = analysis["context_budget_estimate"].as_i64().unwrap_or(0);

    let mut selected = policy_tier(domain, intent).to_string();
    if ["critical", "high"].contains(&risk_level) {
        selected = match selected.as_str() {
            "cheap_executor" => "balanced_worker".to_string(),
            "balanced_worker" => "strong_planner".to_string(),
            _ => selected,
        };
    }

    let mut rejected = Vec::new();
    let mut reasons = vec![format!("policy_map:{domain}_{intent}")];

    if confidence_label == "low" {
        reasons.push("low_confidence_escalation".to_string());
        rejected.push(json!({
            "tier": selected,
            "profile_id": null,
            "reason": "low confidence",
            "constraint_failed": "confidence_threshold",
            "estimated_cost": null
        }));
        selected = "strong_planner".to_string();
    }

    if risk_level == "critical" && selected != "strong_planner" && selected != "advisor" {
        rejected.push(json!({
            "tier": selected,
            "profile_id": null,
            "reason": "critical risk requires stronger tier",
            "constraint_failed": "risk_level",
            "estimated_cost": null
        }));
        selected = "strong_planner".to_string();
        reasons.push("critical_risk_override".to_string());
    }

    if context_budget < 500 {
        rejected.push(json!({
            "tier": "strong_planner",
            "profile_id": null,
            "reason": "budget too low for strong_planner",
            "constraint_failed": "budget_threshold",
            "estimated_cost": null
        }));
        reasons.push("budget_constrained".to_string());
    }

    let fallback = fallback_tier(&selected).to_string();
    let shadow_routes = build_shadow_routes(&selected, &fallback);
    (
        selected,
        fallback,
        shadow_routes,
        rejected,
        reasons.join("; "),
    )
}

fn policy_tier(domain: &str, intent: &str) -> &'static str {
    match (domain, intent) {
        ("code", "generate") => "balanced_worker",
        ("code", "review") => "balanced_worker",
        ("code", "debug") => "strong_planner",
        ("code", "refactor") => "balanced_worker",
        ("docs", "summarize") => "cheap_executor",
        ("docs", "generate") => "cheap_executor",
        ("docs", "review") => "cheap_executor",
        ("docs", "explain") => "cheap_executor",
        ("config", "review") => "cheap_executor",
        ("config", "generate") => "balanced_worker",
        ("infra", "review") => "balanced_worker",
        ("infra", "plan") => "strong_planner",
        ("math", "generate") => "strong_planner",
        ("math", "explain") => "balanced_worker",
        ("architecture", "plan") => "strong_planner",
        ("architecture", "design") => "strong_planner",
        ("repo_ops", "review") => "cheap_executor",
        ("repo_ops", "generate") => "balanced_worker",
        ("governance", "audit") => "verifier",
        ("governance", "review") => "verifier",
        ("other", "classify") => "cheap_executor",
        _ => "balanced_worker",
    }
}

fn fallback_tier(selected: &str) -> &'static str {
    let index = MODEL_TIERS
        .iter()
        .position(|tier| *tier == selected)
        .unwrap_or(1);
    if index < MODEL_TIERS.len() - 1 {
        MODEL_TIERS[index + 1]
    } else {
        MODEL_TIERS[MODEL_TIERS.len() - 1]
    }
}

fn build_shadow_routes(selected: &str, fallback: &str) -> Vec<Value> {
    let mut routes = Vec::new();
    if fallback != selected {
        routes.push(json!({
            "tier": fallback,
            "profile_id": null,
            "reason": "fallback option",
            "admission_scope": "diagnostic",
            "estimated_cost": null,
            "expected_tradeoff": "lower cost, potentially lower quality"
        }));
    }
    if selected != "cheap_executor" {
        routes.push(json!({
            "tier": "cheap_executor",
            "profile_id": null,
            "reason": "cost-optimized alternative",
            "admission_scope": "diagnostic",
            "estimated_cost": null,
            "expected_tradeoff": "lowest cost, adequate for simple tasks"
        }));
    }
    if routes.is_empty() {
        routes.push(json!({
            "tier": selected,
            "profile_id": null,
            "reason": "self-diagnostic (no cheaper alternative)",
            "admission_scope": "diagnostic",
            "estimated_cost": null,
            "expected_tradeoff": "same tier, diagnostic comparison"
        }));
    }
    routes
}

fn create_budget_reservation(
    decision_id: &str,
    analysis: &Value,
    tier: &str,
    runtime: &mut FixtureRuntime,
) -> Value {
    let input_tokens = analysis["context_budget_estimate"].as_i64().unwrap_or(0);
    let output_tokens = analysis["execution_budget_estimate"].as_i64().unwrap_or(0);
    let total_tokens = input_tokens + output_tokens;
    json!({
        "schema_version": BUDGET_RESERVATION_SCHEMA_VERSION,
        "reservation_id": runtime.id("res-"),
        "decision_id": decision_id,
        "currency": "token",
        "pricing_snapshot_id": null,
        "pre_budget": total_tokens,
        "reserved_input_tokens": input_tokens,
        "reserved_output_tokens": output_tokens,
        "reserved_total_tokens": total_tokens,
        "reserved_cost": round6(estimate_cost(tier, input_tokens, output_tokens)),
        "budget_policy_id": null,
        "budget_gate": null,
        "status": "reserved",
        "actual_usage_ref": null,
        "budget_delta": null,
        "budget_violation": false,
        "created_at": runtime.now(),
        "updated_at": runtime.now(),
        "expires_at": null
    })
}

fn estimate_cost(tier: &str, input_tokens: i64, output_tokens: i64) -> f64 {
    let input_rate = match tier {
        "cheap_executor" => 0.0005,
        "balanced_worker" => 0.003,
        "strong_planner" => 0.015,
        "verifier" => 0.003,
        "advisor" => 0.015,
        _ => 0.003,
    };
    let output_rate = match tier {
        "cheap_executor" => 0.0015,
        "balanced_worker" => 0.015,
        "strong_planner" => 0.075,
        "verifier" => 0.015,
        "advisor" => 0.075,
        _ => 0.015,
    };
    (input_tokens as f64 / 1000.0 * input_rate) + (output_tokens as f64 / 1000.0 * output_rate)
}

fn build_execution_policy(analysis: &Value) -> Value {
    let risk_level = analysis["risk_level"].as_str().unwrap_or("low");
    let confidence_label = analysis["confidence_label"].as_str().unwrap_or("high");
    let requires_human_review =
        ["critical", "high"].contains(&risk_level) || confidence_label == "low";
    json!({
        "executor_type": "noop",
        "execution_allowed": true,
        "requires_human_review": requires_human_review,
        "max_retries": 0
    })
}

fn build_execution_gates(
    analysis: &Value,
    reservation: &Value,
    execution_policy: &Value,
    runtime: &mut FixtureRuntime,
) -> Vec<Value> {
    let mut gates = Vec::new();
    gates.push(json!({
        "gate_id": runtime.id("gate-"),
        "gate_type": "provider_disabled",
        "severity": "info",
        "reason": "real provider calls disabled \u{2014} non-provider executor",
        "evidence_refs": [],
        "clearance_required": "policy",
        "cleared": false,
        "cleared_by": null,
        "cleared_at": null
    }));
    gates.push(json!({
        "gate_id": runtime.id("gate-"),
        "gate_type": "sandbox_disabled",
        "severity": "info",
        "reason": "sandbox execution disabled in Phase 1",
        "evidence_refs": [],
        "clearance_required": "policy",
        "cleared": false,
        "cleared_by": null,
        "cleared_at": null
    }));

    let risk_level = analysis["risk_level"].as_str().unwrap_or("low");
    let risk_flags = string_array(&analysis["risk_flags"]);
    if ["critical", "high"].contains(&risk_level) {
        gates.push(json!({
            "gate_id": runtime.id("gate-"),
            "gate_type": "risk",
            "severity": "block",
            "reason": format!("risk_level={risk_level}"),
            "evidence_refs": [],
            "clearance_required": "human",
            "cleared": false,
            "cleared_by": null,
            "cleared_at": null
        }));
    }
    if risk_flags.iter().any(|flag| flag == "target_write") {
        gates.push(json!({
            "gate_id": runtime.id("gate-"),
            "gate_type": "target_write",
            "severity": "block",
            "reason": "target_write risk flag detected",
            "evidence_refs": [],
            "clearance_required": "human",
            "cleared": false,
            "cleared_by": null,
            "cleared_at": null
        }));
    }
    if analysis["confidence_label"] == "low" {
        let confidence = analysis["confidence"].as_f64().unwrap_or(0.0);
        gates.push(json!({
            "gate_id": runtime.id("gate-"),
            "gate_type": "confidence",
            "severity": "warning",
            "reason": format!("confidence={confidence:.2} below threshold"),
            "evidence_refs": [],
            "clearance_required": "none",
            "cleared": false,
            "cleared_by": null,
            "cleared_at": null
        }));
    }
    if reservation["budget_violation"] == true {
        gates.push(json!({
            "gate_id": runtime.id("gate-"),
            "gate_type": "budget",
            "severity": "block",
            "reason": "budget reservation violated",
            "evidence_refs": [],
            "clearance_required": "human",
            "cleared": false,
            "cleared_by": null,
            "cleared_at": null
        }));
    }
    if risk_flags
        .iter()
        .any(|flag| flag == "provider_call" || flag == "sandbox_execution")
    {
        gates.push(json!({
            "gate_id": runtime.id("gate-"),
            "gate_type": "boundary",
            "severity": "block",
            "reason": "boundary violation detected (provider/sandbox)",
            "evidence_refs": [],
            "clearance_required": "human",
            "cleared": false,
            "cleared_by": null,
            "cleared_at": null
        }));
    }
    if execution_policy["requires_human_review"] == true {
        gates.push(json!({
            "gate_id": runtime.id("gate-"),
            "gate_type": "manual_review",
            "severity": "block",
            "reason": "high risk or low confidence requires human review",
            "evidence_refs": [],
            "clearance_required": "human",
            "cleared": false,
            "cleared_by": null,
            "cleared_at": null
        }));
    }
    gates
}

fn derive_hard_constraints(analysis: &Value) -> Vec<&'static str> {
    let mut constraints = vec!["no_target_write", "no_provider_call"];
    if analysis["risk_level"] == "critical" {
        constraints.push("requires_human_approval");
    }
    constraints
}

fn determine_decision_status(gates: &[Value]) -> &'static str {
    if gates
        .iter()
        .any(|gate| gate["severity"] == "block" || gate["severity"] == "critical")
    {
        "needs_approval"
    } else {
        "decided"
    }
}

fn quality_band(tier: &str) -> &'static str {
    match tier {
        "cheap_executor" => "low",
        "balanced_worker" => "medium",
        "strong_planner" | "verifier" | "advisor" => "high",
        _ => "unknown",
    }
}

fn evaluate(result: &Value, decision: &Value, runtime: &mut FixtureRuntime) -> Value {
    let checks = vec![
        json!({
            "check_id": runtime.id("chk-"),
            "name": "schema_validity",
            "status": "pass",
            "reason": "required fields present"
        }),
        json!({
            "check_id": runtime.id("chk-"),
            "name": "boundary_compliance",
            "status": "pass",
            "reason": "executor_type=noop within boundaries"
        }),
        json!({
            "check_id": runtime.id("chk-"),
            "name": "output_present",
            "status": "warning",
            "reason": "noop executor produces no output (expected)"
        }),
        json!({
            "check_id": runtime.id("chk-"),
            "name": "error_free",
            "status": "pass",
            "reason": "no errors"
        }),
        json!({
            "check_id": runtime.id("chk-"),
            "name": "human_review_required",
            "status": if decision["execution_policy"]["requires_human_review"] == true { "warning" } else { "pass" },
            "reason": if decision["execution_policy"]["requires_human_review"] == true { "execution policy requires human review" } else { "no human review required" }
        }),
    ];
    let status = if decision["execution_policy"]["requires_human_review"] == true {
        "needs_human_review"
    } else {
        "pass"
    };
    json!({
        "schema_version": "evaluation_result.v1",
        "evaluation_id": runtime.id("eval-"),
        "dispatch_id": result["dispatch_id"].clone(),
        "decision_id": result["decision_id"].clone(),
        "execution_result_id": result["result_id"].clone(),
        "status": status,
        "checks": checks,
        "quality_score": null,
        "requires_retry": false,
        "retry_reason": null,
        "created_at": runtime.now()
    })
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn round6(value: f64) -> f64 {
    let scaled = value * 1_000_000.0;
    let floor = scaled.floor();
    if scaled - floor == 0.5 {
        floor / 1_000_000.0
    } else {
        scaled.round() / 1_000_000.0
    }
}
