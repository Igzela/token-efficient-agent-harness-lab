use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::feedback::run_trace_recorder::RunTrace;

pub const PATTERN_SCHEMA_VERSION: &str = "feedback_pattern.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PatternType {
    TierFailureConcentration,
    TaskClassFailureConcentration,
    HighCostPerPass,
    HighLatencyCluster,
    RepeatedHumanReview,
    RetryHeavyTaskClass,
    InconclusiveEvaluationConcentration,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PatternSeverity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPattern {
    pub schema_version: String,
    pub pattern_id: String,
    pub pattern_type: PatternType,
    pub affected_tier: Option<String>,
    pub affected_task_class: Option<String>,
    pub count: usize,
    pub denominator: usize,
    pub rate: f64,
    pub evidence_trace_ids: Vec<String>,
    pub severity: PatternSeverity,
    pub recommendation_hint: String,
}

pub struct PatternDetector {
    pub min_sample_count: usize,
    pub failure_rate_threshold: f64,
    pub high_cost_threshold: f64,
    pub high_latency_threshold: i64,
}

impl Default for PatternDetector {
    fn default() -> Self {
        Self {
            min_sample_count: 3,
            failure_rate_threshold: 0.5,
            high_cost_threshold: 0.10,
            high_latency_threshold: 30000,
        }
    }
}

impl PatternDetector {
    pub fn detect(&self, traces: &[RunTrace]) -> Vec<DetectedPattern> {
        let mut patterns = Vec::new();

        patterns.extend(self.detect_tier_failure_concentration(traces));
        patterns.extend(self.detect_task_class_failure_concentration(traces));
        patterns.extend(self.detect_high_cost_per_pass(traces));
        patterns.extend(self.detect_high_latency_cluster(traces));
        patterns.extend(self.detect_repeated_human_review(traces));
        patterns.extend(self.detect_retry_heavy_task_class(traces));
        patterns.extend(self.detect_inconclusive_evaluation_concentration(traces));

        patterns
    }

    fn detect_tier_failure_concentration(&self, traces: &[RunTrace]) -> Vec<DetectedPattern> {
        let groups = group_by(traces, |t| Some(t.selected_tier.clone()));
        let mut patterns = Vec::new();

        for (tier, group) in groups {
            let Some(ref tier_key) = tier else {
                continue;
            };
            let count = group.len();
            if count < self.min_sample_count {
                continue;
            }

            let failed = group.iter().filter(|t| !t.success).count();
            let rate = failed as f64 / count as f64;

            if rate > self.failure_rate_threshold {
                let severity = classify_failure_severity(rate);
                patterns.push(DetectedPattern {
                    schema_version: PATTERN_SCHEMA_VERSION.to_string(),
                    pattern_id: format!("pattern-TierFailureConcentration-{}", tier_key),
                    pattern_type: PatternType::TierFailureConcentration,
                    affected_tier: tier.clone(),
                    affected_task_class: None,
                    count: failed,
                    denominator: count,
                    rate,
                    evidence_trace_ids: group.iter().filter(|t| !t.success).map(|t| t.trace_id.clone()).collect(),
                    severity,
                    recommendation_hint: format!(
                        "Tier '{}' has a high failure rate ({:.1}%). Consider adjusting dispatch routing or reviewing executor health.",
                        tier_key,
                        rate * 100.0
                    ),
                });
            }
        }

        patterns
    }

    fn detect_task_class_failure_concentration(&self, traces: &[RunTrace]) -> Vec<DetectedPattern> {
        let groups = group_by(traces, |t| Some(t.task_class.clone()));
        let mut patterns = Vec::new();

        for (task_class, group) in groups {
            let Some(ref tc_key) = task_class else {
                continue;
            };
            let count = group.len();
            if count < self.min_sample_count {
                continue;
            }

            let failed = group.iter().filter(|t| !t.success).count();
            let rate = failed as f64 / count as f64;

            if rate > self.failure_rate_threshold {
                let severity = classify_failure_severity(rate);
                patterns.push(DetectedPattern {
                    schema_version: PATTERN_SCHEMA_VERSION.to_string(),
                    pattern_id: format!("pattern-TaskClassFailureConcentration-{}", tc_key),
                    pattern_type: PatternType::TaskClassFailureConcentration,
                    affected_tier: None,
                    affected_task_class: task_class.clone(),
                    count: failed,
                    denominator: count,
                    rate,
                    evidence_trace_ids: group.iter().filter(|t| !t.success).map(|t| t.trace_id.clone()).collect(),
                    severity,
                    recommendation_hint: format!(
                        "Task class '{}' has a high failure rate ({:.1}%). Consider reviewing task definitions or model selection for this class.",
                        tc_key,
                        rate * 100.0
                    ),
                });
            }
        }

        patterns
    }

    fn detect_high_cost_per_pass(&self, traces: &[RunTrace]) -> Vec<DetectedPattern> {
        let groups = group_by_composite(traces, |t| {
            (Some(t.task_class.clone()), Some(t.selected_tier.clone()))
        });
        let mut patterns = Vec::new();

        for ((task_class, tier), group) in groups {
            let count = group.len();
            if count < self.min_sample_count {
                continue;
            }

            let pass_count = group.iter().filter(|t| t.success).count();
            if pass_count == 0 {
                continue;
            }

            let total_cost: f64 = group.iter().map(|t| t.total_cost).sum();
            let cost_per_pass = total_cost / pass_count as f64;

            if cost_per_pass > self.high_cost_threshold {
                let tc_key = task_class.as_deref().unwrap_or("unknown").to_string();
                let tier_key = tier.as_deref().unwrap_or("unknown").to_string();
                patterns.push(DetectedPattern {
                    schema_version: PATTERN_SCHEMA_VERSION.to_string(),
                    pattern_id: format!(
                        "pattern-HighCostPerPass-{}_{}",
                        tc_key, tier_key
                    ),
                    pattern_type: PatternType::HighCostPerPass,
                    affected_tier: tier,
                    affected_task_class: task_class,
                    count: pass_count,
                    denominator: count,
                    rate: cost_per_pass,
                    evidence_trace_ids: group.iter().filter(|t| t.success).map(|t| t.trace_id.clone()).collect(),
                    severity: if cost_per_pass > self.high_cost_threshold * 3.0 {
                        PatternSeverity::High
                    } else if cost_per_pass > self.high_cost_threshold * 2.0 {
                        PatternSeverity::Medium
                    } else {
                        PatternSeverity::Low
                    },
                    recommendation_hint: format!(
                        "Cost per pass is ${:.4} for {}/{}. Consider switching to a cheaper tier or optimizing the task definition.",
                        cost_per_pass, tc_key, tier_key
                    ),
                });
            }
        }

        patterns
    }

    fn detect_high_latency_cluster(&self, traces: &[RunTrace]) -> Vec<DetectedPattern> {
        let groups = group_by_composite(traces, |t| {
            (Some(t.task_class.clone()), Some(t.selected_tier.clone()))
        });
        let mut patterns = Vec::new();

        for ((task_class, tier), group) in groups {
            let high_latency: Vec<&RunTrace> = group
                .iter()
                .copied()
                .filter(|t| t.latency_ms.unwrap_or(0) > self.high_latency_threshold)
                .collect();

            if high_latency.len() < self.min_sample_count {
                continue;
            }

            let tc_key = task_class.as_deref().unwrap_or("unknown").to_string();
            let tier_key = tier.as_deref().unwrap_or("unknown").to_string();
            patterns.push(DetectedPattern {
                schema_version: PATTERN_SCHEMA_VERSION.to_string(),
                pattern_id: format!(
                    "pattern-HighLatencyCluster-{}_{}",
                    tc_key, tier_key
                ),
                pattern_type: PatternType::HighLatencyCluster,
                affected_tier: tier,
                affected_task_class: task_class,
                count: high_latency.len(),
                denominator: group.len(),
                rate: high_latency.len() as f64 / group.len() as f64,
                evidence_trace_ids: high_latency.iter().map(|t| t.trace_id.clone()).collect(),
                severity: if high_latency.len() > group.len() / 2 {
                    PatternSeverity::High
                } else {
                    PatternSeverity::Medium
                },
                recommendation_hint: format!(
                    "{} of {} traces in {}/{} exceed {}ms latency. Investigate executor or provider bottlenecks.",
                    high_latency.len(),
                    group.len(),
                    tc_key,
                    tier_key,
                    self.high_latency_threshold
                ),
            });
        }

        patterns
    }

    fn detect_repeated_human_review(&self, traces: &[RunTrace]) -> Vec<DetectedPattern> {
        let groups = group_by(traces, |t| Some(t.task_class.clone()));
        let mut patterns = Vec::new();

        for (task_class, group) in groups {
            let tc_key = match task_class.as_deref() {
                Some(k) => k.to_string(),
                None => continue,
            };
            let reviewed: Vec<&RunTrace> = group
                .iter()
                .copied()
                .filter(|t| t.human_review_flag)
                .collect();

            if reviewed.len() < self.min_sample_count {
                continue;
            }

            patterns.push(DetectedPattern {
                schema_version: PATTERN_SCHEMA_VERSION.to_string(),
                pattern_id: format!("pattern-RepeatedHumanReview-{}", tc_key),
                pattern_type: PatternType::RepeatedHumanReview,
                affected_tier: None,
                affected_task_class: task_class,
                count: reviewed.len(),
                denominator: group.len(),
                rate: reviewed.len() as f64 / group.len() as f64,
                evidence_trace_ids: reviewed.iter().map(|t| t.trace_id.clone()).collect(),
                severity: if reviewed.len() > group.len() / 2 {
                    PatternSeverity::High
                } else {
                    PatternSeverity::Medium
                },
                recommendation_hint: format!(
                    "Task class '{}' has {} human reviews out of {} traces. Consider improving automation or model quality.",
                    tc_key,
                    reviewed.len(),
                    group.len()
                ),
            });
        }

        patterns
    }

    fn detect_retry_heavy_task_class(&self, traces: &[RunTrace]) -> Vec<DetectedPattern> {
        let groups = group_by(traces, |t| Some(t.task_class.clone()));
        let mut patterns = Vec::new();

        for (task_class, group) in groups {
            let tc_key = match task_class.as_deref() {
                Some(k) => k.to_string(),
                None => continue,
            };
            if group.len() < self.min_sample_count {
                continue;
            }

            let avg_retry: f64 =
                group.iter().map(|t| t.retry_count as f64).sum::<f64>() / group.len() as f64;

            if avg_retry > 1.0 {
                patterns.push(DetectedPattern {
                    schema_version: PATTERN_SCHEMA_VERSION.to_string(),
                    pattern_id: format!("pattern-RetryHeavyTaskClass-{}", tc_key),
                    pattern_type: PatternType::RetryHeavyTaskClass,
                    affected_tier: None,
                    affected_task_class: task_class,
                    count: group.iter().filter(|t| t.retry_count > 1).count(),
                    denominator: group.len(),
                    rate: avg_retry,
                    evidence_trace_ids: group.iter().filter(|t| t.retry_count > 1).map(|t| t.trace_id.clone()).collect(),
                    severity: if avg_retry > 3.0 {
                        PatternSeverity::High
                    } else if avg_retry > 2.0 {
                        PatternSeverity::Medium
                    } else {
                        PatternSeverity::Low
                    },
                    recommendation_hint: format!(
                        "Task class '{}' averages {:.1} retries per run. Review retry policy or root causes.",
                        tc_key, avg_retry
                    ),
                });
            }
        }

        patterns
    }

    fn detect_inconclusive_evaluation_concentration(
        &self,
        traces: &[RunTrace],
    ) -> Vec<DetectedPattern> {
        let groups = group_by(traces, |t| Some(t.task_class.clone()));
        let mut patterns = Vec::new();

        for (task_class, group) in groups {
            let tc_key = match task_class.as_deref() {
                Some(k) => k.to_string(),
                None => continue,
            };
            if group.len() < self.min_sample_count {
                continue;
            }

            let inconclusive = group
                .iter()
                .filter(|t| t.evaluation_status == "inconclusive")
                .count();
            let rate = inconclusive as f64 / group.len() as f64;

            if rate > 0.3 {
                patterns.push(DetectedPattern {
                    schema_version: PATTERN_SCHEMA_VERSION.to_string(),
                    pattern_id: format!(
                        "pattern-InconclusiveEvaluationConcentration-{}",
                        tc_key
                    ),
                    pattern_type: PatternType::InconclusiveEvaluationConcentration,
                    affected_tier: None,
                    affected_task_class: task_class,
                    count: inconclusive,
                    denominator: group.len(),
                    rate,
                    evidence_trace_ids: group
                        .iter()
                        .filter(|t| t.evaluation_status == "inconclusive")
                        .map(|t| t.trace_id.clone())
                        .collect(),
                    severity: if rate > 0.6 {
                        PatternSeverity::High
                    } else if rate > 0.4 {
                        PatternSeverity::Medium
                    } else {
                        PatternSeverity::Low
                    },
                    recommendation_hint: format!(
                        "Task class '{}' has {:.1}% inconclusive evaluations. Review evaluation criteria or improve task definitions.",
                        tc_key,
                        rate * 100.0
                    ),
                });
            }
        }

        patterns
    }
}

fn classify_failure_severity(rate: f64) -> PatternSeverity {
    if rate >= 0.8 {
        PatternSeverity::High
    } else if rate >= 0.6 {
        PatternSeverity::Medium
    } else {
        PatternSeverity::Low
    }
}

fn group_by<F>(traces: &[RunTrace], key_fn: F) -> HashMap<Option<String>, Vec<&RunTrace>>
where
    F: Fn(&RunTrace) -> Option<String>,
{
    let mut groups: HashMap<Option<String>, Vec<&RunTrace>> = HashMap::new();
    for trace in traces {
        groups.entry(key_fn(trace)).or_default().push(trace);
    }
    groups
}

fn group_by_composite<F>(
    traces: &[RunTrace],
    key_fn: F,
) -> HashMap<(Option<String>, Option<String>), Vec<&RunTrace>>
where
    F: Fn(&RunTrace) -> (Option<String>, Option<String>),
{
    let mut groups: HashMap<(Option<String>, Option<String>), Vec<&RunTrace>> = HashMap::new();
    for trace in traces {
        groups.entry(key_fn(trace)).or_default().push(trace);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::run_trace_recorder::RunTrace;

    fn make_trace(
        trace_id: &str,
        tier: &str,
        task_class: &str,
        passed: bool,
        cost: f64,
        latency_ms: i64,
        human_review_flag: bool,
        retry_count: i64,
        evaluation_status: &str,
    ) -> RunTrace {
        RunTrace {
            schema_version: "feedback_trace.v1".to_string(),
            trace_id: trace_id.to_string(),
            dispatch_id: format!("dispatch-{}", trace_id),
            history_id: None,
            created_at: None,
            task_class: task_class.to_string(),
            task_domain: None,
            task_intent: None,
            selected_tier: tier.to_string(),
            selected_profile: None,
            routing_policy: None,
            complexity_score: None,
            constraints: Vec::new(),
            human_review_flag,
            retry_policy: if retry_count > 0 {
                Some("auto".to_string())
            } else {
                None
            },
            shadow_routes: Vec::new(),
            executor_type: "unknown".to_string(),
            execution_status: None,
            latency_ms: Some(latency_ms),
            input_tokens: None,
            output_tokens: None,
            estimated_cost_usd: Some(cost),
            reserved_cost: 0.0,
            total_cost: cost,
            retry_count,
            evaluation_status: evaluation_status.to_string(),
            final_status: if passed { "completed" } else { "failed" }.to_string(),
            success: passed,
            failure_domain: None,
            analysis: serde_json::Value::Null,
            decision: serde_json::Value::Null,
            execution: serde_json::Value::Null,
            evaluation: serde_json::Value::Null,
        }
    }

    #[test]
    fn test_detect_tier_failure_concentration() {
        let detector = PatternDetector::default();
        let traces = vec![
            make_trace(
                "t1",
                "cheap_executor",
                "code",
                false,
                0.01,
                1000,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t2",
                "cheap_executor",
                "code",
                false,
                0.02,
                1500,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t3",
                "cheap_executor",
                "code",
                false,
                0.01,
                1200,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t4",
                "cheap_executor",
                "code",
                false,
                0.03,
                800,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t5",
                "cheap_executor",
                "code",
                true,
                0.01,
                900,
                false,
                0,
                "pass",
            ),
        ];

        let patterns = detector.detect(&traces);
        let tier_patterns: Vec<&DetectedPattern> = patterns
            .iter()
            .filter(|p| p.pattern_type == PatternType::TierFailureConcentration)
            .collect();

        assert_eq!(tier_patterns.len(), 1);
        let p = &tier_patterns[0];
        assert_eq!(p.affected_tier.as_deref(), Some("cheap_executor"));
        assert_eq!(p.count, 4);
        assert_eq!(p.denominator, 5);
        assert!((p.rate - 0.8).abs() < f64::EPSILON);
        assert_eq!(p.severity, PatternSeverity::High);
        assert_eq!(p.evidence_trace_ids.len(), 4);
        assert!(p.pattern_id.contains("TierFailureConcentration"));
        assert!(p.pattern_id.contains("cheap_executor"));
    }

    #[test]
    fn test_detect_task_class_failure_concentration() {
        let detector = PatternDetector::default();
        let traces = vec![
            make_trace(
                "t1",
                "balanced_worker",
                "code_review",
                false,
                0.01,
                1000,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t2",
                "balanced_worker",
                "code_review",
                false,
                0.02,
                1500,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t3",
                "balanced_worker",
                "code_review",
                false,
                0.01,
                1200,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t4",
                "balanced_worker",
                "code_review",
                false,
                0.03,
                800,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t5",
                "balanced_worker",
                "code_review",
                true,
                0.01,
                900,
                false,
                0,
                "pass",
            ),
        ];

        let patterns = detector.detect(&traces);
        let tc_patterns: Vec<&DetectedPattern> = patterns
            .iter()
            .filter(|p| p.pattern_type == PatternType::TaskClassFailureConcentration)
            .collect();

        assert_eq!(tc_patterns.len(), 1);
        let p = &tc_patterns[0];
        assert_eq!(p.affected_task_class.as_deref(), Some("code_review"));
        assert_eq!(p.count, 4);
        assert_eq!(p.denominator, 5);
        assert!((p.rate - 0.8).abs() < f64::EPSILON);
        assert_eq!(p.severity, PatternSeverity::High);
        assert!(p.pattern_id.contains("TaskClassFailureConcentration"));
        assert!(p.pattern_id.contains("code_review"));
    }

    #[test]
    fn test_detect_high_cost_per_pass() {
        let detector = PatternDetector {
            high_cost_threshold: 0.10,
            ..Default::default()
        };
        let traces = vec![
            make_trace(
                "t1",
                "strong_planner",
                "debug",
                true,
                0.50,
                1000,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t2",
                "strong_planner",
                "debug",
                true,
                0.60,
                1200,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t3",
                "strong_planner",
                "debug",
                true,
                0.55,
                900,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t4",
                "strong_planner",
                "debug",
                false,
                0.10,
                800,
                false,
                0,
                "pass",
            ),
        ];

        let patterns = detector.detect(&traces);
        let cost_patterns: Vec<&DetectedPattern> = patterns
            .iter()
            .filter(|p| p.pattern_type == PatternType::HighCostPerPass)
            .collect();

        assert_eq!(cost_patterns.len(), 1);
        let p = &cost_patterns[0];
        assert_eq!(p.affected_tier.as_deref(), Some("strong_planner"));
        assert_eq!(p.affected_task_class.as_deref(), Some("debug"));
        assert_eq!(p.count, 3);
        assert!(p.rate > 0.10);
        assert!(p.pattern_id.contains("HighCostPerPass"));
    }

    #[test]
    fn test_detect_empty_traces() {
        let detector = PatternDetector::default();
        let traces: Vec<RunTrace> = vec![];
        let patterns = detector.detect(&traces);
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_detect_below_threshold() {
        let detector = PatternDetector::default();
        let traces = vec![
            make_trace(
                "t1",
                "cheap_executor",
                "code",
                false,
                0.01,
                1000,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t2",
                "cheap_executor",
                "code",
                false,
                0.02,
                1500,
                false,
                0,
                "pass",
            ),
        ];

        let patterns = detector.detect(&traces);
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_pattern_schema_fields() {
        let detector = PatternDetector::default();
        let traces = vec![
            make_trace(
                "t1",
                "cheap_executor",
                "generate",
                false,
                0.01,
                1000,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t2",
                "cheap_executor",
                "generate",
                false,
                0.02,
                1500,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t3",
                "cheap_executor",
                "generate",
                false,
                0.01,
                1200,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t4",
                "cheap_executor",
                "generate",
                false,
                0.03,
                800,
                false,
                0,
                "pass",
            ),
            make_trace(
                "t5",
                "cheap_executor",
                "generate",
                true,
                0.01,
                900,
                false,
                0,
                "pass",
            ),
        ];

        let patterns = detector.detect(&traces);
        assert!(!patterns.is_empty());

        let p = &patterns[0];
        assert_eq!(p.schema_version, PATTERN_SCHEMA_VERSION);
        assert!(!p.pattern_id.is_empty());
        assert!(!p.recommendation_hint.is_empty());
        assert!(!p.evidence_trace_ids.is_empty());
        assert!(p.denominator > 0);
        assert!(p.rate > 0.0);

        let serialized = serde_json::to_string(&p).unwrap();
        assert!(serialized.contains("schema_version"));
        assert!(serialized.contains("pattern_id"));
        assert!(serialized.contains("pattern_type"));
        assert!(serialized.contains("severity"));
        assert!(serialized.contains("evidence_trace_ids"));
    }
}
