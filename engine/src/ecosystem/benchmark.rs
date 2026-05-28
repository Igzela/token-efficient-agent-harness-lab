use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const BENCHMARK_SCHEMA_VERSION: &str = "benchmark.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkTask {
    pub schema_version: String,
    pub task_id: String,
    pub prompt: String,
    pub expected_quality: f64,
    pub task_group: String,
    pub max_tokens: i64,
}

impl BenchmarkTask {
    pub fn to_dict(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub schema_version: String,
    pub task_id: String,
    pub model_name: String,
    pub provider: String,
    pub output: String,
    pub quality_score: f64,
    pub tokens_used: i64,
    pub latency_ms: f64,
    pub cost_usd: f64,
    pub passed: bool,
}

impl BenchmarkResult {
    pub fn to_dict(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Clone)]
pub struct ModelStats {
    pub avg_quality: f64,
    pub avg_latency: f64,
    pub avg_cost: f64,
    pub pass_rate: f64,
    pub task_count: usize,
}

pub struct BenchmarkSuite {
    tasks: HashMap<String, BenchmarkTask>,
    results: HashMap<String, Vec<BenchmarkResult>>,
}

impl Default for BenchmarkSuite {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchmarkSuite {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            results: HashMap::new(),
        }
    }

    pub fn validate_task(&self, task: &BenchmarkTask) -> Vec<String> {
        let mut errors = Vec::new();
        if task.task_id.is_empty() {
            errors.push("task_id is required".to_string());
        }
        if task.prompt.is_empty() {
            errors.push("prompt is required".to_string());
        }
        if !(0.0..=1.0).contains(&task.expected_quality) {
            errors.push("expected_quality must be between 0.0 and 1.0".to_string());
        }
        if task.task_group.is_empty() {
            errors.push("task_group is required".to_string());
        }
        if task.max_tokens <= 0 {
            errors.push("max_tokens must be positive".to_string());
        }
        if task.schema_version != BENCHMARK_SCHEMA_VERSION {
            errors.push(format!(
                "schema_version must be {}",
                BENCHMARK_SCHEMA_VERSION
            ));
        }
        errors
    }

    pub fn validate_result(&self, result: &BenchmarkResult) -> Vec<String> {
        let mut errors = Vec::new();
        if result.task_id.is_empty() {
            errors.push("task_id is required".to_string());
        }
        if result.model_name.is_empty() {
            errors.push("model_name is required".to_string());
        }
        if result.provider.is_empty() {
            errors.push("provider is required".to_string());
        }
        if !(0.0..=1.0).contains(&result.quality_score) {
            errors.push("quality_score must be between 0.0 and 1.0".to_string());
        }
        if result.tokens_used < 0 {
            errors.push("tokens_used must be non-negative".to_string());
        }
        if result.latency_ms < 0.0 {
            errors.push("latency_ms must be non-negative".to_string());
        }
        if result.cost_usd < 0.0 {
            errors.push("cost_usd must be non-negative".to_string());
        }
        if result.schema_version != BENCHMARK_SCHEMA_VERSION {
            errors.push(format!(
                "schema_version must be {}",
                BENCHMARK_SCHEMA_VERSION
            ));
        }
        errors
    }

    pub fn add_task(&mut self, task: &BenchmarkTask) -> bool {
        let errors = self.validate_task(task);
        if !errors.is_empty() {
            return false;
        }
        if self.tasks.contains_key(&task.task_id) {
            return false;
        }
        self.tasks.insert(task.task_id.clone(), task.clone());
        true
    }

    pub fn remove_task(&mut self, task_id: &str) -> bool {
        if self.tasks.remove(task_id).is_some() {
            self.results.remove(task_id);
            true
        } else {
            false
        }
    }

    pub fn list_tasks(&self) -> Vec<&BenchmarkTask> {
        self.tasks.values().collect()
    }

    pub fn record_result(&mut self, result: &BenchmarkResult) -> bool {
        let errors = self.validate_result(result);
        if !errors.is_empty() {
            return false;
        }
        if !self.tasks.contains_key(&result.task_id) {
            return false;
        }
        self.results
            .entry(result.task_id.clone())
            .or_default()
            .push(result.clone());
        true
    }

    pub fn results_for_model(&self, model_name: &str) -> Vec<&BenchmarkResult> {
        self.results
            .values()
            .flat_map(|results| results.iter())
            .filter(|r| r.model_name == model_name)
            .collect()
    }

    pub fn results_for_task(&self, task_id: &str) -> Vec<&BenchmarkResult> {
        self.results
            .get(task_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    pub fn compare_models(&self, model_a: &str, model_b: &str) -> serde_json::Value {
        let all_results: Vec<&BenchmarkResult> = self
            .results
            .values()
            .flat_map(|results| results.iter())
            .collect();

        let a_results: Vec<&&BenchmarkResult> = all_results
            .iter()
            .filter(|r| r.model_name == model_a)
            .collect();
        let b_results: Vec<&&BenchmarkResult> = all_results
            .iter()
            .filter(|r| r.model_name == model_b)
            .collect();

        serde_json::json!({
            "model_a": model_a,
            "model_b": model_b,
            "model_a_stats": stats_json(&a_results),
            "model_b_stats": stats_json(&b_results),
        })
    }

    pub fn leaderboard(&self) -> Vec<serde_json::Value> {
        let mut model_data: HashMap<String, Vec<&BenchmarkResult>> = HashMap::new();
        for results in self.results.values() {
            for r in results {
                model_data.entry(r.model_name.clone()).or_default().push(r);
            }
        }

        let mut entries: Vec<serde_json::Value> = model_data
            .iter()
            .map(|(model, results)| {
                let n = results.len() as f64;
                serde_json::json!({
                    "model": model,
                    "avg_quality": results.iter().map(|r| r.quality_score).sum::<f64>() / n,
                    "avg_latency": results.iter().map(|r| r.latency_ms).sum::<f64>() / n,
                    "avg_cost": results.iter().map(|r| r.cost_usd).sum::<f64>() / n,
                    "pass_rate": results.iter().filter(|r| r.passed).count() as f64 / n,
                    "task_count": results.len(),
                })
            })
            .collect();

        entries.sort_by(|a, b| {
            let aq = a["avg_quality"].as_f64().unwrap_or(0.0);
            let bq = b["avg_quality"].as_f64().unwrap_or(0.0);
            bq.partial_cmp(&aq).unwrap_or(std::cmp::Ordering::Equal)
        });

        entries
    }
}

fn stats_json(results: &[&&BenchmarkResult]) -> serde_json::Value {
    if results.is_empty() {
        return serde_json::json!({
            "avg_quality": 0.0,
            "avg_latency": 0.0,
            "avg_cost": 0.0,
            "pass_rate": 0.0,
            "task_count": 0,
        });
    }
    let n = results.len() as f64;
    serde_json::json!({
        "avg_quality": results.iter().map(|r| r.quality_score).sum::<f64>() / n,
        "avg_latency": results.iter().map(|r| r.latency_ms).sum::<f64>() / n,
        "avg_cost": results.iter().map(|r| r.cost_usd).sum::<f64>() / n,
        "pass_rate": results.iter().filter(|r| r.passed).count() as f64 / n,
        "task_count": results.len(),
    })
}
