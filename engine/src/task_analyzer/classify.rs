use super::rules::{DOMAIN_KEYWORDS, INTENT_KEYWORDS};
use super::RuleBasedTaskAnalyzer;

impl RuleBasedTaskAnalyzer {
    pub(super) fn classify_domain(&self, text: &str) -> &str {
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

    pub(super) fn classify_intent(&self, text: &str) -> &str {
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
}
