use engine::task_analyzer::RuleBasedTaskAnalyzer;

fn analyzer() -> RuleBasedTaskAnalyzer {
    RuleBasedTaskAnalyzer::new()
}

// ---------------------------------------------------------------------------
// DomainClassificationTests
// ---------------------------------------------------------------------------

#[test]
fn test_code_domain() {
    let a = analyzer().analyze("Review auth.py for security issues", "test_fixture");
    assert_eq!(a.task_domain, "code");
}

#[test]
fn test_docs_domain() {
    let a = analyzer().analyze("Summarize the README file", "test_fixture");
    assert_eq!(a.task_domain, "docs");
}

#[test]
fn test_config_domain() {
    let a = analyzer().analyze("Review CI/CD configuration", "test_fixture");
    assert_eq!(a.task_domain, "config");
}

#[test]
fn test_infra_domain() {
    let a = analyzer().analyze("Review infrastructure deployment pipeline", "test_fixture");
    assert_eq!(a.task_domain, "infra");
}

#[test]
fn test_architecture_domain() {
    let a = analyzer().analyze("Design the architecture for a microservice", "test_fixture");
    assert_eq!(a.task_domain, "architecture");
}

#[test]
fn test_math_domain() {
    let a = analyzer().analyze("Calculate the optimal batch size", "test_fixture");
    assert_eq!(a.task_domain, "math");
}

#[test]
fn test_governance_domain() {
    let a = analyzer().analyze("Audit the database schema for compliance", "test_fixture");
    assert_eq!(a.task_domain, "governance");
}

#[test]
fn test_other_domain_fallback() {
    let a = analyzer().analyze("Make it better", "test_fixture");
    assert_eq!(a.task_domain, "other");
}

// ---------------------------------------------------------------------------
// IntentClassificationTests
// ---------------------------------------------------------------------------

#[test]
fn test_review_intent() {
    let a = analyzer().analyze("Review auth.py for security issues", "test_fixture");
    assert_eq!(a.task_intent, "review");
}

#[test]
fn test_summarize_intent() {
    let a = analyzer().analyze("Summarize the README", "test_fixture");
    assert_eq!(a.task_intent, "summarize");
}

#[test]
fn test_generate_intent() {
    let a = analyzer().analyze("Generate a CLI tool for validation", "test_fixture");
    assert_eq!(a.task_intent, "generate");
}

#[test]
fn test_debug_intent() {
    let a = analyzer().analyze("Debug the failing test", "test_fixture");
    assert_eq!(a.task_intent, "debug");
}

#[test]
fn test_audit_intent() {
    let a = analyzer().analyze("Audit the schema for vulnerabilities", "test_fixture");
    assert_eq!(a.task_intent, "audit");
}

#[test]
fn test_plan_intent() {
    let a = analyzer().analyze("Plan the architecture for a new service", "test_fixture");
    assert_eq!(a.task_intent, "plan");
}

#[test]
fn test_classify_fallback() {
    let a = analyzer().analyze("Make it better", "test_fixture");
    assert_eq!(a.task_intent, "classify");
}

// ---------------------------------------------------------------------------
// RiskFlagDetectionTests
// ---------------------------------------------------------------------------

#[test]
fn test_target_write_detected() {
    let a = analyzer().analyze("Fix the bug and commit the changes", "test_fixture");
    assert!(a.risk_flags.contains(&"target_write".to_string()));
}

#[test]
fn test_provider_call_detected() {
    let a = analyzer().analyze("Call OpenAI API to analyze", "test_fixture");
    assert!(a.risk_flags.contains(&"provider_call".to_string()));
}

#[test]
fn test_secret_handling_detected() {
    let a = analyzer().analyze("Rotate the API keys in config", "test_fixture");
    assert!(a.risk_flags.contains(&"secret_handling".to_string()));
}

#[test]
fn test_no_risk_for_summary() {
    let a = analyzer().analyze("Summarize the README", "test_fixture");
    assert!(a.risk_flags.is_empty());
    assert!(a.negative_evidence.is_empty());
}

#[test]
fn test_negated_no_write_only_suppresses_target_write() {
    let a = analyzer().analyze("Review code with no target repo writes", "test_fixture");
    assert!(!a.risk_flags.contains(&"target_write".to_string()));
    assert!(a
        .negative_evidence
        .iter()
        .any(|e| e.feature == "target_write"));
    assert!(!a
        .negative_evidence
        .iter()
        .any(|e| e.feature == "provider_call"));
    assert!(!a
        .negative_evidence
        .iter()
        .any(|e| e.feature == "sandbox_execution"));
}

#[test]
fn test_negated_no_provider_suppresses_provider_only() {
    let a = analyzer().analyze("Review code without any provider calls", "test_fixture");
    assert!(!a.risk_flags.contains(&"provider_call".to_string()));
    assert!(a
        .negative_evidence
        .iter()
        .any(|e| e.feature == "provider_call"));
    assert!(!a
        .negative_evidence
        .iter()
        .any(|e| e.feature == "target_write"));
}

#[test]
fn test_negation_produces_negative_evidence() {
    let a = analyzer().analyze("Review code with no target repo writes", "test_fixture");
    assert!(!a.negative_evidence.is_empty());
    assert_eq!(a.negative_evidence[0].polarity, "negative");
    assert!(a.negative_evidence[0].feature.contains("target_write"));
}

// ---------------------------------------------------------------------------
// ComplexityScoringTests
// ---------------------------------------------------------------------------

#[test]
fn test_sub_scores_in_range() {
    let a = analyzer().analyze("Review auth.py for security issues", "test_fixture");
    assert!(a.cognitive_complexity >= 0.0 && a.cognitive_complexity <= 1.0);
    assert!(a.context_complexity >= 0.0 && a.context_complexity <= 1.0);
    assert!(a.execution_risk >= 0.0 && a.execution_risk <= 1.0);
    assert!(a.ambiguity_score >= 0.0 && a.ambiguity_score <= 1.0);
}

#[test]
fn test_complexity_score_weighted() {
    let a = analyzer().analyze("Review auth.py for security issues", "test_fixture");
    let expected = 0.35 * a.cognitive_complexity
        + 0.25 * a.context_complexity
        + 0.25 * a.execution_risk
        + 0.15 * a.ambiguity_score;
    assert!(
        (a.complexity_score - expected).abs() < 0.001,
        "complexity_score={} expected={}",
        a.complexity_score,
        expected
    );
}

#[test]
fn test_higher_complexity_for_debug() {
    let debug = analyzer().analyze("Debug the failing test in auth.py", "test_fixture");
    let summary = analyzer().analyze("Summarize the README", "test_fixture");
    assert!(debug.cognitive_complexity >= summary.cognitive_complexity);
}

// ---------------------------------------------------------------------------
// ConfidenceTests
// ---------------------------------------------------------------------------

#[test]
fn test_high_confidence_clear_request() {
    let a = analyzer().analyze("Review auth.py for security issues", "test_fixture");
    assert_eq!(a.confidence_label, "high");
}

#[test]
fn test_low_confidence_ambiguous() {
    let a = analyzer().analyze("Make it better", "test_fixture");
    assert_eq!(a.confidence_label, "low");
}

#[test]
fn test_safe_default_escalation_for_low_confidence() {
    let a = analyzer().analyze("Make it better", "test_fixture");
    assert_eq!(a.safe_default, "escalate_to_human");
}

// ---------------------------------------------------------------------------
// BudgetEstimationTests
// ---------------------------------------------------------------------------

#[test]
fn test_budget_estimates_positive() {
    let a = analyzer().analyze("Review auth.py for security issues", "test_fixture");
    assert!(a.context_budget_estimate > 0);
    assert!(a.execution_budget_estimate > 0);
}

#[test]
fn test_budget_constrained_request() {
    let a = analyzer().analyze(
        "Summarize the docs within 500 tokens budget",
        "test_fixture",
    );
    assert!(
        a.context_budget_estimate <= 500,
        "budget was {}",
        a.context_budget_estimate
    );
}

// ---------------------------------------------------------------------------
// SchemaTests
// ---------------------------------------------------------------------------

#[test]
fn test_analysis_method_is_rule_only() {
    let a = analyzer().analyze("Review auth.py", "test_fixture");
    assert_eq!(a.analysis_method, "rule_only");
}

#[test]
fn test_to_dict_roundtrip() {
    let a = analyzer().analyze("Review auth.py", "test_fixture");
    let v = a.to_value();
    assert!(v.get("analysis_id").is_some());
    assert!(v.get("task_domain").is_some());
    assert!(v.get("positive_evidence").is_some());
    assert!(v["risk_flags"].is_array());
}

// ---------------------------------------------------------------------------
// Golden fixture tests
// ---------------------------------------------------------------------------

macro_rules! golden_test {
    ($name:ident, $raw:expr, $domain:expr, $intent:expr, $risk_level:expr, $confidence:expr) => {
        #[test]
        fn $name() {
            let a = analyzer().analyze($raw, "test_fixture");
            assert_eq!(a.task_domain, $domain, "domain mismatch for: {}", $raw);
            assert_eq!(a.task_intent, $intent, "intent mismatch for: {}", $raw);
            assert_eq!(
                a.risk_level, $risk_level,
                "risk_level mismatch for: {}",
                $raw
            );
            assert_eq!(
                a.confidence_label, $confidence,
                "confidence_label mismatch for: {}",
                $raw
            );
        }
    };
}

golden_test!(
    golden_01,
    "Summarize the README file for the project",
    "docs",
    "summarize",
    "low",
    "high"
);
golden_test!(
    golden_02,
    "Audit the documentation for broken links and outdated references",
    "docs",
    "audit",
    "medium",
    "high"
);
golden_test!(
    golden_03,
    "Review auth.py for security issues and potential vulnerabilities",
    "code",
    "review",
    "low",
    "high"
);
golden_test!(
    golden_04,
    "Generate a CLI tool for YAML validation with error reporting",
    "config",
    "generate",
    "low",
    "high"
);
golden_test!(
    golden_05,
    "Debug the failing test in test_auth.py and fix the root cause",
    "code",
    "debug",
    "low",
    "high"
);
golden_test!(
    golden_06,
    "Design the architecture for a new microservice with event-driven communication",
    "architecture",
    "generate",
    "low",
    "high"
);
golden_test!(
    golden_07,
    "Calculate the optimal batch size for cost efficiency given token pricing",
    "math",
    "classify",
    "low",
    "medium"
);
golden_test!(
    golden_08,
    "Review CI/CD configuration for best practices and security",
    "config",
    "review",
    "low",
    "high"
);
golden_test!(
    golden_09,
    "Review infrastructure deployment pipeline for reliability issues",
    "infra",
    "review",
    "critical",
    "high"
);
golden_test!(
    golden_10,
    "Call OpenAI API to analyze the codebase and generate insights",
    "other",
    "generate",
    "high",
    "medium"
);
golden_test!(
    golden_11,
    "Fix the bug and commit the changes to main branch",
    "code",
    "debug",
    "high",
    "high"
);
golden_test!(
    golden_12,
    "Rotate the API keys in the config files and update credentials",
    "config",
    "classify",
    "critical",
    "medium"
);
golden_test!(
    golden_13,
    "Analyze this 500-file large codebase for architectural patterns and anti-patterns",
    "architecture",
    "classify",
    "low",
    "medium"
);
golden_test!(
    golden_14,
    "Make it better",
    "other",
    "classify",
    "low",
    "low"
);
golden_test!(
    golden_15,
    "Minimize cost but use the most powerful model available for this task",
    "other",
    "classify",
    "low",
    "medium"
);
golden_test!(
    golden_16,
    "Audit the database schema for security vulnerabilities and compliance gaps",
    "governance",
    "audit",
    "medium",
    "high"
);
golden_test!(
    golden_17,
    "Review code with no target repo writes, read-only validation only",
    "repo_ops",
    "review",
    "low",
    "high"
);
golden_test!(
    golden_18,
    "Analyze deployment config without any provider calls or sandbox execution",
    "config",
    "classify",
    "critical",
    "medium"
);
golden_test!(
    golden_19,
    "Summarize the docs within 500 tokens budget",
    "docs",
    "summarize",
    "low",
    "high"
);
golden_test!(
    golden_20,
    "Critical security review of authentication system, must be production-grade",
    "governance",
    "review",
    "critical",
    "high"
);
