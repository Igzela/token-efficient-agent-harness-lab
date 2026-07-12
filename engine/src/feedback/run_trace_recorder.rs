use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const RUN_TRACE_SCHEMA_VERSION: &str = "feedback_trace.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunTrace {
    pub schema_version: String,
    pub trace_id: String,
    pub dispatch_id: String,
    pub history_id: Option<i64>,
    pub created_at: Option<String>,
    pub task_class: String,
    pub task_domain: Option<String>,
    pub task_intent: Option<String>,
    pub selected_tier: String,
    pub selected_profile: Option<String>,
    pub routing_policy: Option<String>,
    pub complexity_score: Option<f64>,
    pub constraints: Vec<String>,
    pub human_review_flag: bool,
    pub retry_policy: Option<String>,
    pub shadow_routes: Vec<Value>,
    pub executor_type: String,
    pub execution_status: Option<String>,
    pub latency_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub estimated_cost_usd: Option<f64>,
    pub reserved_cost: f64,
    pub total_cost: f64,
    pub retry_count: i64,
    pub evaluation_status: String,
    pub final_status: String,
    pub success: bool,
    pub failure_domain: Option<String>,
    pub analysis: Value,
    pub decision: Value,
    pub execution: Value,
    pub evaluation: Value,
}

pub struct RunTraceRecorder;

impl RunTraceRecorder {
    pub fn record_from_dispatch(dispatch: &Value) -> RunTrace {
        let bundle = dispatch.get("bundle").unwrap_or(&Value::Null).clone();
        let dispatch_id = str_field(dispatch, "dispatch_id").unwrap_or("unknown");
        let history_id = dispatch.get("history_id").and_then(Value::as_i64);
        let created_at = str_field(dispatch, "created_at").map(str::to_owned);

        Self::record_from_bundle(&bundle, dispatch_id, history_id, created_at)
    }

    pub fn record_from_bundle(
        bundle: &Value,
        dispatch_id: &str,
        history_id: Option<i64>,
        created_at: Option<String>,
    ) -> RunTrace {
        let analysis = extract_section(bundle, &["analysis"])
            .or_else(|| extract_section(bundle, &["decision", "analysis_snapshot"]))
            .unwrap_or(Value::Null);
        let decision = extract_section(bundle, &["decision"]).unwrap_or(Value::Null);
        let execution = extract_section(bundle, &["execution_result"]).unwrap_or(Value::Null);
        let evaluation = extract_section(bundle, &["evaluation_result"]).unwrap_or(Value::Null);

        let final_status = final_status_from_bundle(bundle);
        let evaluation_status = evaluation_status_from_bundle(bundle);
        let success =
            overall_dispatch_success_from_bundle(bundle, &final_status, &evaluation_status);

        RunTrace {
            schema_version: RUN_TRACE_SCHEMA_VERSION.to_string(),
            trace_id: format!("trace-{dispatch_id}"),
            dispatch_id: dispatch_id.to_string(),
            history_id,
            created_at,
            task_class: task_class(bundle),
            task_domain: task_domain(bundle),
            task_intent: task_intent(bundle),
            selected_tier: selected_tier_from_bundle(bundle),
            selected_profile: selected_profile(bundle),
            routing_policy: routing_policy(bundle),
            complexity_score: complexity_score_val(bundle),
            constraints: constraints(bundle),
            human_review_flag: human_review_flag(bundle),
            retry_policy: retry_policy(bundle),
            shadow_routes: shadow_routes(bundle),
            executor_type: executor_type_from_bundle(bundle),
            execution_status: execution_status(bundle),
            latency_ms: latency_ms(bundle),
            input_tokens: input_tokens_val(bundle),
            output_tokens: output_tokens_val(bundle),
            estimated_cost_usd: estimated_cost(bundle),
            reserved_cost: reserved_cost(bundle),
            total_cost: total_cost(bundle),
            retry_count: retry_count(bundle),
            evaluation_status,
            final_status,
            success,
            failure_domain: failure_domain_val(bundle, success),
            analysis,
            decision,
            execution,
            evaluation,
        }
    }

    /// Convert a `RunTrace` into the feedback-trace JSON shape expected by
    /// `GET /api/v1/feedback/traces`, preserving backward compatibility with
    /// the existing endpoint, dashboard, and SDK response schema.
    pub fn to_feedback_trace_json(trace: &RunTrace, attribution_value: Value) -> Value {
        let status = if trace.success { "pass" } else { "fail" };
        json!({
            "schema_version": &trace.schema_version,
            "trace_id": &trace.trace_id,
            "history_id": trace.history_id,
            "dispatch_id": &trace.dispatch_id,
            "created_at": trace.created_at,
            "task_class": &trace.task_class,
            "tier": &trace.selected_tier,
            "status": status,
            "executor_type": &trace.executor_type,
            "success": trace.success,
            "execution_status": &trace.execution_status,
            "execution_terminal": trace.execution_status.as_deref().is_some_and(is_terminal_status),
            "execution_succeeded": execution_succeeded(trace),
            "evaluation_status": &trace.evaluation_status,
            "evaluation_completed": evaluation_completed(Some(&trace.evaluation_status)),
            "evaluation_passed": evaluation_passed(trace),
            "overall_success": overall_dispatch_success(trace),
            "latency_ms": trace.latency_ms,
            "cost_usd": if let Some(cost) = trace.estimated_cost_usd {
                json!(cost)
            } else {
                json!(trace.reserved_cost)
            },
            "error_domain": trace.failure_domain.as_ref().map(|d| json!(d)).unwrap_or(Value::Null),
            "analysis": &trace.analysis,
            "decision": &trace.decision,
            "execution": &trace.execution,
            "evaluation": &trace.evaluation,
            "attribution": attribution_value,
        })
    }
}

// ---------------------------------------------------------------------------
// Path traversal helpers
// ---------------------------------------------------------------------------

fn str_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    current.as_str()
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    Some(current)
}

fn first_str<'a>(value: &'a Value, paths: &[&[&str]]) -> Option<&'a str> {
    paths.iter().find_map(|path| str_at(value, path))
}

fn first_f64(value: &Value, paths: &[&[&str]]) -> Option<f64> {
    paths
        .iter()
        .find_map(|path| value_at(value, path).and_then(Value::as_f64))
}

#[allow(dead_code)]
fn first_i64(value: &Value, paths: &[&[&str]]) -> Option<i64> {
    paths
        .iter()
        .find_map(|path| value_at(value, path).and_then(Value::as_i64))
}

fn first_bool(value: &Value, paths: &[&[&str]]) -> Option<bool> {
    paths
        .iter()
        .find_map(|path| value_at(value, path).and_then(Value::as_bool))
}

fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn extract_section(value: &Value, path: &[&str]) -> Option<Value> {
    value_at(value, path).cloned()
}

fn first_array_of_strings(value: &Value, paths: &[&[&str]]) -> Vec<String> {
    for path in paths {
        if let Some(arr) = value_at(value, path).and_then(Value::as_array) {
            return arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
        }
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
// Field extractors (replicated from dispatch.rs helpers)
// ---------------------------------------------------------------------------

fn task_class(bundle: &Value) -> String {
    first_str(
        bundle,
        &[
            &["analysis", "task_class"],
            &["analysis", "task_domain"],
            &["decision", "analysis_snapshot", "task_class"],
            &["decision", "analysis_snapshot", "task_domain"],
        ],
    )
    .unwrap_or("unknown")
    .to_string()
}

fn task_domain(bundle: &Value) -> Option<String> {
    first_str(
        bundle,
        &[
            &["analysis", "task_domain"],
            &["decision", "analysis_snapshot", "task_domain"],
        ],
    )
    .map(str::to_owned)
}

fn task_intent(bundle: &Value) -> Option<String> {
    first_str(
        bundle,
        &[
            &["analysis", "task_intent"],
            &["analysis", "intent"],
            &["decision", "analysis_snapshot", "task_intent"],
            &["decision", "analysis_snapshot", "intent"],
        ],
    )
    .map(str::to_owned)
}

fn selected_tier_from_bundle(bundle: &Value) -> String {
    str_at(bundle, &["decision", "selected_tier"])
        .unwrap_or("unknown")
        .to_string()
}

fn selected_profile(bundle: &Value) -> Option<String> {
    first_str(
        bundle,
        &[
            &["decision", "selected_profile"],
            &["decision", "agent_profile"],
            &["analysis", "selected_profile"],
        ],
    )
    .map(str::to_owned)
}

fn routing_policy(bundle: &Value) -> Option<String> {
    first_str(
        bundle,
        &[
            &["decision", "routing_policy"],
            &["analysis", "routing_policy"],
        ],
    )
    .map(str::to_owned)
}

fn complexity_score_val(bundle: &Value) -> Option<f64> {
    first_f64(
        bundle,
        &[
            &["analysis", "complexity_score"],
            &["decision", "analysis_snapshot", "complexity_score"],
        ],
    )
}

fn constraints(bundle: &Value) -> Vec<String> {
    first_array_of_strings(
        bundle,
        &[
            &["analysis", "constraints"],
            &["decision", "analysis_snapshot", "constraints"],
        ],
    )
}

fn human_review_flag(bundle: &Value) -> bool {
    first_bool(
        bundle,
        &[
            &["analysis", "human_review_flag"],
            &["decision", "analysis_snapshot", "human_review_flag"],
            &["decision", "requires_human_review"],
        ],
    )
    .unwrap_or(false)
}

fn retry_policy(bundle: &Value) -> Option<String> {
    first_str(
        bundle,
        &[&["decision", "retry_policy"], &["analysis", "retry_policy"]],
    )
    .map(str::to_owned)
}

fn shadow_routes(bundle: &Value) -> Vec<Value> {
    bundle
        .pointer("/decision/shadow_routes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn executor_type_from_bundle(bundle: &Value) -> String {
    str_at(bundle, &["execution_result", "executor_type"])
        .unwrap_or("unknown")
        .to_string()
}

fn execution_status(bundle: &Value) -> Option<String> {
    str_at(bundle, &["execution_result", "status"]).map(str::to_owned)
}

fn latency_ms(bundle: &Value) -> Option<i64> {
    bundle
        .pointer("/execution_result/latency_ms")
        .and_then(Value::as_i64)
}

fn input_tokens_val(bundle: &Value) -> Option<i64> {
    bundle
        .pointer("/execution_result/input_tokens")
        .and_then(Value::as_i64)
}

fn output_tokens_val(bundle: &Value) -> Option<i64> {
    bundle
        .pointer("/execution_result/output_tokens")
        .and_then(Value::as_i64)
}

fn estimated_cost(bundle: &Value) -> Option<f64> {
    bundle
        .pointer("/execution_result/estimated_cost")
        .and_then(Value::as_f64)
}

fn reserved_cost(bundle: &Value) -> f64 {
    bundle
        .pointer("/decision/budget_reservation/reserved_cost")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

fn total_cost(bundle: &Value) -> f64 {
    bundle
        .pointer("/execution_result/estimated_cost")
        .and_then(Value::as_f64)
        .or_else(|| {
            bundle
                .pointer("/decision/budget_reservation/reserved_cost")
                .and_then(Value::as_f64)
        })
        .unwrap_or(0.0)
}

fn retry_count(bundle: &Value) -> i64 {
    bundle
        .pointer("/execution_result/retry_count")
        .and_then(Value::as_i64)
        .or_else(|| {
            bundle
                .pointer("/decision/retry_count")
                .and_then(Value::as_i64)
        })
        .unwrap_or(0)
}

fn final_status_from_bundle(bundle: &Value) -> String {
    str_at(bundle, &["record", "final_status"])
        .unwrap_or("unknown")
        .to_string()
}

fn evaluation_status_from_bundle(bundle: &Value) -> String {
    str_at(bundle, &["evaluation_result", "status"])
        .unwrap_or("unknown")
        .to_string()
}

pub fn overall_dispatch_success_from_bundle(
    bundle: &Value,
    final_status: &str,
    evaluation_status: &str,
) -> bool {
    let execution_success = bundle
        .pointer("/execution_result/success")
        .and_then(Value::as_bool)
        .or_else(|| {
            execution_outcome(
                bundle
                    .pointer("/execution_result/status")
                    .and_then(Value::as_str),
            )
        })
        .or_else(|| execution_outcome(Some(final_status)));
    let evaluation_success = evaluation_outcome(Some(evaluation_status));
    execution_success.unwrap_or(false) && evaluation_success.unwrap_or(false)
}

pub fn is_terminal_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "completed"
            | "success"
            | "succeeded"
            | "passed"
            | "failed"
            | "fail"
            | "error"
            | "cancelled"
            | "timeout"
            | "timed_out"
            | "not_executed"
    )
}

pub fn execution_outcome(status: Option<&str>) -> Option<bool> {
    match status?.trim().to_ascii_lowercase().as_str() {
        "completed" | "success" | "succeeded" | "passed" => Some(true),
        "failed" | "fail" | "error" | "cancelled" | "timeout" | "timed_out" | "not_executed" => {
            Some(false)
        }
        _ => None,
    }
}

pub fn evaluation_outcome(status: Option<&str>) -> Option<bool> {
    match status?.trim().to_ascii_lowercase().as_str() {
        "pass" | "passed" | "success" | "succeeded" => Some(true),
        "fail" | "failed" | "error" | "cancelled" | "timeout" | "timed_out" | "not_executed" => {
            Some(false)
        }
        _ => None,
    }
}

pub fn evaluation_completed(status: Option<&str>) -> bool {
    matches!(
        status
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "completed" | "pass" | "passed" | "success" | "succeeded" | "fail" | "failed" | "error"
    )
}

pub fn execution_succeeded(trace: &RunTrace) -> Option<bool> {
    execution_outcome(trace.execution_status.as_deref())
        .or_else(|| execution_outcome(Some(&trace.final_status)))
}

pub fn evaluation_passed(trace: &RunTrace) -> Option<bool> {
    evaluation_outcome(Some(&trace.evaluation_status))
}

pub fn overall_dispatch_success(trace: &RunTrace) -> bool {
    execution_succeeded(trace).unwrap_or(false) && evaluation_passed(trace).unwrap_or(false)
}

fn failure_domain_val(bundle: &Value, success: bool) -> Option<String> {
    if success {
        return None;
    }
    first_str(
        bundle,
        &[
            &["execution_result", "error_domain"],
            &["evaluation_result", "failure_domain"],
            &["evaluation_result", "error_domain"],
        ],
    )
    .map(str::to_owned)
    .or_else(|| Some("unknown".to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_bundle() -> Value {
        json!({
            "analysis": { "task_class": "implementation" },
            "decision": {
                "selected_tier": "t2-medium",
                "budget_reservation": { "reserved_cost": 0.05 }
            },
            "execution_result": {
                "executor_type": "cli",
                "status": "completed",
                "success": true,
                "latency_ms": 1200,
                "input_tokens": 500,
                "output_tokens": 300,
                "estimated_cost": 0.03
            },
            "evaluation_result": {
                "status": "pass"
            },
            "record": {
                "final_status": "completed"
            }
        })
    }

    fn minimal_dispatch() -> Value {
        json!({
            "dispatch_id": "d-abc123",
            "history_id": 42,
            "created_at": "2026-06-11T10:00:00Z",
            "selected_tier": "t2-medium",
            "bundle": minimal_bundle()
        })
    }

    #[test]
    fn test_record_from_dispatch_produces_stable_schema() {
        let dispatch = minimal_dispatch();
        let trace = RunTraceRecorder::record_from_dispatch(&dispatch);

        assert_eq!(trace.schema_version, RUN_TRACE_SCHEMA_VERSION);
        assert_eq!(trace.schema_version, "feedback_trace.v1");
        assert_eq!(trace.trace_id, "trace-d-abc123");
        assert_eq!(trace.dispatch_id, "d-abc123");
        assert_eq!(trace.history_id, Some(42));
        assert_eq!(trace.created_at.as_deref(), Some("2026-06-11T10:00:00Z"));
        assert_eq!(trace.task_class, "implementation");
        assert_eq!(trace.selected_tier, "t2-medium");
        assert_eq!(trace.executor_type, "cli");
        assert_eq!(trace.latency_ms, Some(1200));
        assert_eq!(trace.input_tokens, Some(500));
        assert_eq!(trace.output_tokens, Some(300));
        assert_eq!(trace.estimated_cost_usd, Some(0.03));
        assert_eq!(trace.reserved_cost, 0.05);
        assert_eq!(trace.evaluation_status, "pass");
        assert_eq!(trace.final_status, "completed");
        assert!(trace.success);
    }

    #[test]
    fn test_record_from_dispatch_pass_status() {
        let dispatch = json!({
            "dispatch_id": "d-pass",
            "bundle": {
                "record": { "final_status": "completed" },
                "evaluation_result": { "status": "pass" },
                "execution_result": { "executor_type": "cli" }
            }
        });
        let trace = RunTraceRecorder::record_from_dispatch(&dispatch);
        assert!(trace.success);
        assert_eq!(trace.final_status, "completed");
    }

    #[test]
    fn test_record_from_dispatch_fail_status() {
        let dispatch = json!({
            "dispatch_id": "d-fail",
            "bundle": {
                "record": { "final_status": "failed" },
                "evaluation_result": { "status": "fail" },
                "execution_result": { "executor_type": "cli" }
            }
        });
        let trace = RunTraceRecorder::record_from_dispatch(&dispatch);
        assert!(!trace.success);
        assert_eq!(trace.final_status, "failed");
        assert_eq!(trace.failure_domain.as_deref(), Some("unknown"));
    }

    #[test]
    fn test_execution_and_evaluation_outcomes_remain_distinct() {
        let completed_but_failed_evaluation = json!({
            "dispatch_id": "d-quality-fail",
            "bundle": {
                "record": { "final_status": "completed" },
                "execution_result": {
                    "status": "completed",
                    "success": true,
                    "executor_type": "cli"
                },
                "evaluation_result": { "status": "failed" }
            }
        });
        let trace = RunTraceRecorder::record_from_dispatch(&completed_but_failed_evaluation);
        assert_eq!(execution_succeeded(&trace), Some(true));
        assert_eq!(evaluation_passed(&trace), Some(false));
        assert!(!overall_dispatch_success(&trace));
        assert!(!trace.success);

        let failed_execution_with_evidence = json!({
            "dispatch_id": "d-timeout",
            "bundle": {
                "record": { "final_status": "timed_out" },
                "execution_result": {
                    "status": "timeout",
                    "success": false,
                    "executor_type": "cli"
                },
                "evaluation_result": { "status": "passed" }
            }
        });
        let trace = RunTraceRecorder::record_from_dispatch(&failed_execution_with_evidence);
        assert_eq!(execution_succeeded(&trace), Some(false));
        assert_eq!(evaluation_passed(&trace), Some(true));
        assert!(!overall_dispatch_success(&trace));

        let completed_evaluation_without_pass = json!({
            "dispatch_id": "d-evaluation-complete",
            "bundle": {
                "record": { "final_status": "completed" },
                "execution_result": {
                    "status": "completed",
                    "success": false,
                    "executor_type": "cli"
                },
                "evaluation_result": { "status": "completed" }
            }
        });
        let trace = RunTraceRecorder::record_from_dispatch(&completed_evaluation_without_pass);
        assert!(evaluation_completed(Some(&trace.evaluation_status)));
        assert_eq!(evaluation_passed(&trace), None);
        assert!(!overall_dispatch_success(&trace));
    }

    #[test]
    fn test_record_from_dispatch_includes_decision_and_evaluation_sections() {
        let dispatch = json!({
            "dispatch_id": "d-dec",
            "bundle": {
                "decision": {
                    "selected_tier": "t1-small",
                    "budget_reservation": { "reserved_cost": 0.01 }
                },
                "evaluation_result": { "status": "pass" },
                "record": { "final_status": "completed" }
            }
        });
        let trace = RunTraceRecorder::record_from_dispatch(&dispatch);
        assert!(trace.decision.is_object());
        assert!(trace.decision.get("selected_tier").is_some());
        assert!(trace.evaluation.is_object());
        assert_eq!(
            trace.evaluation.get("status").and_then(Value::as_str),
            Some("pass")
        );
    }

    #[test]
    fn test_record_from_dispatch_empty_bundle() {
        let dispatch = json!({
            "dispatch_id": "d-empty",
            "bundle": {}
        });
        let trace = RunTraceRecorder::record_from_dispatch(&dispatch);
        assert_eq!(trace.schema_version, RUN_TRACE_SCHEMA_VERSION);
        assert_eq!(trace.trace_id, "trace-d-empty");
        assert_eq!(trace.task_class, "unknown");
        assert_eq!(trace.selected_tier, "unknown");
        assert_eq!(trace.executor_type, "unknown");
        assert!(!trace.success);
        assert_eq!(trace.final_status, "unknown");
        assert_eq!(trace.evaluation_status, "unknown");
        assert_eq!(trace.reserved_cost, 0.0);
        assert!(trace.constraints.is_empty());
        assert!(!trace.human_review_flag);
        assert!(trace.shadow_routes.is_empty());
    }

    #[test]
    fn test_record_from_dispatch_no_bundle_key() {
        let dispatch = json!({ "dispatch_id": "d-nobundle" });
        let trace = RunTraceRecorder::record_from_dispatch(&dispatch);
        assert_eq!(trace.dispatch_id, "d-nobundle");
        assert_eq!(trace.trace_id, "trace-d-nobundle");
        assert!(!trace.success);
    }

    #[test]
    fn test_record_from_bundle_direct() {
        let bundle = minimal_bundle();
        let trace = RunTraceRecorder::record_from_bundle(&bundle, "d-direct", Some(99), None);
        assert_eq!(trace.dispatch_id, "d-direct");
        assert_eq!(trace.history_id, Some(99));
        assert!(trace.created_at.is_none());
        assert!(trace.success);
    }

    #[test]
    fn test_determinism() {
        let dispatch = minimal_dispatch();
        let t1 = RunTraceRecorder::record_from_dispatch(&dispatch);
        let t2 = RunTraceRecorder::record_from_dispatch(&dispatch);
        let j1 = serde_json::to_string(&t1).unwrap();
        let j2 = serde_json::to_string(&t2).unwrap();
        assert_eq!(j1, j2);
    }
}
