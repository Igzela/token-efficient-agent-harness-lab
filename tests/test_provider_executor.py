"""Tests for provider_executor.py — StubProvider execution and health check."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.provider.provider_executor import StubProvider


class StubProviderTests(unittest.TestCase):
    def setUp(self):
        self.provider = StubProvider(provider_id="test-stub")

    def test_execute_returns_provider_result(self):
        engine = DispatchEngine()
        bundle = engine.dispatch("Summarize the README")
        result = self.provider.execute(bundle.decision, "Summarize the README", bundle.record.dispatch_id)
        self.assertEqual(result.executor_type, "provider")
        self.assertEqual(result.status, "provider_completed")
        self.assertTrue(result.output)
        self.assertEqual(result.provider_request_id, f"stub-{__import__('hashlib').sha256(b'Summarize the README').hexdigest()[:16]}")

    def test_execute_has_tokens(self):
        engine = DispatchEngine()
        bundle = engine.dispatch("Test")
        result = self.provider.execute(bundle.decision, "Test request", bundle.record.dispatch_id)
        self.assertIsNotNone(result.input_tokens)
        self.assertIsNotNone(result.output_tokens)
        self.assertGreater(result.input_tokens, 0)
        self.assertGreater(result.output_tokens, 0)

    def test_health_check_always_true(self):
        self.assertTrue(self.provider.health_check())

    def test_deterministic_output(self):
        engine = DispatchEngine()
        bundle = engine.dispatch("Test")
        r1 = self.provider.execute(bundle.decision, "Same input", bundle.record.dispatch_id)
        r2 = self.provider.execute(bundle.decision, "Same input", bundle.record.dispatch_id)
        self.assertEqual(r1.output, r2.output)

    def test_links_to_dispatch(self):
        engine = DispatchEngine()
        bundle = engine.dispatch("Test")
        result = self.provider.execute(bundle.decision, "Test", bundle.record.dispatch_id)
        self.assertEqual(result.dispatch_id, bundle.record.dispatch_id)
        self.assertEqual(result.decision_id, bundle.decision.decision_id)


if __name__ == "__main__":
    unittest.main()
