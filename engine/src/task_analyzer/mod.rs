mod classify;
mod risk;
mod rules;
mod scoring;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::dispatch_decision::Evidence;
use crate::runtime::FixtureRuntime;
use crate::workflow::context_pack::{
    check_budget_compliance, validate_context_layers, ContextBudget, ContextLayers,
};

pub const TASK_ANALYSIS_SCHEMA_VERSION: &str = "task_analysis.v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskAnalysis {
    pub schema_version: String,
    pub analysis_id: String,
    pub raw_request_snapshot: String,
    pub request_source: String,
    pub primary_task_type: String,
    pub task_domain: String,
    pub task_intent: String,
    pub risk_flags: Vec<String>,
    pub complexity_score: f64,
    pub cognitive_complexity: f64,
    pub context_complexity: f64,
    pub execution_risk: f64,
    pub ambiguity_score: f64,
    pub required_capabilities: Vec<String>,
    pub context_budget_estimate: i64,
    pub execution_budget_estimate: i64,
    pub quality_requirement: String,
    pub risk_level: String,
    pub confidence: f64,
    pub confidence_label: String,
    pub uncertainty_reason: Vec<String>,
    pub safe_default: String,
    pub escalation_trigger: Option<String>,
    pub positive_evidence: Vec<Evidence>,
    pub negative_evidence: Vec<Evidence>,
    pub features_detected: Value,
    pub analysis_method: String,
    pub created_at: String,
}

impl TaskAnalysis {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("TaskAnalysis should serialize to JSON")
    }

    pub fn context_budget(&self) -> ContextBudget {
        ContextBudget {
            max_context_tokens: self.context_budget_estimate,
            preferred_context_tokens: (self.context_budget_estimate as f64 * 0.6) as i64,
            max_response_tokens: Some(self.execution_budget_estimate),
            reserved_response_tokens: None,
        }
    }

    pub fn context_pack_validation(&self) -> Value {
        let budget = self.context_budget();
        let layers = ContextLayers::default();
        let layers_value = layers.to_value();
        let layers_map: std::collections::HashMap<String, Value> =
            if let Value::Object(obj) = &layers_value {
                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            } else {
                std::collections::HashMap::new()
            };

        let layer_errors = validate_context_layers(&layers_map);

        let mut pack_data = std::collections::HashMap::new();
        pack_data.insert("context_budget".to_string(), budget.to_value());
        let (budget_compliant, budget_reason) = check_budget_compliance(&pack_data, 0);

        json!({
            "schema_version": "context_pack_validation.v1",
            "context_budget": {
                "max_context_tokens": budget.max_context_tokens,
                "preferred_context_tokens": budget.preferred_context_tokens,
                "max_response_tokens": budget.max_response_tokens,
            },
            "budget_compliant": budget_compliant,
            "budget_reason": budget_reason,
            "layer_validation_errors": layer_errors,
            "layer_validation_ok": layer_errors.is_empty(),
        })
    }
}

pub struct RuleBasedTaskAnalyzer;

impl Default for RuleBasedTaskAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleBasedTaskAnalyzer {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(&self, raw_request: &str, request_source: &str) -> TaskAnalysis {
        let mut runtime = FixtureRuntime::new();
        self.analyze_with_runtime(raw_request, request_source, &mut runtime)
    }

    pub fn analyze_with_runtime(
        &self,
        raw_request: &str,
        request_source: &str,
        runtime: &mut FixtureRuntime,
    ) -> TaskAnalysis {
        let text = raw_request.to_lowercase().trim().to_string();
        let positive_text = risk::positive_risk_text(&text);

        let domain = self.classify_domain(&text);
        let intent = self.classify_intent(&text);
        let (risk_flags, pos_evidence, neg_evidence) =
            self.detect_risk_flags(&text, &positive_text);
        let (cognitive, context, exec_risk, ambiguity) =
            self.compute_complexity(&text, domain, intent, &risk_flags);
        let complexity_score =
            round4(0.35 * cognitive + 0.25 * context + 0.25 * exec_risk + 0.15 * ambiguity);
        let (context_budget, execution_budget) = self.estimate_budgets(domain, intent, &text);
        let (confidence, confidence_label, uncertainty_reasons) =
            self.assess_confidence(domain, intent, &text, &risk_flags);
        let risk_level = self.derive_risk_level(&text, &risk_flags, domain, intent);
        let quality_req = self.derive_quality_requirement(&text, risk_level);
        let safe_default = self.determine_safe_default(confidence, risk_level);
        let escalation = self.determine_escalation(confidence, risk_level, &risk_flags);
        let capabilities = self.detect_capabilities(&text, domain, intent);
        let features = self.detect_features(&text, domain, intent, &risk_flags);

        TaskAnalysis {
            schema_version: TASK_ANALYSIS_SCHEMA_VERSION.to_string(),
            analysis_id: runtime.id("analysis-"),
            raw_request_snapshot: raw_request.to_string(),
            request_source: request_source.to_string(),
            primary_task_type: format!("{}_{}", domain, intent),
            task_domain: domain.to_string(),
            task_intent: intent.to_string(),
            risk_flags: risk_flags.into_iter().map(String::from).collect(),
            complexity_score,
            cognitive_complexity: round4(cognitive),
            context_complexity: round4(context),
            execution_risk: round4(exec_risk),
            ambiguity_score: round4(ambiguity),
            required_capabilities: capabilities.into_iter().map(String::from).collect(),
            context_budget_estimate: context_budget,
            execution_budget_estimate: execution_budget,
            quality_requirement: quality_req.to_string(),
            risk_level: risk_level.to_string(),
            confidence: round4(confidence),
            confidence_label: confidence_label.to_string(),
            uncertainty_reason: uncertainty_reasons.into_iter().map(String::from).collect(),
            safe_default: safe_default.to_string(),
            escalation_trigger: escalation.map(String::from),
            positive_evidence: pos_evidence,
            negative_evidence: neg_evidence,
            features_detected: features,
            analysis_method: "rule_only".to_string(),
            created_at: runtime.now(),
        }
    }
}

pub fn analyze(raw_request: &str, request_source: &str, runtime: &mut FixtureRuntime) -> Value {
    RuleBasedTaskAnalyzer::new()
        .analyze_with_runtime(raw_request, request_source, runtime)
        .to_value()
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_budget_derives_from_analysis() {
        let analyzer = RuleBasedTaskAnalyzer::new();
        let analysis = analyzer.analyze("Refactor the auth module", "api");
        let budget = analysis.context_budget();
        assert_eq!(budget.max_context_tokens, analysis.context_budget_estimate);
        assert!(budget.preferred_context_tokens <= budget.max_context_tokens);
        assert_eq!(
            budget.max_response_tokens,
            Some(analysis.execution_budget_estimate)
        );
    }

    #[test]
    fn context_pack_validation_produces_valid_output() {
        let analyzer = RuleBasedTaskAnalyzer::new();
        let analysis = analyzer.analyze("Write unit tests", "api");
        let validation = analysis.context_pack_validation();

        assert_eq!(validation["schema_version"], "context_pack_validation.v1");
        assert!(validation["budget_compliant"].as_bool().unwrap());
        assert!(validation["layer_validation_ok"].as_bool().unwrap());
        assert!(validation["layer_validation_errors"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn context_budget_scales_with_complexity() {
        let analyzer = RuleBasedTaskAnalyzer::new();
        let analysis = analyzer.analyze(
            "Deploy microservices architecture with CI/CD pipeline and monitoring",
            "api",
        );
        assert!(analysis.context_budget_estimate > 0);
        assert!(analysis.execution_budget_estimate > 0);
        assert!(analysis.complexity_score > 0.0);
    }

    #[test]
    fn context_pack_validation_includes_budget_fields() {
        let analyzer = RuleBasedTaskAnalyzer::new();
        let analysis = analyzer.analyze("Design API", "api");
        let validation = analysis.context_pack_validation();

        let cb = &validation["context_budget"];
        assert!(cb["max_context_tokens"].as_i64().unwrap() > 0);
        assert!(cb["preferred_context_tokens"].as_i64().unwrap() > 0);
    }
}
