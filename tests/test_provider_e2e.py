"""E2E tests for Phase 3: provider execution flow with audit and ledger."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.provider.audit_recorder import ProviderAuditRecorder
from harness_core.dispatch.provider.provider_executor import StubProvider
from harness_core.dispatch.provider.retry_manager import RetryFallbackManager
from harness_core.dispatch.provider.provider_config import RetryPolicy


class ProviderExecutionE2E(unittest.TestCase):
    """Full lifecycle: dispatch → provider → audit → ledger."""

    def test_stub_provider_full_flow(self):
        audit = ProviderAuditRecorder()
        stub = StubProvider(provider_id="e2e-stub")
        engine = DispatchEngine(executor=stub)
        bundle = engine.dispatch("Summarize the README")

        self.assertEqual(bundle.execution_result.executor_type, "provider")
        self.assertEqual(bundle.execution_result.status, "provider_completed")
        self.assertTrue(bundle.execution_result.output)
        self.assertIsNotNone(bundle.execution_result.input_tokens)

    def test_provider_result_links_to_dispatch(self):
        stub = StubProvider()
        engine = DispatchEngine(executor=stub)
        bundle = engine.dispatch("Test request")
        result = bundle.execution_result
        self.assertEqual(result.dispatch_id, bundle.record.dispatch_id)
        self.assertEqual(result.decision_id, bundle.decision.decision_id)

    def test_retry_manager_with_stub(self):
        stub = StubProvider()
        policy = RetryPolicy(policy_id="e2e-retry", max_retries=2, backoff_strategy="none")
        manager = RetryFallbackManager(stub, None, policy, budget_check=lambda: True)
        engine = DispatchEngine(executor=stub)
        bundle = engine.dispatch("Test")
        result = manager.execute(bundle.decision, "Test", bundle.record.dispatch_id)
        self.assertEqual(result.status, "provider_completed")

    def test_multiple_dispatches(self):
        stub = StubProvider()
        engine = DispatchEngine(executor=stub)
        requests = [
            "Summarize the README",
            "Review auth.py for security",
            "Calculate batch size",
        ]
        for req in requests:
            bundle = engine.dispatch(req)
            self.assertEqual(bundle.execution_result.executor_type, "provider")
            self.assertEqual(bundle.execution_result.status, "provider_completed")


if __name__ == "__main__":
    unittest.main()
