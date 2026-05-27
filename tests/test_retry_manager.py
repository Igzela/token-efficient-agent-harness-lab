"""Tests for retry_manager.py — RetryFallbackManager retry, budget, fallback."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.provider.provider_config import RetryPolicy
from harness_core.dispatch.provider.provider_executor import ProviderExecutor, StubProvider
from harness_core.dispatch.provider.retry_manager import RetryFallbackManager
from harness_core.dispatch.executor_adapter import ExecutionResult


class FailingProvider(ProviderExecutor):
    """Always fails with a retryable error."""

    def __init__(self, fail_count: int = 3):
        self._fail_count = fail_count
        self._attempts = 0

    def execute(self, decision, raw_request, dispatch_id):
        self._attempts += 1
        if self._attempts <= self._fail_count:
            return ExecutionResult(
                result_id=f"exec-fail-{self._attempts}",
                dispatch_id=dispatch_id,
                decision_id=decision.decision_id,
                executor_type="provider",
                status="failed",
                error_domain="provider_rate_limit",
                error_message="rate limited",
                attempt_number=self._attempts,
                created_at="2026-01-01T00:00:00Z",
            )
        return StubProvider().execute(decision, raw_request, dispatch_id)

    def health_check(self):
        return True


class RetryFallbackManagerTests(unittest.TestCase):
    def setUp(self):
        self.engine = DispatchEngine()
        self.policy = RetryPolicy(
            policy_id="test-retry",
            max_retries=3,
            backoff_strategy="none",
            budget_check_per_retry=False,
        )

    def test_succeeds_without_retry(self):
        stub = StubProvider()
        manager = RetryFallbackManager(stub, None, self.policy, budget_check=lambda: True)
        bundle = self.engine.dispatch("Test")
        result = manager.execute(bundle.decision, "Test", bundle.record.dispatch_id)
        self.assertEqual(result.status, "provider_completed")

    def test_retries_on_failure(self):
        failing = FailingProvider(fail_count=2)
        stub = StubProvider()
        manager = RetryFallbackManager(failing, stub, self.policy, budget_check=lambda: True)
        bundle = self.engine.dispatch("Test")
        result = manager.execute(bundle.decision, "Test", bundle.record.dispatch_id)
        self.assertEqual(result.status, "provider_completed")
        self.assertEqual(failing._attempts, 3)

    def test_fallback_on_exhausted_retries(self):
        always_fails = FailingProvider(fail_count=100)
        stub = StubProvider()
        policy = RetryPolicy(policy_id="t", max_retries=1, backoff_strategy="none")
        manager = RetryFallbackManager(always_fails, stub, policy, budget_check=lambda: True)
        bundle = self.engine.dispatch("Test")
        result = manager.execute(bundle.decision, "Test", bundle.record.dispatch_id)
        self.assertEqual(result.status, "provider_completed")

    def test_budget_check_stops_retries(self):
        failing = FailingProvider(fail_count=10)
        policy = RetryPolicy(policy_id="t", max_retries=5, backoff_strategy="none", budget_check_per_retry=True)
        call_count = [0]
        def budget_check():
            call_count[0] += 1
            return call_count[0] <= 1
        manager = RetryFallbackManager(failing, None, policy, budget_check=budget_check)
        bundle = self.engine.dispatch("Test")
        result = manager.execute(bundle.decision, "Test", bundle.record.dispatch_id)
        self.assertEqual(result.status, "failed")
        self.assertEqual(result.error_domain, "budget_exhausted")

    def test_no_fallback_returns_failure(self):
        always_fails = FailingProvider(fail_count=100)
        policy = RetryPolicy(policy_id="t", max_retries=1, backoff_strategy="none")
        manager = RetryFallbackManager(always_fails, None, policy, budget_check=lambda: True)
        bundle = self.engine.dispatch("Test")
        result = manager.execute(bundle.decision, "Test", bundle.record.dispatch_id)
        self.assertEqual(result.status, "failed")


if __name__ == "__main__":
    unittest.main()
