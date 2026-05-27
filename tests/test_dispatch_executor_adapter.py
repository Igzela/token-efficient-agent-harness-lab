"""Tests for executor_adapter.py."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_decision import BudgetReservation, DispatchDecision
from harness_core.dispatch.executor_adapter import (
    ExecutionResult,
    ManualExecutor,
    MockExecutor,
    NoopExecutor,
)


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


class NoopExecutorTests(unittest.TestCase):
    def test_returns_not_executed(self):
        executor = NoopExecutor()
        result = executor.execute(make_decision(), "test request", "disp-001")
        self.assertEqual(result.status, "not_executed")
        self.assertEqual(result.executor_type, "noop")
        self.assertIsNone(result.output)

    def test_result_has_ids(self):
        result = NoopExecutor().execute(make_decision(), "req", "disp-001")
        self.assertTrue(result.result_id)
        self.assertEqual(result.dispatch_id, "disp-001")
        self.assertEqual(result.decision_id, "dec-001")

    def test_to_dict(self):
        result = NoopExecutor().execute(make_decision(), "req", "disp-001")
        d = result.to_dict()
        self.assertIn("result_id", d)
        self.assertIn("status", d)


class MockExecutorTests(unittest.TestCase):
    def test_returns_mock_completed(self):
        executor = MockExecutor()
        result = executor.execute(make_decision(), "test request", "disp-001")
        self.assertEqual(result.status, "mock_completed")
        self.assertEqual(result.executor_type, "mock")

    def test_has_output(self):
        result = MockExecutor().execute(make_decision(), "test request", "disp-001")
        self.assertIsNotNone(result.output)
        self.assertIn("mock", result.output)

    def test_has_token_estimates(self):
        result = MockExecutor().execute(make_decision(), "test request", "disp-001")
        self.assertIsNotNone(result.input_tokens)
        self.assertIsNotNone(result.output_tokens)

    def test_deterministic(self):
        d = make_decision()
        r1 = MockExecutor().execute(d, "same input", "disp-001")
        r2 = MockExecutor().execute(d, "same input", "disp-002")
        self.assertEqual(r1.output, r2.output)


class ManualExecutorTests(unittest.TestCase):
    def test_returns_manual_pending(self):
        result = ManualExecutor().execute(make_decision(), "test", "disp-001")
        self.assertEqual(result.status, "manual_pending")
        self.assertEqual(result.executor_type, "manual")

    def test_has_prompt_pack(self):
        result = ManualExecutor().execute(make_decision(), "test", "disp-001")
        self.assertIsNotNone(result.prompt_pack)
        self.assertIn("recommended_model_tier", result.prompt_pack)
        self.assertIn("budget_limit", result.prompt_pack)
        self.assertIn("evaluation_checklist", result.prompt_pack)

    def test_no_output(self):
        result = ManualExecutor().execute(make_decision(), "test", "disp-001")
        self.assertIsNone(result.output)


if __name__ == "__main__":
    unittest.main()
