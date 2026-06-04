use serde_json::{json, Value};

use super::rules::{BUDGET_BASE, INTENT_MULTIPLIER};
use super::RuleBasedTaskAnalyzer;

impl RuleBasedTaskAnalyzer {
    pub(super) fn compute_complexity(
        &self,
        text: &str,
        domain: &str,
        intent: &str,
        risk_flags: &[&str],
    ) -> (f64, f64, f64, f64) {
        let mut cognitive: f64 = 0.2;
        if ["architecture", "math", "code"].contains(&domain) {
            cognitive += 0.3;
        }
        if ["debug", "plan", "refactor", "generate"].contains(&intent) {
            cognitive += 0.2;
        }
        if text.contains("multi-step") || text.contains("trade-off") || text.contains("tradeoff") {
            cognitive += 0.2;
        }
        cognitive = cognitive.min(1.0);

        let mut context: f64 = 0.1;
        if text.contains("code")
            && (text.contains("block") || text.contains("file") || text.contains("module"))
        {
            context += 0.2;
        }
        if text.contains("500-file")
            || text.contains("large codebase")
            || text.contains("entire repo")
        {
            context += 0.4;
        }
        if text.contains("multi-file") || text.contains("cross-module") {
            context += 0.3;
        }
        if text.len() > 500 {
            context += 0.1;
        }
        context = context.min(1.0);

        let mut exec_risk: f64 = 0.0;
        for flag in risk_flags {
            if [
                "target_write",
                "provider_call",
                "sandbox_execution",
                "deployment",
                "destructive_operation",
            ]
            .contains(flag)
            {
                exec_risk += 0.25;
            } else if *flag == "secret_handling" {
                exec_risk += 0.3;
            } else {
                exec_risk += 0.1;
            }
        }
        exec_risk = exec_risk.min(1.0);

        let mut ambiguity: f64 = 0.1;
        for phrase in [
            "make it better",
            "improve",
            "optimize",
            "somehow",
            "whatever",
        ] {
            if text.contains(phrase) {
                ambiguity += 0.15;
            }
        }
        if text.contains("unclear") || text.contains("ambiguous") {
            ambiguity += 0.2;
        }
        if text.split_whitespace().count() < 5 {
            ambiguity += 0.2;
        }
        ambiguity = ambiguity.min(1.0);

        (cognitive, context, exec_risk, ambiguity)
    }

    pub(super) fn estimate_budgets(&self, domain: &str, intent: &str, text: &str) -> (i64, i64) {
        let base = BUDGET_BASE
            .iter()
            .find(|(d, _)| *d == domain)
            .map(|(_, v)| *v)
            .unwrap_or(2000);
        let multiplier = INTENT_MULTIPLIER
            .iter()
            .find(|(i, _)| *i == intent)
            .map(|(_, v)| *v)
            .unwrap_or(1.0);
        let mut context_budget = (base as f64 * multiplier) as i64;
        let mut execution_budget = (base as f64 * multiplier * 0.75) as i64;
        if text.contains("500 tokens") || text.contains("budget") {
            context_budget = context_budget.min(500);
            execution_budget = execution_budget.min(375);
        }
        (context_budget, execution_budget)
    }

    pub(super) fn assess_confidence(
        &self,
        domain: &str,
        intent: &str,
        text: &str,
        risk_flags: &[&str],
    ) -> (f64, &str, Vec<&str>) {
        let mut confidence = 0.8_f64;
        let mut reasons = Vec::new();

        if domain == "other" {
            confidence -= 0.2;
            reasons.push("domain_unclear");
        }
        if intent == "classify" {
            confidence -= 0.15;
            reasons.push("intent_unclear");
        }
        if text.split_whitespace().count() < 5 {
            confidence -= 0.2;
            reasons.push("request_too_short");
        }
        if text.contains("ambiguous") || text.contains("unclear") {
            confidence -= 0.15;
            reasons.push("explicit_ambiguity");
        }
        if risk_flags.contains(&"high_uncertainty") {
            confidence -= 0.1;
            reasons.push("high_uncertainty_flag");
        }

        confidence = confidence.clamp(0.0, 1.0);
        let label = if confidence >= 0.7 {
            "high"
        } else if confidence >= 0.4 {
            "medium"
        } else {
            "low"
        };
        (confidence, label, reasons)
    }

    pub(super) fn derive_risk_level(
        &self,
        text: &str,
        risk_flags: &[&str],
        domain: &str,
        intent: &str,
    ) -> &str {
        if is_read_only_advisory_risk_downgrade(text, domain, intent, risk_flags) {
            return "medium";
        }
        if risk_flags
            .iter()
            .any(|f| ["destructive_operation", "secret_handling", "deployment"].contains(f))
        {
            return "critical";
        }
        if risk_flags
            .iter()
            .any(|f| ["target_write", "provider_call", "sandbox_execution"].contains(f))
        {
            return "high";
        }
        if risk_flags.len() >= 2 {
            return "medium";
        }
        if ["governance", "infra"].contains(&domain) || intent == "audit" {
            return "medium";
        }
        "low"
    }

    pub(super) fn derive_quality_requirement(&self, text: &str, risk_level: &str) -> &str {
        if text.contains("critical")
            || text.contains("production-grade")
            || text.contains("must be")
        {
            return "critical";
        }
        if ["critical", "high"].contains(&risk_level) {
            return "high";
        }
        if text.contains("high quality") || text.contains("thorough") {
            return "high";
        }
        if text.contains("quick") || text.contains("draft") || text.contains("rough") {
            return "draft";
        }
        "standard"
    }

    pub(super) fn determine_safe_default(&self, confidence: f64, risk_level: &str) -> &str {
        if confidence < 0.4 {
            return "escalate_to_human";
        }
        if ["critical", "high"].contains(&risk_level) {
            return "noop_with_review";
        }
        "proceed_with_caution"
    }

    pub(super) fn determine_escalation(
        &self,
        confidence: f64,
        risk_level: &str,
        risk_flags: &[&str],
    ) -> Option<&str> {
        if confidence < 0.3 {
            return Some("low_confidence");
        }
        if risk_level == "critical" {
            return Some("critical_risk");
        }
        if risk_flags.contains(&"target_write") && risk_flags.contains(&"provider_call") {
            return Some("combined_boundary_risk");
        }
        None
    }

    pub(super) fn detect_capabilities(&self, text: &str, domain: &str, intent: &str) -> Vec<&str> {
        let mut capabilities = Vec::new();
        if domain == "code" {
            capabilities.push("code_analysis");
        }
        if ["generate", "refactor"].contains(&intent) {
            capabilities.push("code_generation");
        }
        if intent == "debug" {
            capabilities.push("error_diagnosis");
        }
        if domain == "math" {
            capabilities.push("mathematical_reasoning");
        }
        if domain == "architecture" {
            capabilities.push("system_design");
        }
        if text.contains("security") || text.contains("vulnerability") {
            capabilities.push("security_analysis");
        }
        if text.contains("test") {
            capabilities.push("test_generation");
        }
        capabilities
    }

    pub(super) fn detect_features(
        &self,
        text: &str,
        domain: &str,
        intent: &str,
        risk_flags: &[&str],
    ) -> Value {
        let has_file_refs = [".py", ".js", ".ts", ".yaml", ".json"]
            .iter()
            .any(|kw| text.contains(kw));
        let mut features = json!({
            "domain": domain,
            "intent": intent,
            "has_code_blocks": text.contains("```"),
            "has_file_refs": has_file_refs,
            "risk_flag_count": risk_flags.len(),
            "word_count": text.split_whitespace().count()
        });
        if is_read_only_advisory_risk_downgrade(text, domain, intent, risk_flags) {
            features["read_only_advisory"] = json!(true);
        }
        if has_explicit_dangerous_action(text) {
            features["explicit_dangerous_action"] = json!(true);
        }
        features
    }
}

pub(super) fn is_read_only_advisory_risk_downgrade(
    text: &str,
    domain: &str,
    intent: &str,
    risk_flags: &[&str],
) -> bool {
    is_read_only_advisory(text, domain, intent)
        && !has_explicit_dangerous_action(text)
        && risk_flags
            .iter()
            .any(|f| ["secret_handling", "deployment"].contains(f))
}

pub(super) fn is_read_only_advisory(text: &str, _domain: &str, intent: &str) -> bool {
    let advisory_intent = ["review", "summarize", "audit"].contains(&intent)
        || [
            "review",
            "assess",
            "summarize",
            "audit",
            "analyze",
            "advise",
            "评估",
            "审阅",
            "分析",
            "建议",
        ]
        .iter()
        .any(|kw| text.contains(kw));
    let read_only_constraint = [
        "read-only",
        "readonly",
        "review only",
        "audit only",
        "no write",
        "no writes",
        "no deploy",
        "no deployment",
        "do not write",
        "do not modify",
        "do not deploy",
        "without writes",
        "without deployment",
        "只读",
        "只输出",
        "不要修改",
        "不修改",
        "不要写",
        "不写入",
        "不要部署",
        "不部署",
        "不可部署",
        "不可写",
    ]
    .iter()
    .any(|kw| text.contains(kw));

    advisory_intent && read_only_constraint
}

pub(super) fn has_explicit_dangerous_action(text: &str) -> bool {
    [
        "show api key",
        "print api key",
        "display api key",
        "read api key",
        "show secret",
        "print secret",
        "display secret",
        "read secret",
        "output secret",
        "输出 api key",
        "显示 api key",
        "读取 api key",
        "输出密钥",
        "显示密钥",
        "读取密钥",
        "deploy to production",
        "deploy production",
        "ship to production",
        "部署到生产",
        "发布到生产",
        "上线到生产",
        "modify target repo",
        "write target repo",
        "commit changes",
        "push changes",
        "修改 target repo",
        "写入 target repo",
        "bypass auth",
        "disable auth",
        "disable audit",
        "bypass cost",
        "绕过 auth",
        "绕过认证",
        "关闭审计",
        "绕过成本",
    ]
    .iter()
    .any(|kw| text.contains(kw))
}
