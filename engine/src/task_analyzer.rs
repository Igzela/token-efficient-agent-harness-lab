use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::dispatch_decision::Evidence;
use crate::runtime::FixtureRuntime;

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
}

// ---------------------------------------------------------------------------
// Static keyword maps
// ---------------------------------------------------------------------------

static NEGATED_RISK_PHRASES: &[&str] = &[
    "no target repo writes",
    "no target repository writes",
    "do not write target repo",
    "do not write target repository",
    "target repo remains read-only",
    "target repository remains read-only",
    "without target repo writes",
    "without target repository writes",
    "no source changes",
    "does not modify target repo",
    "does not modify target repository",
    "no target repository mutation",
    "no target repo mutation",
    "without any provider calls",
    "without provider calls",
    "without model calls",
    "without any model calls",
    "no provider calls",
    "no model calls",
    "do not call providers",
    "do not call any providers",
    "no api key",
    "no credentials",
    "without any sandbox execution",
    "without sandbox execution",
    "without executing commands",
    "no sandbox execution",
    "no sandbox",
    "do not run sandbox",
    "no container",
    "no worker",
    "no autonomous workers",
    "read-only validation",
    "audit only",
    "review only",
];

static PHRASE_FLAGS: LazyLock<HashMap<&'static str, Vec<&'static str>>> = LazyLock::new(|| {
    let mut m: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
    for phrase in &[
        "no target repo writes",
        "no target repository writes",
        "do not write target repo",
        "do not write target repository",
        "target repo remains read-only",
        "target repository remains read-only",
        "without target repo writes",
        "without target repository writes",
        "no source changes",
        "does not modify target repo",
        "does not modify target repository",
        "no target repository mutation",
        "no target repo mutation",
        "read-only validation",
        "audit only",
        "review only",
    ] {
        m.insert(phrase, vec!["target_write"]);
    }
    for phrase in &[
        "without any provider calls",
        "without provider calls",
        "without model calls",
        "without any model calls",
        "no provider calls",
        "no model calls",
        "do not call providers",
        "do not call any providers",
    ] {
        m.insert(phrase, vec!["provider_call"]);
    }
    m.insert("no api key", vec!["secret_handling"]);
    m.insert("no credentials", vec!["secret_handling"]);
    for phrase in &[
        "without any sandbox execution",
        "without sandbox execution",
        "without executing commands",
        "no sandbox execution",
        "no sandbox",
        "do not run sandbox",
        "no container",
        "no worker",
        "no autonomous workers",
    ] {
        m.insert(phrase, vec!["sandbox_execution"]);
    }
    m
});

static DOMAIN_KEYWORDS: &[(&str, &[&str])] = &[
    (
        "code",
        &[
            "function",
            "class",
            "module",
            "implement",
            "refactor",
            "debug",
            "bug",
            "fix",
            "endpoint",
            "method",
            "variable",
            "auth.py",
            "test_auth",
        ],
    ),
    (
        "docs",
        &[
            "document",
            "documentation",
            "readme",
            "docs",
            "guide",
            "tutorial",
            "write docs",
            "update docs",
        ],
    ),
    (
        "config",
        &[
            "config",
            "configuration",
            "settings",
            "yaml",
            "toml",
            "env",
            "environment",
            ".env",
            "ci/cd",
            "pipeline config",
        ],
    ),
    (
        "infra",
        &[
            "infrastructure",
            "deploy",
            "deployment",
            "docker",
            "kubernetes",
            "k8s",
            "terraform",
            "aws",
            "cloud",
            "server",
            "container",
        ],
    ),
    (
        "math",
        &[
            "calculate",
            "compute",
            "formula",
            "equation",
            "algorithm",
            "batch size",
            "mathematical",
            "optimal batch",
        ],
    ),
    (
        "architecture",
        &[
            "architecture",
            "system design",
            "microservice",
            "architectural",
            "high-level design",
            "component design",
        ],
    ),
    (
        "repo_ops",
        &[
            "commit",
            "push",
            "merge",
            "branch",
            "pull request",
            "pr",
            "git",
            "repository",
            "repo",
            "clone",
            "fork",
        ],
    ),
    (
        "governance",
        &[
            "governance",
            "compliance",
            "audit",
            "policy",
            "security audit",
            "vulnerability",
            "security review",
            "security",
        ],
    ),
];

static INTENT_KEYWORDS: &[(&str, &[&str])] = &[
    (
        "generate",
        &[
            "generate",
            "create",
            "build",
            "implement",
            "add",
            "new",
            "scaffold",
            "produce",
        ],
    ),
    (
        "review",
        &["review", "look at", "analyze for issues", "examine"],
    ),
    (
        "debug",
        &[
            "debug",
            "fix",
            "bug",
            "error",
            "failing",
            "broken",
            "troubleshoot",
            "diagnose",
        ],
    ),
    (
        "summarize",
        &[
            "summarize",
            "summary",
            "overview",
            "tldr",
            "short version",
            "condensed",
        ],
    ),
    (
        "audit",
        &[
            "audit",
            "compliance",
            "vulnerability",
            "scan",
            "penetration",
            "security audit",
            "security review",
        ],
    ),
    (
        "plan",
        &[
            "plan",
            "strategy",
            "roadmap",
            "approach",
            "design plan",
            "implementation plan",
        ],
    ),
    (
        "refactor",
        &[
            "refactor",
            "restructure",
            "reorganize",
            "clean up",
            "improve code",
            "code quality",
        ],
    ),
    (
        "compare",
        &[
            "compare",
            "contrast",
            "versus",
            "vs",
            "difference",
            "trade-off",
        ],
    ),
    (
        "explain",
        &[
            "explain",
            "describe",
            "how does",
            "what is",
            "clarify",
            "elaborate",
        ],
    ),
    (
        "classify",
        &["classify", "categorize", "sort", "group", "label", "tag"],
    ),
];

static RISK_KEYWORDS: &[(&str, &[&str])] = &[
    (
        "target_write",
        &[
            "write",
            "modify",
            "edit",
            "commit",
            "push",
            "merge",
            "delete",
            "remove",
            "create file",
            "fix and commit",
        ],
    ),
    (
        "provider_call",
        &[
            "call openai",
            "call anthropic",
            "openai api",
            "anthropic api",
            "provider call",
            "model call",
            "llm call",
            "gpt api",
            "claude api",
        ],
    ),
    (
        "sandbox_execution",
        &["sandbox", "container", "docker run", "shell command"],
    ),
    (
        "deployment",
        &[
            "deploy",
            "release",
            "publish",
            "production",
            "staging",
            "ship",
        ],
    ),
    (
        "secret_handling",
        &[
            "secret",
            "api key",
            "credential",
            "password",
            "rotate key",
            "rotate the api",
        ],
    ),
    (
        "destructive_operation",
        &[
            "delete",
            "drop table",
            "destroy",
            "wipe",
            "purge",
            "truncate",
            "rm -rf",
        ],
    ),
    (
        "long_context",
        &[
            "500-file",
            "large codebase",
            "entire repo",
            "all files",
            "full codebase",
            "massive",
            "huge",
        ],
    ),
    (
        "high_uncertainty",
        &[
            "unclear",
            "ambiguous",
            "not sure",
            "maybe",
            "might",
            "possibly",
            "make it better",
        ],
    ),
];

static BUDGET_BASE: &[(&str, i64)] = &[
    ("code", 3000),
    ("docs", 2000),
    ("config", 1500),
    ("infra", 2500),
    ("math", 2000),
    ("architecture", 3500),
    ("repo_ops", 1500),
    ("governance", 2000),
    ("other", 2000),
];

static INTENT_MULTIPLIER: &[(&str, f64)] = &[
    ("generate", 1.5),
    ("review", 1.0),
    ("debug", 1.3),
    ("summarize", 0.7),
    ("audit", 1.2),
    ("plan", 1.4),
    ("refactor", 1.3),
    ("compare", 1.1),
    ("explain", 0.9),
    ("classify", 0.8),
];

static NEGATION_PREFIXES: &[&str] = &[
    "without any ",
    "without ",
    "no ",
    "do not ",
    "don't ",
    "never ",
    "cannot ",
    "can't ",
    "must not ",
    "shall not ",
];

// ---------------------------------------------------------------------------
// RuleBasedTaskAnalyzer
// ---------------------------------------------------------------------------

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
        let positive_text = positive_risk_text(&text);

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
        let risk_level = self.derive_risk_level(&risk_flags, domain, intent);
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

    // ------------------------------------------------------------------
    // classify_domain
    // ------------------------------------------------------------------

    fn classify_domain(&self, text: &str) -> &str {
        // Priority-based checks
        if text.contains("architecture")
            || text.contains("microservice")
            || text.contains("system design")
        {
            return "architecture";
        }
        if text.contains("calculate") || text.contains("batch size") || text.contains("formula") {
            return "math";
        }
        if [".py", ".js", ".ts", ".go", ".rs", ".java", "test_auth"]
            .iter()
            .any(|kw| text.contains(kw))
        {
            return "code";
        }
        if ["bug", "fix", "debug", "function", "class", "module"]
            .iter()
            .any(|kw| text.contains(kw))
        {
            return "code";
        }
        if ["readme", "documentation", "docs", "document"]
            .iter()
            .any(|kw| text.contains(kw))
        {
            return "docs";
        }
        if [
            "config",
            "configuration",
            "settings",
            "ci/cd",
            "yaml",
            ".env",
        ]
        .iter()
        .any(|kw| text.contains(kw))
        {
            return "config";
        }
        if [
            "deploy",
            "docker",
            "kubernetes",
            "k8s",
            "terraform",
            "infrastructure",
            "deployment",
        ]
        .iter()
        .any(|kw| text.contains(kw))
        {
            return "infra";
        }
        if ["commit", "push", "merge", "branch", "git", "repo"]
            .iter()
            .any(|kw| text.contains(kw))
        {
            return "repo_ops";
        }
        if [
            "audit",
            "compliance",
            "governance",
            "vulnerability",
            "security",
        ]
        .iter()
        .any(|kw| text.contains(kw))
        {
            return "governance";
        }

        // Fallback: score-based
        let mut best = "other";
        let mut best_score = 0;
        for (domain, keywords) in DOMAIN_KEYWORDS {
            let score = keywords.iter().filter(|kw| text.contains(**kw)).count();
            if score > best_score {
                best = domain;
                best_score = score;
            }
        }
        best
    }

    // ------------------------------------------------------------------
    // classify_intent
    // ------------------------------------------------------------------

    fn classify_intent(&self, text: &str) -> &str {
        if text.contains("summarize") || text.contains("summary") {
            return "summarize";
        }
        if text.contains("audit") {
            return "audit";
        }
        if text.contains("debug")
            || (text.contains("fix") && (text.contains("bug") || text.contains("failing")))
        {
            return "debug";
        }
        if text.contains("generate") || text.contains("create") || text.contains("build") {
            return "generate";
        }
        if text.contains("plan") || text.contains("strategy") || text.contains("roadmap") {
            return "plan";
        }
        if text.contains("review") || text.contains("inspect") || text.contains("examine") {
            return "review";
        }
        if text.contains("refactor") || text.contains("restructure") {
            return "refactor";
        }
        if text.contains("explain") || text.contains("describe") || text.contains("how does") {
            return "explain";
        }
        if text.contains("compare") || text.contains("versus") || text.contains("contrast") {
            return "compare";
        }

        // Fallback: score-based
        let mut best = "classify";
        let mut best_score = 0;
        for (intent, keywords) in INTENT_KEYWORDS {
            let score = keywords.iter().filter(|kw| text.contains(**kw)).count();
            if score > best_score {
                best = intent;
                best_score = score;
            }
        }
        best
    }

    // ------------------------------------------------------------------
    // detect_risk_flags
    // ------------------------------------------------------------------

    fn detect_risk_flags(
        &self,
        text: &str,
        positive_text: &str,
    ) -> (Vec<&str>, Vec<Evidence>, Vec<Evidence>) {
        let mut flags = Vec::new();
        let mut positive_evidence = Vec::new();
        let mut negative_evidence = Vec::new();

        let mut negated_flags_set: Vec<&str> = Vec::new();
        for phrase in NEGATED_RISK_PHRASES {
            if text.contains(phrase) {
                if let Some(flag_list) = PHRASE_FLAGS.get(phrase) {
                    for flag in flag_list {
                        if !negated_flags_set.contains(flag) {
                            negated_flags_set.push(flag);
                        }
                    }
                }
            }
        }

        for (flag, keywords) in RISK_KEYWORDS {
            let mut detected = false;
            let mut matched_keyword = "";
            let mut matched_index = 0usize;
            for keyword in *keywords {
                if let Some(idx) = positive_text.find(keyword) {
                    let original_idx = text.find(keyword).unwrap_or(idx);
                    if !is_negated_occurrence(text, keyword, original_idx) {
                        detected = true;
                        matched_keyword = keyword;
                        matched_index = idx;
                        break;
                    }
                }
            }

            if detected {
                flags.push(*flag);
                positive_evidence.push(Evidence {
                    feature: flag.to_string(),
                    text: matched_keyword.to_string(),
                    span: [
                        matched_index as i64,
                        (matched_index + matched_keyword.len()) as i64,
                    ],
                    polarity: "positive".to_string(),
                    source: "raw_request".to_string(),
                    rule_id: Some(format!("risk_{}", flag)),
                    confidence: 0.9,
                    negation_scope: None,
                });
            } else if negated_flags_set.contains(flag) {
                negative_evidence.push(Evidence {
                    feature: flag.to_string(),
                    text: "[negated]".to_string(),
                    span: [0, 0],
                    polarity: "negative".to_string(),
                    source: "raw_request".to_string(),
                    rule_id: Some(format!("negation_{}", flag)),
                    confidence: 0.95,
                    negation_scope: Some(format!("negation phrase suppressed {}", flag)),
                });
            }
        }

        (flags, positive_evidence, negative_evidence)
    }

    // ------------------------------------------------------------------
    // compute_complexity
    // ------------------------------------------------------------------

    fn compute_complexity(
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

    // ------------------------------------------------------------------
    // estimate_budgets
    // ------------------------------------------------------------------

    fn estimate_budgets(&self, domain: &str, intent: &str, text: &str) -> (i64, i64) {
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

    // ------------------------------------------------------------------
    // assess_confidence
    // ------------------------------------------------------------------

    fn assess_confidence(
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

    // ------------------------------------------------------------------
    // derive_risk_level
    // ------------------------------------------------------------------

    fn derive_risk_level(&self, risk_flags: &[&str], domain: &str, intent: &str) -> &str {
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

    // ------------------------------------------------------------------
    // derive_quality_requirement
    // ------------------------------------------------------------------

    fn derive_quality_requirement(&self, text: &str, risk_level: &str) -> &str {
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

    // ------------------------------------------------------------------
    // determine_safe_default
    // ------------------------------------------------------------------

    fn determine_safe_default(&self, confidence: f64, risk_level: &str) -> &str {
        if confidence < 0.4 {
            return "escalate_to_human";
        }
        if ["critical", "high"].contains(&risk_level) {
            return "noop_with_review";
        }
        "proceed_with_caution"
    }

    // ------------------------------------------------------------------
    // determine_escalation
    // ------------------------------------------------------------------

    fn determine_escalation(
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

    // ------------------------------------------------------------------
    // detect_capabilities
    // ------------------------------------------------------------------

    fn detect_capabilities(&self, text: &str, domain: &str, intent: &str) -> Vec<&str> {
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

    // ------------------------------------------------------------------
    // detect_features
    // ------------------------------------------------------------------

    fn detect_features(
        &self,
        text: &str,
        domain: &str,
        intent: &str,
        risk_flags: &[&str],
    ) -> Value {
        let has_file_refs = [".py", ".js", ".ts", ".yaml", ".json"]
            .iter()
            .any(|kw| text.contains(kw));
        json!({
            "domain": domain,
            "intent": intent,
            "has_code_blocks": text.contains("```"),
            "has_file_refs": has_file_refs,
            "risk_flag_count": risk_flags.len(),
            "word_count": text.split_whitespace().count()
        })
    }
}

pub fn analyze(raw_request: &str, request_source: &str, runtime: &mut FixtureRuntime) -> Value {
    RuleBasedTaskAnalyzer::new()
        .analyze_with_runtime(raw_request, request_source, runtime)
        .to_value()
}

// ---------------------------------------------------------------------------
// Module-level helpers
// ---------------------------------------------------------------------------

fn positive_risk_text(text: &str) -> String {
    let mut result = text.to_string();
    for phrase in NEGATED_RISK_PHRASES {
        result = result.replace(phrase, " ");
    }
    result
}

fn is_negated_occurrence(text: &str, _keyword: &str, start: usize) -> bool {
    let clause_start = start.saturating_sub(40);
    let clause = &text[clause_start..start];
    NEGATION_PREFIXES
        .iter()
        .any(|prefix| clause.contains(prefix))
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}
