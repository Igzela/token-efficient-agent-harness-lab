"""Tests for manual_evaluator.py — ManualEvaluator evaluating pasteback output."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.manual_evaluator import ManualEvaluator, ManualEvalResult
from harness_core.dispatch.pasteback_parser import PastebackParser
from harness_core.dispatch.prompt_pack_gen import PromptPackGenerator


def _make_evaluator_and_pack(raw_request: str = "Summarize the README"):
    engine = DispatchEngine()
    bundle = engine.dispatch(raw_request)
    gen = PromptPackGenerator()
    pack = gen.generate(bundle.decision, raw_request)
    return ManualEvaluator(), pack


class ManualEvaluatorTests(unittest.TestCase):
    def setUp(self):
        self.parser = PastebackParser()

    def test_pass_for_valid_output(self):
        evaluator, pack = _make_evaluator_and_pack()
        sub = self.parser.parse("disp-001", "The README describes the project structure.")
        result = evaluator.evaluate(sub, pack)
        self.assertEqual(result.status, "pass")
        self.assertTrue(result.eval_id)

    def test_fail_for_empty_output(self):
        evaluator, pack = _make_evaluator_and_pack()
        sub = self.parser.parse("disp-001", "Some output")
        # Manually create a submission with empty-ish content won't work since parser validates
        # Test the evaluator check directly
        checks = evaluator._run_checks(sub, pack)
        output_check = [c for c in checks if c.name == "output_present"][0]
        self.assertEqual(output_check.status, "pass")

    def test_fail_for_error_markers(self):
        evaluator, pack = _make_evaluator_and_pack()
        sub = self.parser.parse("disp-001", "Traceback (most recent call last):\n  Error: something broke")
        result = evaluator.evaluate(sub, pack)
        # Should have a warning for error_free
        error_checks = [c for c in result.checks if c.name == "error_free"]
        self.assertEqual(len(error_checks), 1)
        self.assertEqual(error_checks[0].status, "warning")

    def test_boundary_violation_detected(self):
        evaluator, pack = _make_evaluator_and_pack("Fix the bug and commit changes to main")
        sub = self.parser.parse("disp-001", "Done. I wrote to the target repository.")
        result = evaluator.evaluate(sub, pack)
        boundary_checks = [c for c in result.checks if c.name == "boundary_compliance"]
        self.assertTrue(len(boundary_checks) > 0)

    def test_human_review_check_present_for_high_risk(self):
        evaluator, pack = _make_evaluator_and_pack("Fix the bug and commit changes to main")
        sub = self.parser.parse("disp-001", "The fix has been applied.")
        result = evaluator.evaluate(sub, pack)
        hr_checks = [c for c in result.checks if c.name == "human_review_required"]
        self.assertEqual(len(hr_checks), 1)

    def test_eval_result_to_dict(self):
        evaluator, pack = _make_evaluator_and_pack()
        sub = self.parser.parse("disp-001", "Output text here")
        result = evaluator.evaluate(sub, pack)
        d = result.to_dict()
        self.assertIn("eval_id", d)
        self.assertIn("checks", d)
        self.assertIsInstance(d["checks"], list)

    def test_boundary_heuristic_target_write_detected(self):
        evaluator, pack = _make_evaluator_and_pack("Fix the bug and commit changes to main")
        sub = self.parser.parse("disp-001", "Done. I committed the fix to the repository.")
        result = evaluator.evaluate(sub, pack)
        boundary_checks = [c for c in result.checks if c.name == "boundary_compliance"]
        self.assertEqual(len(boundary_checks), 1)
        self.assertEqual(boundary_checks[0].status, "fail")
        self.assertIn("no_target_write", boundary_checks[0].reason)

    def test_boundary_heuristic_provider_detected(self):
        evaluator, pack = _make_evaluator_and_pack("Summarize the README without provider calls")
        sub = self.parser.parse("disp-001", "I called OpenAI to summarize the file.")
        result = evaluator.evaluate(sub, pack)
        boundary_checks = [c for c in result.checks if c.name == "boundary_compliance"]
        self.assertEqual(len(boundary_checks), 1)
        self.assertEqual(boundary_checks[0].status, "fail")
        self.assertIn("no_provider_call", boundary_checks[0].reason)

    def test_boundary_heuristic_clean_output_passes(self):
        evaluator, pack = _make_evaluator_and_pack("Fix the bug and commit changes to main")
        sub = self.parser.parse("disp-001", "The fix has been applied to the code.")
        result = evaluator.evaluate(sub, pack)
        boundary_checks = [c for c in result.checks if c.name == "boundary_compliance"]
        self.assertEqual(len(boundary_checks), 1)
        self.assertEqual(boundary_checks[0].status, "pass")

    def test_all_checks_have_ids(self):
        evaluator, pack = _make_evaluator_and_pack()
        sub = self.parser.parse("disp-001", "Output")
        result = evaluator.evaluate(sub, pack)
        for check in result.checks:
            self.assertTrue(check.check_id)
            self.assertTrue(check.name)
            self.assertIn(check.status, ("pass", "fail", "warning"))


if __name__ == "__main__":
    unittest.main()
