import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.advisor import (
    AdvisorBroker,
    AdvisorBrokerBudgetExceeded,
    AdvisorBudget,
    AdvisorContextPack,
    AdvisorProtocolValidator,
    AdvisorResponse,
    StubAdvisorProvider,
)


def make_context(call_type="preflight", task_id="task_001", failure_code=None, completion=None):
    return AdvisorContextPack(
        task_id=task_id,
        call_type=call_type,
        task_spec={"task_id": task_id, "type": "code_small_change"},
        completion=completion,
        handoff_pack=None,
        run_log_text=None,
        failure_code=failure_code,
        project_context=None,
    )


class StubAdvisorProviderTests(unittest.TestCase):
    def test_preflight_returns_go(self):
        provider = StubAdvisorProvider()
        ctx = make_context("preflight")
        resp = provider.invoke(ctx, AdvisorBudget())
        self.assertEqual("preflight", resp.call_type)
        self.assertIn("proceed", resp.recommended_action.lower())
        self.assertEqual(0.9, resp.confidence)
        self.assertEqual("stub", resp.provider)

    def test_correction_uses_failure_code(self):
        provider = StubAdvisorProvider()
        ctx = make_context("correction", failure_code="F007_TEST_FAILURE")
        resp = provider.invoke(ctx, AdvisorBudget())
        self.assertEqual("correction", resp.call_type)
        self.assertIn("F007", resp.diagnosis)
        self.assertIn("test", resp.recommended_action.lower())

    def test_correction_unknown_failure_code(self):
        provider = StubAdvisorProvider()
        ctx = make_context("correction", failure_code="F999_UNKNOWN")
        resp = provider.invoke(ctx, AdvisorBudget())
        self.assertEqual("correction", resp.call_type)
        self.assertIn("F999", resp.recommended_action)

    def test_arbitration_pass_when_score_high(self):
        provider = StubAdvisorProvider()
        ctx = make_context("arbitration", completion={"score": 0.8})
        resp = provider.invoke(ctx, AdvisorBudget())
        self.assertEqual("arbitration", resp.call_type)
        self.assertEqual("pass", resp.recommended_action)

    def test_arbitration_fail_when_score_low(self):
        provider = StubAdvisorProvider()
        ctx = make_context("arbitration", completion={"score": 0.3})
        resp = provider.invoke(ctx, AdvisorBudget())
        self.assertEqual("arbitration", resp.call_type)
        self.assertIn("fail", resp.recommended_action)

    def test_risk_scan_low_risk(self):
        provider = StubAdvisorProvider()
        ctx = make_context("risk_scan")
        resp = provider.invoke(ctx, AdvisorBudget())
        self.assertEqual("risk_scan", resp.call_type)
        self.assertIn("low", resp.diagnosis)

    def test_risk_scan_high_risk(self):
        provider = StubAdvisorProvider()
        ctx = make_context("risk_scan", completion={"requested_action": ["delete_files"]})
        resp = provider.invoke(ctx, AdvisorBudget())
        self.assertEqual("risk_scan", resp.call_type)
        self.assertIn("high", resp.diagnosis)

    def test_deterministic_repeatability(self):
        provider = StubAdvisorProvider()
        ctx = make_context("preflight")
        budget = AdvisorBudget()
        r1 = provider.invoke(ctx, budget)
        r2 = provider.invoke(ctx, budget)
        self.assertEqual(r1, r2)


class AdvisorBrokerTests(unittest.TestCase):
    def test_preflight_call(self):
        broker = AdvisorBroker(StubAdvisorProvider(), AdvisorBudget())
        ctx = make_context("preflight")
        resp = broker.preflight(ctx)
        self.assertEqual("preflight", resp.call_type)
        self.assertEqual(1, broker.budget.current_calls)

    def test_correction_call(self):
        broker = AdvisorBroker(StubAdvisorProvider(), AdvisorBudget())
        ctx = make_context("correction", failure_code="F008_FORMAT_ERROR")
        resp = broker.correction(ctx)
        self.assertEqual("correction", resp.call_type)

    def test_arbitration_call(self):
        broker = AdvisorBroker(StubAdvisorProvider(), AdvisorBudget())
        ctx = make_context("arbitration", completion={"score": 0.7})
        resp = broker.arbitration(ctx)
        self.assertEqual("arbitration", resp.call_type)

    def test_risk_scan_call(self):
        broker = AdvisorBroker(StubAdvisorProvider(), AdvisorBudget())
        ctx = make_context("risk_scan")
        resp = broker.risk_scan(ctx)
        self.assertEqual("risk_scan", resp.call_type)

    def test_max_calls_exceeded(self):
        budget = AdvisorBudget(max_calls_per_task=2)
        broker = AdvisorBroker(StubAdvisorProvider(), budget)
        broker.preflight(make_context("preflight"))
        broker.correction(make_context("correction"))
        with self.assertRaises(AdvisorBrokerBudgetExceeded):
            broker.arbitration(make_context("arbitration"))

    def test_max_tokens_exceeded(self):
        budget = AdvisorBudget(max_tokens=100, max_calls_per_task=10)
        broker = AdvisorBroker(StubAdvisorProvider(), budget)
        # First call succeeds (budget check is pre-call), but after preflight
        # current_tokens becomes 120. Next call should fail.
        broker.preflight(make_context("preflight"))
        with self.assertRaises(AdvisorBrokerBudgetExceeded):
            broker.correction(make_context("correction"))

    def test_budget_tracks_tokens(self):
        budget = AdvisorBudget(max_tokens=5000)
        broker = AdvisorBroker(StubAdvisorProvider(), budget)
        broker.preflight(make_context("preflight"))
        self.assertGreater(broker.budget.current_tokens, 0)

    def test_budget_tracks_calls(self):
        broker = AdvisorBroker(StubAdvisorProvider(), AdvisorBudget())
        self.assertEqual(0, broker.budget.current_calls)
        broker.preflight(make_context("preflight"))
        self.assertEqual(1, broker.budget.current_calls)
        broker.correction(make_context("correction"))
        self.assertEqual(2, broker.budget.current_calls)

    def test_missing_context_returns_error_response(self):
        broker = AdvisorBroker(StubAdvisorProvider(), AdvisorBudget())
        ctx = AdvisorContextPack(
            task_id="",
            call_type="preflight",
            task_spec={},
        )
        resp = broker.preflight(ctx)
        self.assertIn("invalid context", resp.diagnosis)
        self.assertEqual(0.0, resp.confidence)

    def test_all_four_call_types(self):
        budget = AdvisorBudget(max_calls_per_task=10, max_tokens=10000)
        broker = AdvisorBroker(StubAdvisorProvider(), budget)
        types = ["preflight", "correction", "arbitration", "risk_scan"]
        for call_type in types:
            ctx = make_context(call_type)
            if call_type == "correction":
                ctx = make_context(call_type, failure_code="F007_TEST_FAILURE")
            if call_type == "arbitration":
                ctx = make_context(call_type, completion={"score": 0.7})
            resp = getattr(broker, call_type)(ctx)
            self.assertEqual(call_type, resp.call_type)


class AdvisorProtocolValidatorTests(unittest.TestCase):
    def setUp(self):
        self.validator = AdvisorProtocolValidator()

    def test_valid_response_accepted(self):
        resp = AdvisorResponse(
            call_type="preflight",
            diagnosis="ok",
            recommended_action="proceed",
            do_not_do="none",
            confidence=0.9,
            token_usage=100,
            provider="stub",
        )
        result = self.validator.validate_response(resp)
        self.assertTrue(result.ok)

    def test_invalid_call_type_rejected(self):
        resp = AdvisorResponse(
            call_type="invalid_type",
            diagnosis="ok",
            recommended_action="proceed",
            do_not_do="none",
            confidence=0.9,
            token_usage=100,
            provider="stub",
        )
        result = self.validator.validate_response(resp)
        self.assertFalse(result.ok)
        self.assertTrue(any("call_type" in e for e in result.errors))

    def test_invalid_confidence_rejected(self):
        resp = AdvisorResponse(
            call_type="preflight",
            diagnosis="ok",
            recommended_action="proceed",
            do_not_do="none",
            confidence=1.5,
            token_usage=100,
            provider="stub",
        )
        result = self.validator.validate_response(resp)
        self.assertFalse(result.ok)
        self.assertTrue(any("confidence" in e for e in result.errors))

    def test_negative_confidence_rejected(self):
        resp = AdvisorResponse(
            call_type="preflight",
            diagnosis="ok",
            recommended_action="proceed",
            do_not_do="none",
            confidence=-0.1,
            token_usage=100,
            provider="stub",
        )
        result = self.validator.validate_response(resp)
        self.assertFalse(result.ok)

    def test_negative_token_usage_rejected(self):
        resp = AdvisorResponse(
            call_type="preflight",
            diagnosis="ok",
            recommended_action="proceed",
            do_not_do="none",
            confidence=0.9,
            token_usage=-1,
            provider="stub",
        )
        result = self.validator.validate_response(resp)
        self.assertFalse(result.ok)
        self.assertTrue(any("token_usage" in e for e in result.errors))

    def test_empty_provider_rejected(self):
        resp = AdvisorResponse(
            call_type="preflight",
            diagnosis="ok",
            recommended_action="proceed",
            do_not_do="none",
            confidence=0.9,
            token_usage=100,
            provider="",
        )
        result = self.validator.validate_response(resp)
        self.assertFalse(result.ok)
        self.assertTrue(any("provider" in e for e in result.errors))

    def test_valid_context_pack_accepted(self):
        ctx = make_context("preflight")
        result = self.validator.validate_context_pack(ctx)
        self.assertTrue(result.ok)

    def test_empty_task_id_rejected(self):
        ctx = AdvisorContextPack(task_id="", call_type="preflight", task_spec={"x": 1})
        result = self.validator.validate_context_pack(ctx)
        self.assertFalse(result.ok)

    def test_empty_task_spec_rejected(self):
        ctx = AdvisorContextPack(task_id="t1", call_type="preflight", task_spec={})
        result = self.validator.validate_context_pack(ctx)
        self.assertFalse(result.ok)

    def test_valid_budget_accepted(self):
        budget = AdvisorBudget(max_tokens=2000, max_calls_per_task=3)
        result = self.validator.validate_budget(budget)
        self.assertTrue(result.ok)

    def test_invalid_budget_rejected(self):
        budget = AdvisorBudget(max_tokens=0, max_calls_per_task=-1)
        result = self.validator.validate_budget(budget)
        self.assertFalse(result.ok)
        self.assertEqual(2, len(result.errors))


if __name__ == "__main__":
    unittest.main()
