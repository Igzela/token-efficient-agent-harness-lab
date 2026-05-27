"""Tests for openai_provider.py — OpenAIProvider error mapping (no real API calls)."""

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.provider.audit_recorder import ProviderAuditRecorder
from harness_core.dispatch.provider.credential_boundary import CredentialBoundary
from harness_core.dispatch.provider.openai_provider import OpenAIProvider
from harness_core.dispatch.provider.provider_config import CredentialRef, ProviderConfig


class OpenAIProviderTests(unittest.TestCase):
    def setUp(self):
        self.config = ProviderConfig(
            provider_id="openai-test",
            provider_type="openai_compatible",
            base_url="https://api.openai.com/v1",
            model_id="gpt-4",
            credential_ref="OPENAI_TEST_KEY",
            timeout_ms=5000,
        )
        self.cred_ref = CredentialRef(
            credential_ref_id="OPENAI_TEST_KEY",
            storage_backend="env",
            redacted_display="sk-***test",
            scope="provider:openai",
        )
        self.boundary = CredentialBoundary(backend="env")
        self.audit = ProviderAuditRecorder()

    def test_execute_without_key_fails(self):
        os.environ.pop("OPENAI_TEST_KEY", None)
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        engine = DispatchEngine()
        bundle = engine.dispatch("Test")
        result = provider.execute(bundle.decision, "Test", bundle.record.dispatch_id)
        self.assertEqual(result.status, "failed")
        self.assertEqual(result.error_domain, "provider_auth")

    def test_health_check_fails_without_key(self):
        os.environ.pop("OPENAI_TEST_KEY", None)
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        self.assertFalse(provider.health_check())

    def test_audit_events_recorded(self):
        os.environ.pop("OPENAI_TEST_KEY", None)
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        engine = DispatchEngine()
        bundle = engine.dispatch("Test")
        provider.execute(bundle.decision, "Test", bundle.record.dispatch_id)
        events = self.audit.list_events(bundle.record.dispatch_id)
        self.assertTrue(len(events) > 0)
        event_types = [e.event_type for e in events]
        self.assertIn("error", event_types)

    def test_error_domain_mapped(self):
        os.environ.pop("OPENAI_TEST_KEY", None)
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        engine = DispatchEngine()
        bundle = engine.dispatch("Test")
        result = provider.execute(bundle.decision, "Test", bundle.record.dispatch_id)
        self.assertIn(result.error_domain, ("provider_auth", "provider_error", "provider_timeout"))

    def test_audit_events_never_contain_secrets(self):
        os.environ.pop("OPENAI_TEST_KEY", None)
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        engine = DispatchEngine()
        bundle = engine.dispatch("Test with secret api_key=sk-12345")
        provider.execute(bundle.decision, "Test with secret api_key=sk-12345", bundle.record.dispatch_id)
        for event in self.audit.list_all():
            d = event.to_dict()
            for key, val in d.items():
                if isinstance(val, str):
                    self.assertNotIn("sk-12345", val, f"Secret leaked in audit field {key}")
                    self.assertNotIn("Authorization", val, f"Auth header leaked in audit field {key}")
                    self.assertNotIn("api_key", val.lower().replace("_", ""), f"API key reference in audit field {key}")

    def test_engine_provider_execution_policy(self):
        os.environ.pop("OPENAI_TEST_KEY", None)
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        engine = DispatchEngine(executor=provider)
        bundle = engine.dispatch("Test")
        self.assertEqual(bundle.decision.execution_policy["executor_type"], "provider")
        self.assertTrue(bundle.decision.execution_policy["execution_allowed"])
        gate_types = [g.gate_type for g in bundle.decision.execution_gates]
        self.assertNotIn("provider_disabled", gate_types)
        self.assertNotIn("no_provider_call", bundle.decision.hard_constraints)


class CountingProvider:
    """Provider that counts execute calls — verifies engine blocks when needed."""

    def __init__(self):
        self.calls = 0

    def execute(self, decision, raw_request, dispatch_id):
        self.calls += 1
        from harness_core.dispatch.executor_adapter import ExecutionResult
        from datetime import datetime, timezone
        import uuid
        return ExecutionResult(
            result_id=f"exec-{uuid.uuid4().hex[:12]}",
            dispatch_id=dispatch_id,
            decision_id=decision.decision_id,
            executor_type="provider",
            status="provider_completed",
            output="counted",
            attempt_number=1,
            created_at=datetime.now(timezone.utc).isoformat(),
        )


class ProviderExecutionGuardTests(unittest.TestCase):
    def test_provider_not_called_when_decision_blocked(self):
        counting = CountingProvider()
        engine = DispatchEngine(executor=counting)
        bundle = engine.dispatch("Fix auth.py and commit changes to main")
        self.assertEqual(bundle.decision.decision_status, "needs_approval")
        self.assertEqual(counting.calls, 0)
        self.assertEqual(bundle.execution_result.status, "not_executed")
        self.assertEqual(bundle.execution_result.error_domain, "execution_not_authorized")

    def test_provider_called_when_decision_decided(self):
        counting = CountingProvider()
        engine = DispatchEngine(executor=counting)
        bundle = engine.dispatch("Summarize the README")
        self.assertEqual(bundle.decision.decision_status, "decided")
        self.assertEqual(counting.calls, 1)
        self.assertEqual(bundle.execution_result.status, "provider_completed")


if __name__ == "__main__":
    unittest.main()
