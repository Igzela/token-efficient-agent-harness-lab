use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::feedback::run_trace_recorder::RunTrace;

pub const OUTCOME_ATTRIBUTION_SCHEMA_VERSION: &str = "outcome_attribution.v1";

const COST_HIGH_THRESHOLD: f64 = 0.10;
const LATENCY_HIGH_THRESHOLD_MS: f64 = 30000.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Outcome {
    Passed,
    Failed,
    Inconclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeAttribution {
    pub schema_version: String,
    pub outcome: Outcome,
    pub tier_outcome: String,
    pub constraint_outcome: String,
    pub retry_outcome: String,
    pub evaluation_outcome: String,
    pub cost_outcome: String,
    pub latency_outcome: String,
    pub human_review_outcome: Option<String>,
    pub likely_failure_factors: Vec<String>,
    pub likely_success_factors: Vec<String>,
    pub confidence: f64,
    pub reason_labels: Vec<String>,
}

pub struct OutcomeAttributor;

impl OutcomeAttributor {
    pub fn attribute(trace: &RunTrace) -> OutcomeAttribution {
        let outcome = if trace.success {
            Outcome::Passed
        } else if trace.final_status == "unknown" && trace.evaluation_status == "unknown" {
            Outcome::Inconclusive
        } else {
            Outcome::Failed
        };

        let tier_outcome = Self::tier_outcome(&trace.selected_tier);
        let constraint_outcome =
            Self::constraint_outcome(!trace.constraints.is_empty(), trace.success);
        let retry_outcome = Self::retry_outcome(trace.retry_policy.is_some());
        let evaluation_outcome = trace.evaluation_status.clone();
        let cost_outcome = Self::cost_outcome(trace.estimated_cost_usd);
        let latency_outcome = Self::latency_outcome(trace.latency_ms.map(|v| v as f64));
        let human_review_outcome = Self::human_review_outcome(trace.human_review_flag);

        let (failure_factors, success_factors) = Self::determine_factors(
            trace.success,
            &trace.selected_tier,
            &trace.task_class,
            trace.complexity_score,
            !trace.constraints.is_empty(),
            &evaluation_outcome,
            &cost_outcome,
            &latency_outcome,
        );

        let signal_count = Self::count_available_signals(
            &trace.final_status,
            &trace.evaluation_status,
            &trace.selected_tier,
            trace.estimated_cost_usd,
            trace.latency_ms.map(|v| v as f64),
        );
        let confidence = Self::compute_confidence(signal_count);

        let reason_labels = Self::build_reason_labels(
            trace.success,
            &trace.final_status,
            &trace.evaluation_status,
            &trace.selected_tier,
            &cost_outcome,
            &latency_outcome,
            trace.human_review_flag,
        );

        OutcomeAttribution {
            schema_version: OUTCOME_ATTRIBUTION_SCHEMA_VERSION.to_string(),
            outcome,
            tier_outcome,
            constraint_outcome,
            retry_outcome,
            evaluation_outcome,
            cost_outcome,
            latency_outcome,
            human_review_outcome,
            likely_failure_factors: failure_factors,
            likely_success_factors: success_factors,
            confidence,
            reason_labels,
        }
    }

    pub fn attribute_from_bundle(
        bundle: &Value,
        final_status: &str,
        evaluation_status: &str,
    ) -> OutcomeAttribution {
        let success = Self::resolve_success(bundle, final_status, evaluation_status);
        let outcome = Self::resolve_outcome(success, final_status, evaluation_status);
        let tier = Self::extract_tier(bundle);
        let task_class = Self::extract_task_class(bundle);
        let complexity_score = Self::extract_complexity(bundle);
        let estimated_cost = Self::extract_cost(bundle);
        let latency_ms = Self::extract_latency(bundle);
        let constraints_present = Self::has_constraints(bundle);
        let retry_policy = Self::has_retry_policy(bundle);
        let human_review = Self::has_human_review(bundle);

        let tier_outcome = Self::tier_outcome(&tier);
        let constraint_outcome = Self::constraint_outcome(constraints_present, success);
        let retry_outcome = Self::retry_outcome(retry_policy);
        let evaluation_outcome = evaluation_status.to_string();
        let cost_outcome = Self::cost_outcome(estimated_cost);
        let latency_outcome = Self::latency_outcome(latency_ms);
        let human_review_outcome = Self::human_review_outcome(human_review);

        let (failure_factors, success_factors) = Self::determine_factors(
            success,
            &tier,
            &task_class,
            complexity_score,
            constraints_present,
            &evaluation_outcome,
            &cost_outcome,
            &latency_outcome,
        );

        let signal_count = Self::count_available_signals(
            final_status,
            evaluation_status,
            &tier,
            estimated_cost,
            latency_ms,
        );
        let confidence = Self::compute_confidence(signal_count);

        let reason_labels = Self::build_reason_labels(
            success,
            final_status,
            evaluation_status,
            &tier,
            &cost_outcome,
            &latency_outcome,
            human_review,
        );

        OutcomeAttribution {
            schema_version: OUTCOME_ATTRIBUTION_SCHEMA_VERSION.to_string(),
            outcome,
            tier_outcome,
            constraint_outcome,
            retry_outcome,
            evaluation_outcome,
            cost_outcome,
            latency_outcome,
            human_review_outcome,
            likely_failure_factors: failure_factors,
            likely_success_factors: success_factors,
            confidence,
            reason_labels,
        }
    }

    fn resolve_success(bundle: &Value, final_status: &str, evaluation_status: &str) -> bool {
        if bundle
            .pointer("/execution_result/success")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return true;
        }

        let fs = final_status.to_ascii_lowercase();
        let es = evaluation_status.to_ascii_lowercase();

        if matches!(
            fs.as_str(),
            "failed" | "fail" | "error" | "cancelled" | "timeout" | "timed_out"
        ) || matches!(es.as_str(), "failed" | "fail" | "error")
        {
            return false;
        }

        matches!(
            fs.as_str(),
            "completed" | "success" | "succeeded" | "passed"
        ) || matches!(es.as_str(), "pass" | "passed" | "success" | "succeeded")
    }

    fn resolve_outcome(success: bool, final_status: &str, evaluation_status: &str) -> Outcome {
        if success {
            return Outcome::Passed;
        }
        let fs = final_status.to_ascii_lowercase();
        let es = evaluation_status.to_ascii_lowercase();
        if fs == "unknown" && es == "unknown" {
            return Outcome::Inconclusive;
        }
        Outcome::Failed
    }

    fn extract_tier(bundle: &Value) -> String {
        Self::str_from_paths(
            bundle,
            &[
                &["decision", "selected_tier"],
                &["decision", "analysis_snapshot", "selected_tier"],
            ],
        )
        .unwrap_or("unknown")
        .to_string()
    }

    fn extract_task_class(bundle: &Value) -> String {
        Self::str_from_paths(
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

    fn extract_complexity(bundle: &Value) -> Option<f64> {
        Self::f64_from_paths(
            bundle,
            &[
                &["analysis", "complexity_score"],
                &["decision", "analysis_snapshot", "complexity_score"],
            ],
        )
    }

    fn extract_cost(bundle: &Value) -> Option<f64> {
        Self::f64_from_paths(
            bundle,
            &[
                &["execution_result", "estimated_cost"],
                &["execution_result", "estimated_cost_usd"],
                &["record", "estimated_cost_usd"],
            ],
        )
    }

    fn extract_latency(bundle: &Value) -> Option<f64> {
        Self::f64_from_paths(
            bundle,
            &[
                &["execution_result", "latency_ms"],
                &["record", "latency_ms"],
            ],
        )
    }

    fn has_constraints(bundle: &Value) -> bool {
        bundle
            .pointer("/decision/constraints")
            .and_then(|v| {
                if let Value::Array(arr) = v {
                    Some(!arr.is_empty())
                } else {
                    v.as_str().map(|s| !s.is_empty())
                }
            })
            .unwrap_or(false)
    }

    fn has_retry_policy(bundle: &Value) -> bool {
        bundle
            .pointer("/decision/retry_policy")
            .map(|v| !v.is_null())
            .unwrap_or(false)
    }

    fn has_human_review(bundle: &Value) -> bool {
        bundle
            .pointer("/decision/human_review_flag")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn tier_outcome(tier: &str) -> String {
        if tier == "unknown" {
            "unmatched"
        } else {
            "matched"
        }
        .to_string()
    }

    fn constraint_outcome(constraints_present: bool, success: bool) -> String {
        if constraints_present {
            if success {
                "satisfied"
            } else {
                "violated"
            }
        } else {
            "satisfied"
        }
        .to_string()
    }

    fn retry_outcome(retry_policy: bool) -> String {
        if retry_policy { "used" } else { "not_used" }.to_string()
    }

    fn cost_outcome(estimated_cost: Option<f64>) -> String {
        match estimated_cost {
            Some(cost) if cost > COST_HIGH_THRESHOLD => "high",
            _ => "normal",
        }
        .to_string()
    }

    fn latency_outcome(latency_ms: Option<f64>) -> String {
        match latency_ms {
            Some(latency) if latency > LATENCY_HIGH_THRESHOLD_MS => "high",
            _ => "normal",
        }
        .to_string()
    }

    fn human_review_outcome(flag: bool) -> Option<String> {
        if flag {
            Some("required".to_string())
        } else {
            None
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn determine_factors(
        success: bool,
        tier: &str,
        task_class: &str,
        complexity: Option<f64>,
        constraints_present: bool,
        evaluation_outcome: &str,
        cost_outcome: &str,
        latency_outcome: &str,
    ) -> (Vec<String>, Vec<String>) {
        let mut failure_factors = Vec::new();
        let mut success_factors = Vec::new();

        if success {
            if tier != "unknown" {
                success_factors.push("tier_matched".to_string());
            }
            if task_class != "unknown" {
                success_factors.push("task_class_identified".to_string());
            }
            if let Some(c) = complexity {
                if c < 0.5 {
                    success_factors.push("low_complexity".to_string());
                }
            }
            if latency_outcome == "normal" {
                success_factors.push("fast_execution".to_string());
            }
            if cost_outcome == "normal" {
                success_factors.push("cost_efficient".to_string());
            }
        } else {
            if tier == "unknown" {
                failure_factors.push("tier_mismatch".to_string());
            }
            if let Some(c) = complexity {
                if c > 0.7 {
                    failure_factors.push("high_complexity".to_string());
                }
            }
            if constraints_present {
                failure_factors.push("constraint_violation".to_string());
            }
            if matches!(evaluation_outcome, "failed" | "fail" | "error") {
                failure_factors.push("evaluation_failed".to_string());
            }
            if cost_outcome == "high" {
                failure_factors.push("high_cost".to_string());
            }
            if latency_outcome == "high" {
                failure_factors.push("high_latency".to_string());
            }
        }

        (failure_factors, success_factors)
    }

    fn count_available_signals(
        final_status: &str,
        evaluation_status: &str,
        tier: &str,
        estimated_cost: Option<f64>,
        latency_ms: Option<f64>,
    ) -> u32 {
        let mut count: u32 = 0;
        if final_status != "unknown" {
            count += 1;
        }
        if evaluation_status != "unknown" {
            count += 1;
        }
        if tier != "unknown" {
            count += 1;
        }
        if estimated_cost.is_some() {
            count += 1;
        }
        if latency_ms.is_some() {
            count += 1;
        }
        count
    }

    fn compute_confidence(signal_count: u32) -> f64 {
        match signal_count {
            0 => 0.1,
            1 => 0.3,
            2 => 0.5,
            3 => 0.7,
            4 => 0.85,
            _ => 0.95,
        }
    }

    fn build_reason_labels(
        success: bool,
        final_status: &str,
        evaluation_status: &str,
        tier: &str,
        cost_outcome: &str,
        latency_outcome: &str,
        human_review: bool,
    ) -> Vec<String> {
        let mut labels = Vec::new();

        if success {
            labels.push("evaluation_pass".to_string());
        } else {
            labels.push("final_status_fail".to_string());
        }

        let fs = final_status.to_ascii_lowercase();
        if matches!(
            fs.as_str(),
            "failed" | "fail" | "error" | "cancelled" | "timeout" | "timed_out"
        ) {
            labels.push(format!("final_status_{}", fs));
        }

        if tier != "unknown" {
            labels.push("tier_matched".to_string());
        } else {
            labels.push("tier_unknown".to_string());
        }

        labels.push(format!("cost_{}", cost_outcome));
        labels.push(format!("latency_{}", latency_outcome));

        if human_review {
            labels.push("human_review_required".to_string());
        }

        let es = evaluation_status.to_ascii_lowercase();
        if matches!(es.as_str(), "pass" | "passed" | "success" | "succeeded") {
            labels.push("evaluation_status_pass".to_string());
        } else if matches!(es.as_str(), "failed" | "fail" | "error") {
            labels.push("evaluation_status_fail".to_string());
        }

        labels
    }

    fn str_from_paths<'a>(value: &'a Value, paths: &[&[&str]]) -> Option<&'a str> {
        paths.iter().find_map(|path| {
            let mut current = value;
            for part in *path {
                current = current.get(*part)?;
            }
            current.as_str()
        })
    }

    fn f64_from_paths(value: &Value, paths: &[&[&str]]) -> Option<f64> {
        paths.iter().find_map(|path| {
            let mut current = value;
            for part in *path {
                current = current.get(*part)?;
            }
            current.as_f64()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_bundle(
        success: Option<bool>,
        final_status: &str,
        evaluation_status: &str,
        tier: &str,
        cost: Option<f64>,
        latency: Option<f64>,
        constraints: bool,
        retry: bool,
        human_review: bool,
        complexity: Option<f64>,
    ) -> Value {
        let mut bundle = json!({});
        if let Some(s) = success {
            bundle["execution_result"]["success"] = json!(s);
        }
        bundle["record"]["final_status"] = json!(final_status);
        bundle["evaluation_result"]["status"] = json!(evaluation_status);
        bundle["decision"]["selected_tier"] = json!(tier);
        if let Some(c) = cost {
            bundle["execution_result"]["estimated_cost"] = json!(c);
        }
        if let Some(l) = latency {
            bundle["execution_result"]["latency_ms"] = json!(l);
        }
        if constraints {
            bundle["decision"]["constraints"] = json!(["constraint_a"]);
        }
        if retry {
            bundle["decision"]["retry_policy"] = json!({"max_retries": 3});
        }
        if human_review {
            bundle["decision"]["human_review_flag"] = json!(true);
        }
        if let Some(cx) = complexity {
            bundle["analysis"]["complexity_score"] = json!(cx);
        }
        bundle
    }

    #[test]
    fn test_attribute_passed() {
        let bundle = make_bundle(
            Some(true),
            "completed",
            "passed",
            "tier_1",
            Some(0.05),
            Some(1000.0),
            false,
            false,
            false,
            Some(0.3),
        );
        let attr = OutcomeAttributor::attribute_from_bundle(&bundle, "completed", "passed");
        assert_eq!(attr.outcome, Outcome::Passed);
        assert!(attr.likely_failure_factors.is_empty());
        assert!(!attr.likely_success_factors.is_empty());
        assert!(attr
            .likely_success_factors
            .contains(&"tier_matched".to_string()));
        assert!(attr
            .likely_success_factors
            .contains(&"low_complexity".to_string()));
        assert!(attr
            .likely_success_factors
            .contains(&"fast_execution".to_string()));
        assert_eq!(attr.tier_outcome, "matched");
        assert_eq!(attr.cost_outcome, "normal");
        assert_eq!(attr.latency_outcome, "normal");
        assert!(attr.reason_labels.contains(&"evaluation_pass".to_string()));
    }

    #[test]
    fn test_attribute_failed() {
        let bundle = make_bundle(
            Some(false),
            "failed",
            "failed",
            "unknown",
            Some(0.05),
            Some(1000.0),
            true,
            false,
            false,
            Some(0.9),
        );
        let attr = OutcomeAttributor::attribute_from_bundle(&bundle, "failed", "failed");
        assert_eq!(attr.outcome, Outcome::Failed);
        assert!(!attr.likely_failure_factors.is_empty());
        assert!(attr
            .likely_failure_factors
            .contains(&"tier_mismatch".to_string()));
        assert!(attr
            .likely_failure_factors
            .contains(&"high_complexity".to_string()));
        assert!(attr
            .likely_failure_factors
            .contains(&"constraint_violation".to_string()));
        assert!(attr
            .likely_failure_factors
            .contains(&"evaluation_failed".to_string()));
        assert!(attr.likely_success_factors.is_empty());
        assert_eq!(attr.tier_outcome, "unmatched");
        assert_eq!(attr.constraint_outcome, "violated");
        assert!(attr
            .reason_labels
            .contains(&"final_status_fail".to_string()));
    }

    #[test]
    fn test_attribute_inconclusive() {
        let bundle = make_bundle(
            Some(false),
            "unknown",
            "unknown",
            "tier_1",
            None,
            None,
            false,
            false,
            false,
            None,
        );
        let attr = OutcomeAttributor::attribute_from_bundle(&bundle, "unknown", "unknown");
        assert_eq!(attr.outcome, Outcome::Inconclusive);
        assert_eq!(attr.tier_outcome, "matched");
    }

    #[test]
    fn test_attribute_high_cost() {
        let bundle = make_bundle(
            Some(true),
            "completed",
            "passed",
            "tier_1",
            Some(0.50),
            Some(5000.0),
            false,
            false,
            false,
            None,
        );
        let attr = OutcomeAttributor::attribute_from_bundle(&bundle, "completed", "passed");
        assert_eq!(attr.cost_outcome, "high");
        assert_eq!(attr.outcome, Outcome::Passed);
        assert!(!attr
            .likely_success_factors
            .contains(&"cost_efficient".to_string()));
        assert!(attr.reason_labels.contains(&"cost_high".to_string()));
    }

    #[test]
    fn test_attribute_high_latency() {
        let bundle = make_bundle(
            Some(false),
            "failed",
            "failed",
            "tier_1",
            Some(0.05),
            Some(45000.0),
            false,
            false,
            false,
            None,
        );
        let attr = OutcomeAttributor::attribute_from_bundle(&bundle, "failed", "failed");
        assert_eq!(attr.latency_outcome, "high");
        assert_eq!(attr.outcome, Outcome::Failed);
        assert!(attr
            .likely_failure_factors
            .contains(&"high_latency".to_string()));
        assert!(attr.reason_labels.contains(&"latency_high".to_string()));
    }

    #[test]
    fn test_attribute_retry_used() {
        let bundle = make_bundle(
            Some(true),
            "completed",
            "passed",
            "tier_2",
            Some(0.01),
            Some(500.0),
            false,
            true,
            false,
            None,
        );
        let attr = OutcomeAttributor::attribute_from_bundle(&bundle, "completed", "passed");
        assert_eq!(attr.retry_outcome, "used");
    }

    #[test]
    fn test_attribute_human_review() {
        let bundle = make_bundle(
            Some(false),
            "failed",
            "failed",
            "tier_1",
            Some(0.02),
            Some(2000.0),
            false,
            false,
            true,
            None,
        );
        let attr = OutcomeAttributor::attribute_from_bundle(&bundle, "failed", "failed");
        assert_eq!(attr.human_review_outcome, Some("required".to_string()));
        assert!(attr
            .reason_labels
            .contains(&"human_review_required".to_string()));
    }

    #[test]
    fn test_attribute_confidence_scales_with_signals() {
        let bundle_minimal = json!({});
        let attr_min =
            OutcomeAttributor::attribute_from_bundle(&bundle_minimal, "unknown", "unknown");
        assert!(attr_min.confidence <= 0.3);

        let bundle_full = make_bundle(
            Some(true),
            "completed",
            "passed",
            "tier_1",
            Some(0.01),
            Some(500.0),
            false,
            false,
            false,
            None,
        );
        let attr_full =
            OutcomeAttributor::attribute_from_bundle(&bundle_full, "completed", "passed");
        assert!(attr_full.confidence >= 0.85);
    }

    #[test]
    fn test_attribute_schema_version() {
        let bundle = json!({});
        let attr = OutcomeAttributor::attribute_from_bundle(&bundle, "unknown", "unknown");
        assert_eq!(attr.schema_version, "outcome_attribution.v1");
    }
}
