"""Tests for dispatch_ledger.py."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_decision import BudgetReservation, DispatchDecision
from harness_core.dispatch.dispatch_ledger import DispatchBundle, DispatchLedger
from harness_core.dispatch.evaluation_stub import EvaluationCheck, EvaluationResult
from harness_core.dispatch.executor_adapter import ExecutionResult
from harness_core.dispatch.task_analyzer import RuleBasedTaskAnalyzer, TaskAnalysis


class DispatchLedgerTests(unittest.TestCase):
    def setUp(self):
        self.ledger = DispatchLedger()

    def test_create_record(self):
        r = self.ledger.create_record("disp-001", "test request", "a-001", "dec-001")
        self.assertEqual(r.dispatch_id, "disp-001")
        self.assertEqual(r.final_status, "dispatched")

    def test_get_record(self):
        self.ledger.create_record("disp-001", "req", "a-001", "dec-001")
        r = self.ledger.get_record("disp-001")
        self.assertIsNotNone(r)
        self.assertEqual(r.dispatch_id, "disp-001")

    def test_get_nonexistent_record(self):
        r = self.ledger.get_record("nonexistent")
        self.assertIsNone(r)

    def test_list_records(self):
        self.ledger.create_record("disp-001", "req1", "a-001", "dec-001")
        self.ledger.create_record("disp-002", "req2", "a-002", "dec-002")
        records = self.ledger.list_records()
        self.assertEqual(len(records), 2)

    def test_update_record(self):
        r = self.ledger.create_record("disp-001", "req", "a-001", "dec-001")
        updated = self.ledger.update_record(r, final_status="completed", execution_result_id="exec-001")
        self.assertEqual(updated.final_status, "completed")
        self.assertEqual(updated.execution_result_id, "exec-001")
        self.assertNotEqual(updated.updated_at, r.created_at)

    def test_update_persists(self):
        r = self.ledger.create_record("disp-001", "req", "a-001", "dec-001")
        self.ledger.update_record(r, final_status="completed")
        stored = self.ledger.get_record("disp-001")
        self.assertEqual(stored.final_status, "completed")

    def test_replay(self):
        self.ledger.create_record("disp-001", "req", "a-001", "dec-001")
        r = self.ledger.replay("disp-001")
        self.assertIsNotNone(r)
        self.assertEqual(r.dispatch_id, "disp-001")

    def test_to_dict(self):
        r = self.ledger.create_record("disp-001", "req", "a-001", "dec-001")
        d = r.to_dict()
        self.assertIn("dispatch_id", d)
        self.assertIn("final_status", d)
        self.assertIn("schema_version", d)

    def test_create_with_budget_reservation(self):
        r = self.ledger.create_record("disp-001", "req", "a-001", "dec-001",
                                       budget_reservation_id="bres-001")
        self.assertEqual(r.budget_reservation_id, "bres-001")

    def test_update_with_usage_ledger_row(self):
        r = self.ledger.create_record("disp-001", "req", "a-001", "dec-001")
        updated = self.ledger.update_record(r, usage_ledger_row_id="ulr-001")
        self.assertEqual(updated.usage_ledger_row_id, "ulr-001")


def _make_analysis():
    return RuleBasedTaskAnalyzer().analyze("Review auth.py for security issues")


def _make_decision():
    return DispatchDecision(
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


def _make_execution_result():
    return ExecutionResult(
        result_id="exec-001",
        dispatch_id="disp-001",
        decision_id="dec-001",
        executor_type="noop",
        status="completed",
        output="test output",
        created_at="2026-01-01T00:00:00Z",
    )


def _make_evaluation_result():
    return EvaluationResult(
        evaluation_id="eval-001",
        dispatch_id="disp-001",
        decision_id="dec-001",
        execution_result_id="exec-001",
        status="pass",
        checks=(
            EvaluationCheck(check_id="c1", name="schema_ok", status="pass", reason="ok"),
        ),
        created_at="2026-01-01T00:00:00Z",
    )


class DispatchBundleTests(unittest.TestCase):
    def setUp(self):
        self.ledger = DispatchLedger()
        self.record = self.ledger.create_record("disp-001", "req", "a-001", "dec-001")

    def test_store_bundle(self):
        bundle = self.ledger.store_bundle(
            self.record, _make_analysis(), _make_decision(),
            _make_execution_result(), _make_evaluation_result(),
        )
        self.assertIsInstance(bundle, DispatchBundle)
        self.assertEqual(bundle.record.dispatch_id, "disp-001")

    def test_get_bundle(self):
        self.ledger.store_bundle(
            self.record, _make_analysis(), _make_decision(),
            _make_execution_result(), _make_evaluation_result(),
        )
        bundle = self.ledger.get_bundle("disp-001")
        self.assertIsNotNone(bundle)
        self.assertEqual(bundle.record.dispatch_id, "disp-001")

    def test_get_nonexistent_bundle(self):
        self.assertIsNone(self.ledger.get_bundle("missing"))

    def test_list_bundles(self):
        self.ledger.store_bundle(
            self.record, _make_analysis(), _make_decision(),
            _make_execution_result(), _make_evaluation_result(),
        )
        bundles = self.ledger.list_bundles()
        self.assertEqual(len(bundles), 1)

    def test_replay_returns_bundle_when_stored(self):
        self.ledger.store_bundle(
            self.record, _make_analysis(), _make_decision(),
            _make_execution_result(), _make_evaluation_result(),
        )
        result = self.ledger.replay("disp-001")
        self.assertIsInstance(result, DispatchBundle)

    def test_replay_returns_record_when_no_bundle(self):
        result = self.ledger.replay("disp-001")
        self.assertIsInstance(result, type(self.record))
        self.assertNotIsInstance(result, DispatchBundle)

    def test_bundle_to_dict(self):
        bundle = self.ledger.store_bundle(
            self.record, _make_analysis(), _make_decision(),
            _make_execution_result(), _make_evaluation_result(),
        )
        d = bundle.to_dict()
        self.assertIn("record", d)
        self.assertIn("analysis", d)
        self.assertIn("decision", d)
        self.assertIn("execution_result", d)
        self.assertIn("evaluation_result", d)
