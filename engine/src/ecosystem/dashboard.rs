use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DASHBOARD_SCHEMA_VERSION: &str = "dashboard.v1";
pub const VALID_WINNERS: &[&str] = &["a", "b", "tie"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExperimentResult {
    pub schema_version: String,
    pub experiment_id: String,
    pub model_a: String,
    pub model_b: String,
    pub task_group: String,
    pub metric_name: String,
    pub value_a: f64,
    pub value_b: f64,
    pub winner: String,
    pub sample_count: i64,
    pub created_at: f64,
}

impl ExperimentResult {
    pub fn to_dict(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub total_dispatches: i64,
    pub total_plans: i64,
    pub active_experiments: usize,
    pub cost_savings_pct: f64,
    pub quality_delta_pct: f64,
    pub top_models: Vec<(String, i64)>,
}

pub struct DispatchDashboard {
    experiments: HashMap<String, ExperimentResult>,
}

impl Default for DispatchDashboard {
    fn default() -> Self {
        Self::new()
    }
}

impl DispatchDashboard {
    pub fn new() -> Self {
        Self {
            experiments: HashMap::new(),
        }
    }

    pub fn validate_experiment(&self, result: &ExperimentResult) -> Vec<String> {
        let mut errors = Vec::new();
        if result.experiment_id.is_empty() {
            errors.push("experiment_id is required".to_string());
        }
        if result.model_a.is_empty() {
            errors.push("model_a is required".to_string());
        }
        if result.model_b.is_empty() {
            errors.push("model_b is required".to_string());
        }
        if result.task_group.is_empty() {
            errors.push("task_group is required".to_string());
        }
        if result.metric_name.is_empty() {
            errors.push("metric_name is required".to_string());
        }
        if !VALID_WINNERS.contains(&result.winner.as_str()) {
            errors.push(format!("winner must be one of {:?}", VALID_WINNERS));
        }
        if result.sample_count < 0 {
            errors.push("sample_count must be non-negative".to_string());
        }
        if result.schema_version != DASHBOARD_SCHEMA_VERSION {
            errors.push(format!(
                "schema_version must be {}",
                DASHBOARD_SCHEMA_VERSION
            ));
        }
        if !result.value_a.is_finite() || !result.value_b.is_finite() {
            errors.push("value_a and value_b must be finite numbers".to_string());
        }
        errors
    }

    pub fn record_experiment(&mut self, result: &ExperimentResult) -> bool {
        let errors = self.validate_experiment(result);
        if !errors.is_empty() {
            return false;
        }
        if self.experiments.contains_key(&result.experiment_id) {
            return false;
        }
        self.experiments
            .insert(result.experiment_id.clone(), result.clone());
        true
    }

    pub fn get_experiment(&self, experiment_id: &str) -> Option<&ExperimentResult> {
        self.experiments.get(experiment_id)
    }

    pub fn list_experiments(&self) -> Vec<&ExperimentResult> {
        self.experiments.values().collect()
    }

    pub fn experiments_by_model(&self, model_name: &str) -> Vec<&ExperimentResult> {
        self.experiments
            .values()
            .filter(|e| e.model_a == model_name || e.model_b == model_name)
            .collect()
    }

    pub fn experiments_by_task_group(&self, task_group: &str) -> Vec<&ExperimentResult> {
        self.experiments
            .values()
            .filter(|e| e.task_group == task_group)
            .collect()
    }

    pub fn compute_summary(&self, total_dispatches: i64, total_plans: i64) -> DashboardSummary {
        let experiments: Vec<&ExperimentResult> = self.experiments.values().collect();
        let active = experiments.len();

        let mut cost_savings = 0.0;
        let mut quality_delta = 0.0;
        let mut model_counter: HashMap<String, i64> = HashMap::new();

        for e in &experiments {
            *model_counter.entry(e.model_a.clone()).or_insert(0) += 1;
            *model_counter.entry(e.model_b.clone()).or_insert(0) += 1;

            if e.metric_name == "cost" && e.value_a != 0.0 {
                cost_savings += (e.value_b - e.value_a) / e.value_a.abs() * 100.0;
            } else if e.metric_name == "quality" && e.value_a != 0.0 {
                quality_delta += (e.value_b - e.value_a) / e.value_a.abs() * 100.0;
            }
        }

        if active > 0 {
            cost_savings /= active as f64;
            quality_delta /= active as f64;
        }

        let mut top: Vec<(String, i64)> = model_counter.into_iter().collect();
        top.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

        DashboardSummary {
            total_dispatches,
            total_plans,
            active_experiments: active,
            cost_savings_pct: cost_savings,
            quality_delta_pct: quality_delta,
            top_models: top,
        }
    }
}
