"""Tests for manual_usage_bridge.py and cost_of_pass.py — usage bridge and cost aggregation."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.cost_of_pass import CostOfPassAccumulator
from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.manual_evaluator import ManualEvaluator
from harness_core.dispatch.manual_usage_bridge import ManualUsageBridge
from harness_core.dispatch.pasteback_parser import PastebackParser
from harness_core.dispatch.prompt_pack_gen import PromptPackGenerator
from harness_core.usage_ledger import UsageLedgerRow


def _make_eval_result(status: str = "pass"):
    from harness_core.dispatch.manual_evaluator import ManualEvalResult, ManualEvalCheck
    return ManualEvalResult(
        eval_id="meval-test-001",
        dispatch_id="disp-001",
        submission_id="pb-001",
        status=status,
        checks=(ManualEvalCheck(check_id="mc-001", name="output_present", status="pass", reason="ok"),),
        created_at="2026-01-01T00:00:00Z",
    )


class ManualUsageBridgeTests(unittest.TestCase):
    def setUp(self):
        self.bridge = ManualUsageBridge()
        self.parser = PastebackParser()

    def test_bridge_creates_usage_row(self):
        sub = self.parser.parse("disp-001", "Model output here")
        eval_result = _make_eval_result("pass")
        row = self.bridge.bridge(sub, eval_result=eval_result)
        self.assertIsInstance(row, UsageLedgerRow)
        self.assertTrue(row.run_id)
        self.assertEqual(row.case_id, "manual_dispatch")

    def test_bridge_uses_claimed_tokens(self):
        sub = self.parser.parse(
            "disp-001", "Output",
            claimed_input_tokens=500,
            claimed_output_tokens=200,
            claimed_cost=0.003,
        )
        eval_result = _make_eval_result("pass")
        row = self.bridge.bridge(sub, eval_result=eval_result)
        self.assertEqual(row.input_tokens, 500)
        self.assertEqual(row.output_tokens, 200)
        self.assertEqual(row.estimated_cost, 0.003)

    def test_bridge_estimates_when_no_claims(self):
        sub = self.parser.parse("disp-001", "Short output")
        eval_result = _make_eval_result("pass")
        row = self.bridge.bridge(sub, eval_result=eval_result)
        self.assertGreater(row.input_tokens, 0)
        self.assertGreater(row.output_tokens, 0)
        self.assertGreater(row.estimated_cost, 0)

    def test_bridge_custom_group(self):
        sub = self.parser.parse("disp-001", "Output")
        eval_result = _make_eval_result("pass")
        row = self.bridge.bridge(
            sub,
            eval_result=eval_result,
            cost_of_pass_group="eval/task_a/variant_1/success",
            model_profile_id="gpt-4",
        )
        self.assertEqual(row.cost_of_pass_group, "eval/task_a/variant_1/success")
        self.assertEqual(row.model_profile_id, "gpt-4")

    def test_bridge_pass_derived_from_eval_result(self):
        engine = DispatchEngine()
        gen = PromptPackGenerator()
        evaluator = ManualEvaluator()

        bundle = engine.dispatch("Summarize the README")
        did = bundle.record.dispatch_id
        pack = gen.generate(bundle.decision, "Summarize the README", dispatch_id=did)
        sub = self.parser.parse(did, "The README describes the project.")

        eval_pass = evaluator.evaluate(sub, pack)
        row = self.bridge.bridge(sub, eval_result=eval_pass)
        self.assertTrue(row.pass_)

    def test_bridge_fail_on_eval_fail(self):
        engine = DispatchEngine()
        gen = PromptPackGenerator()
        evaluator = ManualEvaluator()

        bundle = engine.dispatch("Summarize the README")
        did = bundle.record.dispatch_id
        pack = gen.generate(bundle.decision, "Summarize the README", dispatch_id=did)
        sub = self.parser.parse(did, "Traceback (most recent call last):\n  Error: something broke")

        eval_result = evaluator.evaluate(sub, pack)
        self.assertEqual(eval_result.status, "needs_human_review")
        row = self.bridge.bridge(sub, eval_result=eval_result)
        self.assertFalse(row.pass_)

    def test_bridge_input_tokens_from_prompt_pack(self):
        engine = DispatchEngine()
        gen = PromptPackGenerator()

        bundle = engine.dispatch("Summarize the README")
        did = bundle.record.dispatch_id
        pack = gen.generate(bundle.decision, "Summarize the README", dispatch_id=did)
        sub = self.parser.parse(did, "Short output")
        eval_result = _make_eval_result("pass")

        row = self.bridge.bridge(sub, eval_result=eval_result, prompt_pack=pack)
        self.assertGreater(row.input_tokens, 0)
        self.assertGreater(row.output_tokens, 0)


class CostOfPassAccumulatorTests(unittest.TestCase):
    def setUp(self):
        self.accum = CostOfPassAccumulator()
        self.bridge = ManualUsageBridge()
        self.parser = PastebackParser()

    def _add_row(self, group: str = "manual/task/default/success", passed: bool = True):
        sub = self.parser.parse("disp-001", "Output text")
        eval_status = "pass" if passed else "fail"
        eval_result = _make_eval_result(eval_status)
        row = self.bridge.bridge(sub, eval_result=eval_result, cost_of_pass_group=group)
        self.accum.add(row)

    def test_total_rows(self):
        self._add_row()
        self._add_row()
        self.assertEqual(self.accum.total_rows(), 2)

    def test_total_cost(self):
        self._add_row()
        self.assertGreater(self.accum.total_cost(), 0)

    def test_success_rate(self):
        self._add_row(passed=True)
        self._add_row(passed=True)
        self._add_row(passed=False)
        self.assertAlmostEqual(self.accum.success_rate(), 0.6667, places=3)

    def test_success_rate_empty(self):
        self.assertEqual(self.accum.success_rate(), 0.0)

    def test_rows_for_group(self):
        self._add_row("manual/task_a/default/success")
        self._add_row("manual/task_a/default/success")
        self._add_row("manual/task_b/default/success")
        rows = self.accum.rows_for_group("manual/task_a/default/success")
        self.assertEqual(len(rows), 2)

    def test_aggregate_all(self):
        self._add_row("manual/task_a/default/success")
        self._add_row("manual/task_a/default/success")
        aggs = self.accum.aggregate_all()
        self.assertTrue(len(aggs) > 0)
        agg = aggs[0]
        self.assertEqual(agg.total_count, 2)

    def test_aggregate_group(self):
        self._add_row("manual/task_a/default/success")
        agg = self.accum.aggregate_group("manual/task_a/default/success")
        self.assertIsNotNone(agg)
        self.assertEqual(agg.total_count, 1)


if __name__ == "__main__":
    unittest.main()
