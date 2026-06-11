use serde::{Deserialize, Serialize};

use super::run_trace_recorder::RunTrace;
use super::shadow_router::tier_cost_multiplier;

pub const POLICY_SIMULATION_SCHEMA_VERSION: &str = "policy_simulation_report.v1";

// ---------------------------------------------------------------------------
// PolicyCandidate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyCandidate {
    Cheapest,
    Balanced,
    Strong,
    ComplexityAware,
}

impl std::fmt::Display for PolicyCandidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cheapest => write!(f, "cheapest"),
            Self::Balanced => write!(f, "balanced"),
            Self::Strong => write!(f, "strong"),
            Self::ComplexityAware => write!(f, "complexity_aware"),
        }
    }
}

impl PolicyCandidate {
    pub fn select_tier(&self, trace: &RunTrace) -> String {
        match self {
            Self::Cheapest => "cheap_executor".to_string(),
            Self::Balanced => "balanced_worker".to_string(),
            Self::Strong => "strong_planner".to_string(),
            Self::ComplexityAware => {
                let c = trace.complexity_score.unwrap_or(0.5);
                if c >= 0.7 {
                    "strong_planner".to_string()
                } else if c >= 0.4 {
                    "balanced_worker".to_string()
                } else {
                    "cheap_executor".to_string()
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TierEstimate
// ---------------------------------------------------------------------------

pub struct TierEstimate {
    pub success_probability: f64,
    pub latency_multiplier: f64,
    pub human_review_probability: f64,
}

pub fn tier_estimate(tier: &str) -> TierEstimate {
    match tier {
        "cheap_executor" => TierEstimate {
            success_probability: 0.7,
            latency_multiplier: 0.8,
            human_review_probability: 0.3,
        },
        "balanced_worker" => TierEstimate {
            success_probability: 0.85,
            latency_multiplier: 1.0,
            human_review_probability: 0.15,
        },
        "strong_planner" => TierEstimate {
            success_probability: 0.95,
            latency_multiplier: 1.3,
            human_review_probability: 0.05,
        },
        "claude_code_cli" => TierEstimate {
            success_probability: 0.9,
            latency_multiplier: 1.5,
            human_review_probability: 0.1,
        },
        "codex_cli" => TierEstimate {
            success_probability: 0.8,
            latency_multiplier: 0.9,
            human_review_probability: 0.2,
        },
        _ => TierEstimate {
            success_probability: 0.8,
            latency_multiplier: 1.0,
            human_review_probability: 0.15,
        },
    }
}

// ---------------------------------------------------------------------------
// SimulationResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub schema_version: String,
    pub scenario_id: String,
    pub candidate_policy_id: String,
    pub input_trace_count: usize,
    pub actual_success_rate: f64,
    pub simulated_success_rate: f64,
    pub success_rate_delta: f64,
    pub actual_average_cost: f64,
    pub simulated_average_cost: f64,
    pub cost_delta: f64,
    pub actual_average_latency_ms: f64,
    pub simulated_average_latency_ms: f64,
    pub latency_delta: f64,
    pub actual_human_review_rate: f64,
    pub simulated_human_review_rate: f64,
    pub human_review_rate_delta: f64,
    pub assumptions: Vec<String>,
    pub evidence_trace_ids: Vec<String>,
    pub safety: String,
}

// ---------------------------------------------------------------------------
// PolicySimulator
// ---------------------------------------------------------------------------

pub struct PolicySimulator;

impl PolicySimulator {
    pub fn simulate(traces: &[RunTrace], policy: &PolicyCandidate) -> SimulationResult {
        let n = traces.len();
        if n == 0 {
            return SimulationResult {
                schema_version: POLICY_SIMULATION_SCHEMA_VERSION.to_string(),
                scenario_id: format!("sim-{policy}"),
                candidate_policy_id: format!("policy-{policy}"),
                input_trace_count: 0,
                actual_success_rate: 0.0,
                simulated_success_rate: 0.0,
                success_rate_delta: 0.0,
                actual_average_cost: 0.0,
                simulated_average_cost: 0.0,
                cost_delta: 0.0,
                actual_average_latency_ms: 0.0,
                simulated_average_latency_ms: 0.0,
                latency_delta: 0.0,
                actual_human_review_rate: 0.0,
                simulated_human_review_rate: 0.0,
                human_review_rate_delta: 0.0,
                assumptions: assumptions(),
                evidence_trace_ids: vec![],
                safety: "shadow_only / no_live_influence".to_string(),
            };
        }

        let mut actual_success: i64 = 0;
        let mut actual_cost: f64 = 0.0;
        let mut actual_latency: f64 = 0.0;
        let mut actual_review: i64 = 0;

        let mut sim_success: f64 = 0.0;
        let mut sim_cost: f64 = 0.0;
        let mut sim_latency: f64 = 0.0;
        let mut sim_review: f64 = 0.0;

        let mut evidence_trace_ids = Vec::with_capacity(n);

        for trace in traces {
            actual_success += trace.success as i64;
            actual_cost += trace.estimated_cost_usd.unwrap_or(trace.reserved_cost);
            actual_latency += trace.latency_ms.unwrap_or(0) as f64;
            actual_review += trace.human_review_flag as i64;

            let candidate_tier = policy.select_tier(trace);
            let est = tier_estimate(&candidate_tier);

            sim_success += est.success_probability;
            sim_cost += estimate_cost(trace, &candidate_tier);
            sim_latency += trace.latency_ms.unwrap_or(0) as f64 * est.latency_multiplier;
            sim_review += est.human_review_probability;

            evidence_trace_ids.push(trace.trace_id.clone());
        }

        let nf = n as f64;

        let actual_success_rate = actual_success as f64 / nf;
        let simulated_success_rate = sim_success / nf;
        let actual_average_cost = actual_cost / nf;
        let simulated_average_cost = sim_cost / nf;
        let actual_average_latency = actual_latency / nf;
        let simulated_average_latency = sim_latency / nf;
        let actual_review_rate = actual_review as f64 / nf;
        let simulated_review_rate = sim_review / nf;

        SimulationResult {
            schema_version: POLICY_SIMULATION_SCHEMA_VERSION.to_string(),
            scenario_id: format!("sim-{policy}"),
            candidate_policy_id: format!("policy-{policy}"),
            input_trace_count: n,
            actual_success_rate,
            simulated_success_rate,
            success_rate_delta: simulated_success_rate - actual_success_rate,
            actual_average_cost,
            simulated_average_cost,
            cost_delta: simulated_average_cost - actual_average_cost,
            actual_average_latency_ms: actual_average_latency,
            simulated_average_latency_ms: simulated_average_latency,
            latency_delta: simulated_average_latency - actual_average_latency,
            actual_human_review_rate: actual_review_rate,
            simulated_human_review_rate: simulated_review_rate,
            human_review_rate_delta: simulated_review_rate - actual_review_rate,
            assumptions: assumptions(),
            evidence_trace_ids,
            safety: "shadow_only / no_live_influence".to_string(),
        }
    }
}

fn estimate_cost(trace: &RunTrace, candidate_tier: &str) -> f64 {
    let base = trace.estimated_cost_usd.unwrap_or(trace.reserved_cost);
    let actual_mult = tier_cost_multiplier(&trace.selected_tier);
    let cand_mult = tier_cost_multiplier(candidate_tier);
    if actual_mult > 0.0 {
        cand_mult / actual_mult * base
    } else {
        base
    }
}

fn assumptions() -> Vec<String> {
    vec![
        "tier_success_rates_are_estimates".to_string(),
        "cost_estimates_use_relative_multipliers".to_string(),
        "no_actual_re_execution".to_string(),
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_trace() -> RunTrace {
        RunTrace {
            schema_version: "feedback_trace.v1".to_string(),
            trace_id: "trace-001".to_string(),
            dispatch_id: "disp-001".to_string(),
            history_id: None,
            created_at: None,
            task_class: "codegen".to_string(),
            task_domain: None,
            task_intent: None,
            selected_tier: "balanced_worker".to_string(),
            selected_profile: None,
            routing_policy: None,
            complexity_score: None,
            constraints: vec![],
            human_review_flag: false,
            retry_policy: None,
            shadow_routes: vec![],
            executor_type: "local".to_string(),
            execution_status: None,
            latency_ms: Some(1000),
            input_tokens: None,
            output_tokens: None,
            estimated_cost_usd: Some(0.05),
            reserved_cost: 0.05,
            total_cost: 0.0,
            retry_count: 0,
            evaluation_status: "pending".to_string(),
            final_status: "pending".to_string(),
            success: true,
            failure_domain: None,
            analysis: json!(null),
            decision: json!(null),
            execution: json!(null),
            evaluation: json!(null),
        }
    }

    #[test]
    fn empty_traces() {
        let result = PolicySimulator::simulate(&[], &PolicyCandidate::Cheapest);
        assert_eq!(result.input_trace_count, 0);
        assert_eq!(result.actual_success_rate, 0.0);
        assert_eq!(result.simulated_success_rate, 0.0);
        assert_eq!(result.success_rate_delta, 0.0);
        assert_eq!(result.actual_average_cost, 0.0);
        assert_eq!(result.simulated_average_cost, 0.0);
        assert_eq!(result.cost_delta, 0.0);
        assert_eq!(result.schema_version, POLICY_SIMULATION_SCHEMA_VERSION);
        assert_eq!(result.safety, "shadow_only / no_live_influence");
    }

    #[test]
    fn cheapest_selects() {
        let trace = minimal_trace();
        let tier = PolicyCandidate::Cheapest.select_tier(&trace);
        assert_eq!(tier, "cheap_executor");
    }

    #[test]
    fn balanced_selects() {
        let trace = minimal_trace();
        let tier = PolicyCandidate::Balanced.select_tier(&trace);
        assert_eq!(tier, "balanced_worker");
    }

    #[test]
    fn complexity_aware_high() {
        let mut trace = minimal_trace();
        trace.complexity_score = Some(0.8);
        let tier = PolicyCandidate::ComplexityAware.select_tier(&trace);
        assert_eq!(tier, "strong_planner");
    }

    #[test]
    fn complexity_aware_low() {
        let mut trace = minimal_trace();
        trace.complexity_score = Some(0.2);
        let tier = PolicyCandidate::ComplexityAware.select_tier(&trace);
        assert_eq!(tier, "cheap_executor");
    }

    #[test]
    fn success_rate_delta() {
        let trace = minimal_trace();
        let result = PolicySimulator::simulate(&[trace], &PolicyCandidate::Cheapest);
        // actual: 1.0 (success=true), simulated: 0.7 (cheap_executor)
        assert!((result.actual_success_rate - 1.0).abs() < 1e-10);
        assert!((result.simulated_success_rate - 0.7).abs() < 1e-10);
        assert!((result.success_rate_delta - (-0.3)).abs() < 1e-10);
    }

    #[test]
    fn cost_delta() {
        let trace = minimal_trace();
        let result = PolicySimulator::simulate(&[trace], &PolicyCandidate::Cheapest);
        // actual cost: 0.05 (balanced_worker mult=1.0), simulated: cheap_executor mult=0.5
        // 0.5/1.0 * 0.05 = 0.025
        assert!((result.actual_average_cost - 0.05).abs() < 1e-10);
        assert!((result.simulated_average_cost - 0.025).abs() < 1e-10);
        assert!((result.cost_delta - (-0.025)).abs() < 1e-10);
    }

    #[test]
    fn latency_delta() {
        let trace = minimal_trace();
        let result = PolicySimulator::simulate(&[trace], &PolicyCandidate::Strong);
        // actual: 1000ms, simulated: 1000 * 1.3 = 1300ms
        assert!((result.actual_average_latency_ms - 1000.0).abs() < 1e-10);
        assert!((result.simulated_average_latency_ms - 1300.0).abs() < 1e-10);
        assert!((result.latency_delta - 300.0).abs() < 1e-10);
    }

    #[test]
    fn review_delta() {
        let trace = minimal_trace();
        let result = PolicySimulator::simulate(&[trace], &PolicyCandidate::Cheapest);
        // actual: 0.0 (human_review_flag=false), simulated: 0.3 (cheap_executor)
        assert!((result.actual_human_review_rate - 0.0).abs() < 1e-10);
        assert!((result.simulated_human_review_rate - 0.3).abs() < 1e-10);
        assert!((result.human_review_rate_delta - 0.3).abs() < 1e-10);
    }

    #[test]
    fn safety_field() {
        let trace = minimal_trace();
        let result = PolicySimulator::simulate(&[trace], &PolicyCandidate::Balanced);
        assert_eq!(result.safety, "shadow_only / no_live_influence");
        assert_eq!(
            result.assumptions,
            vec![
                "tier_success_rates_are_estimates",
                "cost_estimates_use_relative_multipliers",
                "no_actual_re_execution",
            ]
        );
    }

    #[test]
    fn determinism() {
        let trace = minimal_trace();
        let r1 = PolicySimulator::simulate(
            std::slice::from_ref(&trace),
            &PolicyCandidate::ComplexityAware,
        );
        let trace2 = minimal_trace();
        let r2 = PolicySimulator::simulate(
            std::slice::from_ref(&trace2),
            &PolicyCandidate::ComplexityAware,
        );
        let j1 = serde_json::to_string(&r1).unwrap();
        let j2 = serde_json::to_string(&r2).unwrap();
        assert_eq!(j1, j2);
    }
}
