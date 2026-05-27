"""Tests for dispatch_engine.py — end-to-end, safety invariants, and bundle verification."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.dispatch_ledger import DispatchBundle
from harness_core.dispatch.executor_adapter import ManualExecutor, MockExecutor, NoopExecutor

FIXTURES_DIR = Path(__file__).resolve().parent / "fixtures" / "dispatch"


class DispatchEngineBasicTests(unittest.TestCase):
    def setUp(self):
        self.engine = DispatchEngine()

    def test_dispatch_returns_bundle(self):
        bundle = self.engine.dispatch("Summarize the README")
        self.assertIsInstance(bundle, DispatchBundle)
        self.assertTrue(bundle.record.dispatch_id)
        self.assertEqual(bundle.record.final_status, "not_executed")

    def test_dispatch_with_mock_executor(self):
        engine = DispatchEngine(executor=MockExecutor())
        bundle = engine.dispatch("Summarize the README")
        self.assertEqual(bundle.record.final_status, "completed")
        self.assertEqual(bundle.execution_result.executor_type, "mock")
        self.assertIsNotNone(bundle.execution_result.output)

    def test_dispatch_with_manual_executor(self):
        engine = DispatchEngine(executor=ManualExecutor())
        bundle = engine.dispatch("Summarize the README")
        self.assertEqual(bundle.record.final_status, "manual_pending")
        self.assertIsNotNone(bundle.execution_result.prompt_pack)

    def test_dispatch_populates_record_fields(self):
        bundle = self.engine.dispatch("Test request")
        self.assertTrue(bundle.record.task_analysis_id)
        self.assertTrue(bundle.record.decision_id)
        self.assertTrue(bundle.record.budget_reservation_id)
        self.assertTrue(bundle.record.created_at)
        self.assertTrue(bundle.record.updated_at)

    def test_dispatch_stores_in_ledger(self):
        bundle = self.engine.dispatch("Test request")
        stored = self.engine._ledger.get_record(bundle.record.dispatch_id)
        self.assertIsNotNone(stored)
        self.assertEqual(stored.dispatch_id, bundle.record.dispatch_id)

    def test_dispatch_bundle_contains_full_chain(self):
        engine = DispatchEngine(executor=MockExecutor())
        bundle = engine.dispatch("Summarize the README")
        self.assertIsNotNone(bundle.analysis)
        self.assertIsNotNone(bundle.decision)
        self.assertIsNotNone(bundle.execution_result)
        self.assertIsNotNone(bundle.evaluation_result)
        self.assertEqual(bundle.analysis.analysis_id, bundle.decision.analysis_id)
        self.assertEqual(bundle.execution_result.dispatch_id, bundle.record.dispatch_id)
        self.assertEqual(bundle.evaluation_result.dispatch_id, bundle.record.dispatch_id)

    def test_dispatch_bundle_stored_in_ledger(self):
        engine = DispatchEngine(executor=MockExecutor())
        bundle = engine.dispatch("Test request")
        stored_bundle = engine._ledger.get_bundle(bundle.record.dispatch_id)
        self.assertIsNotNone(stored_bundle)
        self.assertEqual(stored_bundle.record.dispatch_id, bundle.record.dispatch_id)
        self.assertEqual(stored_bundle.analysis.analysis_id, bundle.analysis.analysis_id)

    def test_bundle_to_dict_roundtrip(self):
        engine = DispatchEngine(executor=MockExecutor())
        bundle = engine.dispatch("Test request")
        d = bundle.to_dict()
        self.assertIn("record", d)
        self.assertIn("analysis", d)
        self.assertIn("decision", d)
        self.assertIn("execution_result", d)
        self.assertIn("evaluation_result", d)


class SafetyInvariantTests(unittest.TestCase):
    def setUp(self):
        self.engine = DispatchEngine()

    def test_no_provider_executor_type(self):
        """Phase 1: provider executor must never be used."""
        bundle = self.engine.dispatch("Test request")
        self.assertNotEqual(bundle.execution_result.executor_type, "provider")

    def test_decision_has_shadow_routes_or_reason(self):
        bundle = self.engine.dispatch("Summarize the README")
        d = bundle.decision
        self.assertTrue(d.shadow_routes or d.no_shadow_route_reason)

    def test_budget_exists_before_execution(self):
        bundle = self.engine.dispatch("Test request")
        self.assertIsNotNone(bundle.record.budget_reservation_id)
        self.assertIsNotNone(bundle.decision.budget_reservation)

    def test_execution_result_links_to_record(self):
        engine = DispatchEngine(executor=MockExecutor())
        bundle = engine.dispatch("Test request")
        self.assertEqual(bundle.execution_result.dispatch_id, bundle.record.dispatch_id)
        self.assertEqual(bundle.execution_result.decision_id, bundle.decision.decision_id)

    def test_evaluation_result_links_to_record(self):
        engine = DispatchEngine(executor=MockExecutor())
        bundle = engine.dispatch("Test request")
        self.assertEqual(bundle.evaluation_result.dispatch_id, bundle.record.dispatch_id)
        self.assertEqual(bundle.evaluation_result.execution_result_id,
                         bundle.execution_result.result_id)

    def test_provider_disabled_gate_always_present(self):
        bundle = self.engine.dispatch("Test request")
        gate_types = [g.gate_type for g in bundle.decision.execution_gates]
        self.assertIn("provider_disabled", gate_types)
        self.assertIn("sandbox_disabled", gate_types)

    def test_low_risk_noop_is_decided(self):
        """Low-risk noop dispatch should have decision_status=decided."""
        bundle = self.engine.dispatch("Summarize the README")
        self.assertEqual(bundle.decision.decision_status, "decided")
        self.assertEqual(bundle.record.final_status, "not_executed")

    def test_low_risk_noop_gates_are_info(self):
        """Provider/sandbox gates for low-risk noop should be info, not block."""
        bundle = self.engine.dispatch("Summarize the README")
        for g in bundle.decision.execution_gates:
            if g.gate_type in ("provider_disabled", "sandbox_disabled"):
                self.assertEqual(g.severity, "info")


class GateTests(unittest.TestCase):
    def setUp(self):
        self.engine = DispatchEngine()

    def test_high_risk_generates_risk_gate(self):
        bundle = self.engine.dispatch("Fix the bug and commit changes to main")
        gate_types = [g.gate_type for g in bundle.decision.execution_gates]
        self.assertIn("target_write", gate_types)
        self.assertEqual(bundle.decision.decision_status, "needs_approval")

    def test_target_write_generates_gate(self):
        bundle = self.engine.dispatch("Fix the bug and commit the changes to main")
        gate_types = [g.gate_type for g in bundle.decision.execution_gates]
        self.assertIn("target_write", gate_types)

    def test_low_confidence_generates_gate(self):
        bundle = self.engine.dispatch("Make it better")
        gate_types = [g.gate_type for g in bundle.decision.execution_gates]
        self.assertIn("confidence", gate_types)


class GoldenFixtureE2ETests(unittest.TestCase):
    """End-to-end dispatch of all 20 golden fixtures."""

    def setUp(self):
        self.engine = DispatchEngine()

    def _run_fixture(self, filename):
        path = FIXTURES_DIR / filename
        with open(path) as f:
            fixture = json.load(f)

        bundle = self.engine.dispatch(
            fixture["raw_request"],
            request_source=fixture.get("request_source", "test_fixture"),
        )

        self.assertTrue(bundle.record.dispatch_id, f"dispatch_id missing for {filename}")
        self.assertTrue(bundle.record.task_analysis_id, f"task_analysis_id missing for {filename}")
        self.assertTrue(bundle.record.decision_id, f"decision_id missing for {filename}")
        self.assertIn(bundle.record.final_status,
                      ("not_executed", "completed", "failed", "escalated", "manual_pending"))
        return bundle

    def test_fixture_01(self):
        self._run_fixture("fixture_01_low_risk_summary.json")

    def test_fixture_02(self):
        self._run_fixture("fixture_02_doc_audit.json")

    def test_fixture_03(self):
        self._run_fixture("fixture_03_code_review.json")

    def test_fixture_04(self):
        self._run_fixture("fixture_04_code_gen.json")

    def test_fixture_05(self):
        self._run_fixture("fixture_05_debug.json")

    def test_fixture_06(self):
        self._run_fixture("fixture_06_architecture.json")

    def test_fixture_07(self):
        self._run_fixture("fixture_07_math.json")

    def test_fixture_08(self):
        self._run_fixture("fixture_08_config_review.json")

    def test_fixture_09(self):
        self._run_fixture("fixture_09_infra_deploy.json")

    def test_fixture_10(self):
        self._run_fixture("fixture_10_provider_boundary.json")

    def test_fixture_11(self):
        self._run_fixture("fixture_11_target_write.json")

    def test_fixture_12(self):
        self._run_fixture("fixture_12_secret_handling.json")

    def test_fixture_13(self):
        self._run_fixture("fixture_13_long_context.json")

    def test_fixture_14(self):
        self._run_fixture("fixture_14_ambiguous.json")

    def test_fixture_15(self):
        self._run_fixture("fixture_15_conflicting.json")

    def test_fixture_16(self):
        self._run_fixture("fixture_16_read_only_high_risk.json")

    def test_fixture_17(self):
        self._run_fixture("fixture_17_negated_no_write.json")

    def test_fixture_18(self):
        self._run_fixture("fixture_18_negated_no_execute.json")

    def test_fixture_19(self):
        self._run_fixture("fixture_19_budget_constrained.json")

    def test_fixture_20(self):
        self._run_fixture("fixture_20_high_quality_critical.json")


class DeterministicReplayTests(unittest.TestCase):
    def test_same_input_same_domain(self):
        engine = DispatchEngine()
        b1 = engine.dispatch("Summarize the README")
        b2 = engine.dispatch("Summarize the README")
        self.assertEqual(b1.analysis.task_domain, b2.analysis.task_domain)
        self.assertEqual(b1.analysis.task_intent, b2.analysis.task_intent)
        self.assertEqual(b1.record.final_status, b2.record.final_status)

    def test_bundle_analysis_matches_request(self):
        engine = DispatchEngine()
        bundle = engine.dispatch("Summarize the README")
        self.assertEqual(bundle.analysis.raw_request_snapshot, "Summarize the README")
        self.assertEqual(bundle.analysis.task_domain, "docs")
        self.assertEqual(bundle.analysis.task_intent, "summarize")


if __name__ == "__main__":
    unittest.main()
