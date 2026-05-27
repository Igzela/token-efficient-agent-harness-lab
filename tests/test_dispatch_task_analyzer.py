"""Tests for task_analyzer.py — RuleBasedTaskAnalyzer."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.task_analyzer import RuleBasedTaskAnalyzer, TaskAnalysis

FIXTURES_DIR = Path(__file__).resolve().parent / "fixtures" / "dispatch"


class DomainClassificationTests(unittest.TestCase):
    def setUp(self):
        self.analyzer = RuleBasedTaskAnalyzer()

    def test_code_domain(self):
        a = self.analyzer.analyze("Review auth.py for security issues")
        self.assertEqual(a.task_domain, "code")

    def test_docs_domain(self):
        a = self.analyzer.analyze("Summarize the README file")
        self.assertEqual(a.task_domain, "docs")

    def test_config_domain(self):
        a = self.analyzer.analyze("Review CI/CD configuration")
        self.assertEqual(a.task_domain, "config")

    def test_infra_domain(self):
        a = self.analyzer.analyze("Review infrastructure deployment pipeline")
        self.assertEqual(a.task_domain, "infra")

    def test_architecture_domain(self):
        a = self.analyzer.analyze("Design the architecture for a microservice")
        self.assertEqual(a.task_domain, "architecture")

    def test_math_domain(self):
        a = self.analyzer.analyze("Calculate the optimal batch size")
        self.assertEqual(a.task_domain, "math")

    def test_governance_domain(self):
        a = self.analyzer.analyze("Audit the database schema for compliance")
        self.assertEqual(a.task_domain, "governance")

    def test_other_domain_fallback(self):
        a = self.analyzer.analyze("Make it better")
        self.assertEqual(a.task_domain, "other")


class IntentClassificationTests(unittest.TestCase):
    def setUp(self):
        self.analyzer = RuleBasedTaskAnalyzer()

    def test_review_intent(self):
        a = self.analyzer.analyze("Review auth.py for security issues")
        self.assertEqual(a.task_intent, "review")

    def test_summarize_intent(self):
        a = self.analyzer.analyze("Summarize the README")
        self.assertEqual(a.task_intent, "summarize")

    def test_generate_intent(self):
        a = self.analyzer.analyze("Generate a CLI tool for validation")
        self.assertEqual(a.task_intent, "generate")

    def test_debug_intent(self):
        a = self.analyzer.analyze("Debug the failing test")
        self.assertEqual(a.task_intent, "debug")

    def test_audit_intent(self):
        a = self.analyzer.analyze("Audit the schema for vulnerabilities")
        self.assertEqual(a.task_intent, "audit")

    def test_plan_intent(self):
        a = self.analyzer.analyze("Plan the architecture for a new service")
        self.assertEqual(a.task_intent, "plan")

    def test_classify_fallback(self):
        a = self.analyzer.analyze("Make it better")
        self.assertEqual(a.task_intent, "classify")


class RiskFlagDetectionTests(unittest.TestCase):
    def setUp(self):
        self.analyzer = RuleBasedTaskAnalyzer()

    def test_target_write_detected(self):
        a = self.analyzer.analyze("Fix the bug and commit the changes")
        self.assertIn("target_write", a.risk_flags)

    def test_provider_call_detected(self):
        a = self.analyzer.analyze("Call OpenAI API to analyze")
        self.assertIn("provider_call", a.risk_flags)

    def test_secret_handling_detected(self):
        a = self.analyzer.analyze("Rotate the API keys in config")
        self.assertIn("secret_handling", a.risk_flags)

    def test_no_risk_for_summary(self):
        a = self.analyzer.analyze("Summarize the README")
        self.assertEqual(len(a.risk_flags), 0)

    def test_negated_no_write(self):
        """Critical invariant: 'no target repo writes' must NOT trigger target_write."""
        a = self.analyzer.analyze("Review code with no target repo writes, read-only validation")
        self.assertNotIn("target_write", a.risk_flags)

    def test_negated_no_execute(self):
        a = self.analyzer.analyze("Analyze config without any provider calls or sandbox execution")
        self.assertNotIn("provider_call", a.risk_flags)
        self.assertNotIn("sandbox_execution", a.risk_flags)

    def test_negation_produces_negative_evidence(self):
        a = self.analyzer.analyze("Review code with no target repo writes")
        self.assertTrue(len(a.negative_evidence) > 0)
        neg = a.negative_evidence[0]
        self.assertEqual(neg.polarity, "negative")
        self.assertIn("target_write", neg.feature or neg.negation_scope or "")


class ComplexityScoringTests(unittest.TestCase):
    def setUp(self):
        self.analyzer = RuleBasedTaskAnalyzer()

    def test_sub_scores_in_range(self):
        a = self.analyzer.analyze("Design the architecture for a new microservice")
        for score in (a.cognitive_complexity, a.context_complexity, a.execution_risk, a.ambiguity_score):
            self.assertGreaterEqual(score, 0.0)
            self.assertLessEqual(score, 1.0)

    def test_complexity_score_weighted(self):
        a = self.analyzer.analyze("Design the architecture for a new microservice")
        expected = (
            0.35 * a.cognitive_complexity
            + 0.25 * a.context_complexity
            + 0.25 * a.execution_risk
            + 0.15 * a.ambiguity_score
        )
        self.assertAlmostEqual(a.complexity_score, round(expected, 4), places=3)

    def test_higher_complexity_for_debug(self):
        simple = self.analyzer.analyze("Summarize the README")
        complex_ = self.analyzer.analyze("Debug the failing test and fix the root cause")
        self.assertGreaterEqual(complex_.cognitive_complexity, simple.cognitive_complexity)


class ConfidenceTests(unittest.TestCase):
    def setUp(self):
        self.analyzer = RuleBasedTaskAnalyzer()

    def test_high_confidence_clear_request(self):
        a = self.analyzer.analyze("Review auth.py for security issues")
        self.assertEqual(a.confidence_label, "high")

    def test_low_confidence_ambiguous(self):
        a = self.analyzer.analyze("Make it better")
        self.assertEqual(a.confidence_label, "low")

    def test_safe_default_escalation_for_low_confidence(self):
        a = self.analyzer.analyze("Make it better")
        self.assertEqual(a.safe_default, "escalate_to_human")


class BudgetEstimationTests(unittest.TestCase):
    def setUp(self):
        self.analyzer = RuleBasedTaskAnalyzer()

    def test_budget_estimates_positive(self):
        a = self.analyzer.analyze("Summarize the README")
        self.assertGreater(a.context_budget_estimate, 0)
        self.assertGreater(a.execution_budget_estimate, 0)

    def test_budget_constrained_request(self):
        a = self.analyzer.analyze("Summarize the docs within 500 tokens budget")
        self.assertLessEqual(a.context_budget_estimate, 500)


class SchemaTests(unittest.TestCase):
    def setUp(self):
        self.analyzer = RuleBasedTaskAnalyzer()

    def test_analysis_method_is_rule_only(self):
        a = self.analyzer.analyze("Test request")
        self.assertEqual(a.analysis_method, "rule_only")

    def test_to_dict_roundtrip(self):
        a = self.analyzer.analyze("Test request for dict conversion")
        d = a.to_dict()
        self.assertIn("analysis_id", d)
        self.assertIn("task_domain", d)
        self.assertIn("positive_evidence", d)
        self.assertIsInstance(d["risk_flags"], list)


class GoldenFixtureTests(unittest.TestCase):
    """Test all 20 golden fixtures pass expected analysis."""

    def setUp(self):
        self.analyzer = RuleBasedTaskAnalyzer()

    def _load_fixture(self, filename):
        path = FIXTURES_DIR / filename
        with open(path) as f:
            return json.load(f)

    def _run_fixture(self, fixture_file):
        fixture = self._load_fixture(fixture_file)
        result = self.analyzer.analyze(
            fixture["raw_request"],
            request_source=fixture.get("request_source", "test_fixture"),
        )
        expected = fixture["expected_analysis"]

        self.assertEqual(result.task_domain, expected["task_domain"], f"domain mismatch in {fixture_file}")
        self.assertEqual(result.task_intent, expected["task_intent"], f"intent mismatch in {fixture_file}")
        self.assertEqual(result.risk_level, expected["risk_level"], f"risk_level mismatch in {fixture_file}")
        self.assertEqual(result.confidence_label, expected["confidence_label"], f"confidence_label mismatch in {fixture_file}")

        for flag in expected.get("risk_flags", []):
            self.assertIn(flag, result.risk_flags, f"expected risk_flag {flag} in {fixture_file}")

        return result

    def test_fixture_01_low_risk_summary(self):
        self._run_fixture("fixture_01_low_risk_summary.json")

    def test_fixture_02_doc_audit(self):
        self._run_fixture("fixture_02_doc_audit.json")

    def test_fixture_03_code_review(self):
        self._run_fixture("fixture_03_code_review.json")

    def test_fixture_04_code_gen(self):
        self._run_fixture("fixture_04_code_gen.json")

    def test_fixture_05_debug(self):
        self._run_fixture("fixture_05_debug.json")

    def test_fixture_06_architecture(self):
        self._run_fixture("fixture_06_architecture.json")

    def test_fixture_07_math(self):
        self._run_fixture("fixture_07_math.json")

    def test_fixture_08_config_review(self):
        self._run_fixture("fixture_08_config_review.json")

    def test_fixture_09_infra_deploy(self):
        self._run_fixture("fixture_09_infra_deploy.json")

    def test_fixture_10_provider_boundary(self):
        self._run_fixture("fixture_10_provider_boundary.json")

    def test_fixture_11_target_write(self):
        self._run_fixture("fixture_11_target_write.json")

    def test_fixture_12_secret_handling(self):
        self._run_fixture("fixture_12_secret_handling.json")

    def test_fixture_13_long_context(self):
        self._run_fixture("fixture_13_long_context.json")

    def test_fixture_14_ambiguous(self):
        self._run_fixture("fixture_14_ambiguous.json")

    def test_fixture_15_conflicting(self):
        self._run_fixture("fixture_15_conflicting.json")

    def test_fixture_16_read_only_high_risk(self):
        self._run_fixture("fixture_16_read_only_high_risk.json")

    def test_fixture_17_negated_no_write(self):
        self._run_fixture("fixture_17_negated_no_write.json")

    def test_fixture_18_negated_no_execute(self):
        self._run_fixture("fixture_18_negated_no_execute.json")

    def test_fixture_19_budget_constrained(self):
        self._run_fixture("fixture_19_budget_constrained.json")

    def test_fixture_20_high_quality_critical(self):
        self._run_fixture("fixture_20_high_quality_critical.json")


if __name__ == "__main__":
    unittest.main()
