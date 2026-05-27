"""E2E tests for Phase 2: full manual execution flow."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.cost_of_pass import CostOfPassAccumulator
from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.manual_evaluator import ManualEvaluator
from harness_core.dispatch.manual_session import ManualSessionStore
from harness_core.dispatch.manual_usage_bridge import ManualUsageBridge
from harness_core.dispatch.pasteback_parser import PastebackParser
from harness_core.dispatch.prompt_pack_gen import PromptPackGenerator


class ManualExecutionFlowE2E(unittest.TestCase):
    """Full lifecycle: dispatch → prompt pack → session → pasteback → evaluate → ledger."""

    def test_full_manual_flow(self):
        # Step 1: Dispatch
        engine = DispatchEngine()
        bundle = engine.dispatch("Summarize the README")
        decision = bundle.decision
        dispatch_id = bundle.record.dispatch_id

        # Step 2: Generate prompt pack (uses record.dispatch_id, not decision.decision_id)
        gen = PromptPackGenerator()
        pack = gen.generate(decision, "Summarize the README", dispatch_id=dispatch_id)
        self.assertTrue(pack.prompt_pack_id)
        self.assertEqual(pack.dispatch_id, dispatch_id)
        self.assertNotEqual(pack.dispatch_id, decision.decision_id)

        # Step 3: Create session
        store = ManualSessionStore()
        session = store.create(dispatch_id, pack.prompt_pack_id)
        self.assertEqual(session.status, "created")
        self.assertEqual(session.dispatch_id, dispatch_id)

        # Step 4: Advance to prompt_generated
        session = store.advance(session, "prompt_generated")
        self.assertEqual(session.status, "prompt_generated")

        # Step 5: Human executes and pastes back
        parser = PastebackParser()
        submission = parser.parse(
            dispatch_id,
            "The README describes a token-efficient agent harness with dispatch kernel.",
            model_used="gpt-4",
            provider_used="openai",
            claimed_input_tokens=500,
            claimed_output_tokens=200,
            claimed_cost=0.005,
        )
        self.assertEqual(submission.dispatch_id, dispatch_id)
        session = store.advance(session, "result_submitted", submission_id=submission.submission_id)
        self.assertEqual(session.submission_id, submission.submission_id)

        # Step 6: Evaluate
        evaluator = ManualEvaluator()
        eval_result = evaluator.evaluate(submission, pack)
        self.assertIn(eval_result.status, ("pass", "needs_human_review"))
        session = store.advance(session, "evaluated", evaluation_id=eval_result.eval_id)

        # Step 7: Write to usage ledger (pass_ derived from eval_result)
        bridge = ManualUsageBridge()
        usage_row = bridge.bridge(
            submission,
            eval_result=eval_result,
            prompt_pack=pack,
            case_id="manual_readme_summary",
            cost_of_pass_group="manual/dispatch/summary/success",
            model_profile_id="gpt-4",
        )
        self.assertTrue(usage_row.run_id)
        self.assertTrue(usage_row.pass_)

        # Step 8: Record in accumulator
        accum = CostOfPassAccumulator()
        accum.add(usage_row)
        self.assertEqual(accum.total_rows(), 1)
        self.assertGreater(accum.total_cost(), 0)

        # Step 9: Finalize session
        session = store.advance(session, "recorded")
        self.assertEqual(session.status, "recorded")

    def test_multiple_manual_executions(self):
        engine = DispatchEngine()
        gen = PromptPackGenerator()
        store = ManualSessionStore()
        parser = PastebackParser()
        evaluator = ManualEvaluator()
        bridge = ManualUsageBridge()
        accum = CostOfPassAccumulator()

        requests = [
            "Summarize the README",
            "Review auth.py for security issues",
            "Calculate the optimal batch size",
        ]

        for req in requests:
            bundle = engine.dispatch(req)
            did = bundle.record.dispatch_id
            pack = gen.generate(bundle.decision, req, dispatch_id=did)
            session = store.create(did, pack.prompt_pack_id)
            session = store.advance(session, "prompt_generated")

            sub = parser.parse(did, f"Output for: {req}")
            session = store.advance(session, "result_submitted", submission_id=sub.submission_id)

            eval_result = evaluator.evaluate(sub, pack)
            session = store.advance(session, "evaluated", evaluation_id=eval_result.eval_id)

            row = bridge.bridge(sub, eval_result=eval_result, prompt_pack=pack,
                                cost_of_pass_group=f"manual/dispatch/{req.split()[0].lower()}/success")
            accum.add(row)

            session = store.advance(session, "recorded")

        self.assertEqual(accum.total_rows(), 3)
        self.assertEqual(len(store.list_sessions()), 3)

    def test_no_provider_calls_in_flow(self):
        """Phase 2 boundary: no provider calls should be possible."""
        engine = DispatchEngine()
        bundle = engine.dispatch("Test request")
        # Verify execution policy has no provider
        self.assertNotEqual(bundle.execution_result.executor_type, "provider")
        # Verify all gates include provider_disabled
        gate_types = [g.gate_type for g in bundle.decision.execution_gates]
        self.assertIn("provider_disabled", gate_types)


if __name__ == "__main__":
    unittest.main()
