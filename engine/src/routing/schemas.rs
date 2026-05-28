use serde::{Deserialize, Serialize};
use serde_json::Value;

// Schema versions
pub const ROUTING_EXPERIMENT_SCHEMA_VERSION: &str = "routing_experiment.v1";
pub const ROUTING_ARM_SCHEMA_VERSION: &str = "routing_arm.v1";
pub const ROUTING_OBSERVATION_SCHEMA_VERSION: &str = "routing_observation.v1";
pub const PROMOTION_VERDICT_SCHEMA_VERSION: &str = "promotion_verdict.v1";

// Constants
pub const EXPERIMENT_STATUSES: &[&str] = &["created", "running", "concluded", "rolled_back"];
pub const EXPERIMENT_CONCLUSIONS: &[&str] = &[
    "adopt_candidate",
    "keep_baseline",
    "inconclusive",
    "rolled_back",
];
pub const ROUTING_MODES: &[&str] = &["static", "adaptive", "shadow"];
pub const PROMOTION_VERDICTS: &[&str] = &["promote", "hold", "reject", "insufficient_data"];
pub const DOWNGRADE_REASONS: &[&str] =
    &["cost_optimization", "quality_sufficient", "budget_pressure"];
pub const UPGRADE_REASONS: &[&str] = &[
    "high_uncertainty",
    "failure_rate",
    "critical_task",
    "quality_risk",
];

pub const PROMOTION_GATE_MIN_SAMPLE_COUNT: usize = 30;
pub const PROMOTION_GATE_MAX_FAILURE_RATE_DELTA: f64 = 0.05;
pub const PROMOTION_GATE_MIN_COST_REDUCTION_PCT: f64 = 5.0;

const TASK_GROUP_SEP: &str = "/";

pub fn make_task_group(domain: &str, intent: &str) -> String {
    format!("{domain}{TASK_GROUP_SEP}{intent}")
}

pub fn parse_task_group(task_group: &str) -> (String, String) {
    let mut parts = task_group.splitn(2, TASK_GROUP_SEP);
    let domain = parts.next().unwrap_or("").to_string();
    let intent = parts.next().unwrap_or("").to_string();
    (domain, intent)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingObservation {
    pub schema_version: String,
    pub observation_id: String,
    pub arm_id: String,
    pub dispatch_id: String,
    pub task_domain: String,
    pub task_intent: String,
    pub selected_tier: String,
    pub baseline_tier: String,
    pub quality_score: f64,
    pub cost: f64,
    pub latency_ms: i64,
    pub success: bool,
    pub failure_domain: Option<String>,
    pub budget_violation: bool,
    pub observed_at: String,
}

impl RoutingObservation {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingArm {
    pub schema_version: String,
    pub arm_id: String,
    pub experiment_id: String,
    pub tier: String,
    pub profile_id: Option<String>,
    pub traffic_weight: f64,
    pub observations: Vec<RoutingObservation>,
}

impl RoutingArm {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingExperiment {
    pub schema_version: String,
    pub experiment_id: String,
    pub name: String,
    pub task_group: String,
    pub arms: Vec<RoutingArm>,
    pub status: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub conclusion: Option<String>,
}

impl RoutingExperiment {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotionVerdict {
    pub schema_version: String,
    pub verdict: String,
    pub task_group: String,
    pub candidate_tier: String,
    pub baseline_tier: String,
    pub sample_count: usize,
    pub quality_delta: f64,
    pub cost_reduction_pct: f64,
    pub failure_rate_delta: f64,
    pub reasons: Vec<String>,
    pub requires_human_review: bool,
}

impl PromotionVerdict {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingSelection {
    pub selected_tier: String,
    pub selected_profile_id: Option<String>,
    pub fallback_tier: String,
    pub fallback_profile_id: Option<String>,
    pub shadow_routes: Vec<Value>,
    pub rejected_candidates: Vec<Value>,
    pub routing_reason: String,
    pub routing_mode: String,
    pub routing_experiment_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostOfPassAggregate {
    pub tier: String,
    pub task_group: String,
    pub total_count: usize,
    pub failure_count: usize,
    pub total_cost: f64,
    pub total_quality: f64,
    pub cost_of_pass: Option<f64>,
}

impl CostOfPassAggregate {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageLedgerRow {
    pub row_id: String,
    pub dispatch_id: String,
    pub model_profile_id: String,
    pub cost_of_pass_group: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub estimated_cost: f64,
    pub quality_score: f64,
    pub success: bool,
    pub failure_domain: Option<String>,
    pub latency_ms: i64,
}

impl UsageLedgerRow {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

pub fn parse_cost_of_pass_group(group: &str) -> (String, String, String, String) {
    let parts: Vec<&str> = group.split('/').collect();
    let scope = parts.first().unwrap_or(&"").to_string();
    let family = parts.get(1).unwrap_or(&"").to_string();
    let variant = parts.get(2).unwrap_or(&"").to_string();
    let criterion = parts.get(3).unwrap_or(&"").to_string();
    (scope, family, variant, criterion)
}

pub fn aggregate_cost_of_pass(rows: &[UsageLedgerRow]) -> Option<CostOfPassAggregate> {
    if rows.is_empty() {
        return None;
    }
    let first = &rows[0];
    let tier = first.model_profile_id.clone();
    let group = first.cost_of_pass_group.clone();
    let (_, family, variant, _) = parse_cost_of_pass_group(&group);
    let task_group = make_task_group(&family, &variant);

    let total_count = rows.len();
    let failure_count = rows.iter().filter(|r| !r.success).count();
    let total_cost: f64 = rows.iter().map(|r| r.estimated_cost).sum();
    let total_quality: f64 = rows.iter().map(|r| r.quality_score).sum();

    let cost_of_pass = if total_count > 0 && total_quality > 0.0 {
        Some(total_cost / total_quality)
    } else {
        None
    };

    Some(CostOfPassAggregate {
        tier,
        task_group,
        total_count,
        failure_count,
        total_cost,
        total_quality,
        cost_of_pass,
    })
}
