use crate::dispatch_decision::BudgetReservation;
use crate::runtime::FixtureRuntime;
use crate::task_analyzer::TaskAnalysis;

pub struct BudgetManager {
    default_currency: String,
}

impl Default for BudgetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BudgetManager {
    pub fn new() -> Self {
        Self {
            default_currency: "token".to_string(),
        }
    }

    pub fn create_reservation(
        &self,
        decision_id: &str,
        analysis: &TaskAnalysis,
        tier: &str,
        runtime: &mut FixtureRuntime,
    ) -> BudgetReservation {
        let input_tokens = analysis.context_budget_estimate;
        let output_tokens = analysis.execution_budget_estimate;
        let total_tokens = input_tokens + output_tokens;
        BudgetReservation {
            reservation_id: runtime.id("res-"),
            decision_id: decision_id.to_string(),
            currency: self.default_currency.clone(),
            pre_budget: total_tokens,
            reserved_input_tokens: input_tokens,
            reserved_output_tokens: output_tokens,
            reserved_total_tokens: total_tokens,
            reserved_cost: round6(self.estimate_cost(tier, input_tokens, output_tokens)),
            status: "reserved".to_string(),
            created_at: runtime.now(),
            updated_at: runtime.now(),
            ..BudgetReservation::default()
        }
    }

    pub fn check_violation(
        &self,
        reservation: &BudgetReservation,
        actual_tokens: i64,
    ) -> (bool, Option<String>) {
        if actual_tokens > reservation.reserved_total_tokens {
            let delta = actual_tokens - reservation.reserved_total_tokens;
            return (true, Some(format!("budget exceeded by {delta} tokens")));
        }
        (false, None)
    }

    pub fn estimate_cost(&self, tier: &str, input_tokens: i64, output_tokens: i64) -> f64 {
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
