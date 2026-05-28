"""Tests for evaluation_stub.py."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_decision import BudgetReservation, DispatchDecision
from harness_core.dispatch.evaluation_stub import EVAL_CHECK_NAMES, EvaluationStub
from harness_core.dispatch.executor_adapter import ExecutionResult


def make_execution_result(**overrides):
    defaults = dict(
        result_id="exec-001",
        dispatch_id="disp-001",
        decision_id="dec-001",
        executor_type="noop",
        status="not_executed",
        created_at="2026-01-01T00:00:00Z",
    )
    defaults.update(overrides)
    return ExecutionResult(**defaults)


def make_decision(**overrides):
    defaults = dict(
        decision_id="dec-001",
        analysis_id="a-001",
        analysis_snapshot={},
        selected_tier="balanced_worker",
        fallback_tier="cheap_executor",
        routing_reason="test",
        quality_requirement="standard",
        expected_quality_band="medium",
        confidence=0.8,
        confidence_label="high",
        budget_reservation=BudgetReservation(
            reservation_id="r-001", decision_id="dec-001", currency="token",
            pre_budget=5000, reserved_input_tokens=3000, reserved_output_tokens=2000,
            reserved_total_tokens=5000, reserved_cost=0.05, status="reserved",
            created_at="2026-01-01T00:00:00Z", updated_at="2026-01-01T00:00:00Z",
        ),
        execution_policy={"executor_type": "noop", "execution_allowed": True, "requires_human_review": False, "max_retries": 0},
        decision_status="decided",
        created_at="2026-01-01T00:00:00Z",
    )
    defaults.update(overrides)
    return DispatchDecision(**defaults)


class EvaluationStubTests(unittest.TestCase):
    def setUp(self):
        self.evaluator = EvaluationStub()

    def test_all_five_checks_present(self):
        result = self.evaluator.evaluate(make_execution_result(), make_decision())
        check_names = {c.name for c in result.checks}
        for name in EVAL_CHECK_NAMES:
            self.assertIn(name, check_names)

    def test_noop_output_present_is_warning(self):
        result = self.evaluator.evaluate(make_execution_result(), make_decision())
        output_check = next(c for c in result.checks if c.name == "output_present")
        self.assertEqual(output_check.status, "warning")

    def test_mock_output_present_is_pass(self):
        er = make_execution_result(executor_type="mock", status="mock_completed", output="mock output")
        result = self.evaluator.evaluate(er, make_decision())
        output_check = next(c for c in result.checks if c.name == "output_present")
        self.assertEqual(output_check.status, "pass")

    def test_boundary_compliance_noop(self):
        result = self.evaluator.evaluate(make_execution_result(), make_decision())
        bc = next(c for c in result.checks if c.name == "boundary_compliance")
        self.assertEqual(bc.status, "pass")

    def test_boundary_compliance_provider_passes_in_phase3(self):
        er = make_execution_result(executor_type="provider")
        result = self.evaluator.evaluate(er, make_decision())
        bc = next(c for c in result.checks if c.name == "boundary_compliance")
        self.assertEqual(bc.status, "pass")

    def test_error_free_no_error(self):
        result = self.evaluator.evaluate(make_execution_result(), make_decision())
        ef = next(c for c in result.checks if c.name == "error_free")
        self.assertEqual(ef.status, "pass")

    def test_error_free_with_error(self):
        er = make_execution_result(error_domain="execution", error_message="something broke")
        result = self.evaluator.evaluate(er, make_decision())
        ef = next(c for c in result.checks if c.name == "error_free")
        self.assertEqual(ef.status, "fail")

    def test_human_review_required(self):
        d = make_decision(execution_policy={"executor_type": "noop", "execution_allowed": True, "requires_human_review": True, "max_retries": 0})
        result = self.evaluator.evaluate(make_execution_result(), d)
        hr = next(c for c in result.checks if c.name == "human_review_required")
        self.assertEqual(hr.status, "warning")

    def test_overall_status_pass(self):
        result = self.evaluator.evaluate(make_execution_result(), make_decision())
        self.assertEqual(result.status, "pass")

    def test_overall_status_fail_on_provider(self):
        er = make_execution_result(executor_type="provider")
        result = self.evaluator.evaluate(er, make_decision())
        self.assertEqual(result.status, "fail")

    def test_to_dict(self):
        result = self.evaluator.evaluate(make_execution_result(), make_decision())
        d = result.to_dict()
        self.assertIn("evaluation_id", d)
        self.assertIn("checks", d)
        self.assertIsInstance(d["checks"], list)


if __name__ == "__main__":
    unittest.main()
