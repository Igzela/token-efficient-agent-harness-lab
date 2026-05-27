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


class MockUrlopen:
    """Mocks urllib.request.urlopen for testing OpenAI provider without network."""

    def __init__(self, response_body: dict, status: int = 200):
        self._body = response_body
        self._status = status

    def __enter__(self):
        import io
        body_bytes = io.BytesIO(self._body.encode() if isinstance(self._body, str) else __import__("json").dumps(self._body).encode())
        self._body_io = body_bytes
        return self

    def __exit__(self, *args):
        pass

    @property
    def status(self):
        return self._status

    def read(self):
        return self._body_io.read() if hasattr(self, '_body_io') else b'{}'


class MockUrlopenFactory:
    """Returns a mock urlopen that can be configured per-call."""

    def __init__(self, response_body: dict, status: int = 200):
        self._response_body = response_body
        self._status = status
        self.last_payload = None

    def __call__(self, req, timeout=None):
        import io
        if hasattr(req, 'data') and req.data:
            self.last_payload = __import__("json").loads(req.data.decode())
        body_bytes = io.BytesIO(__import__("json").dumps(self._response_body).encode())
        mock_resp = type('MockResp', (), {
            'status': self._status,
            'read': lambda s: body_bytes.read(),
            '__enter__': lambda s: s,
            '__exit__': lambda s, *a: None,
        })()
        return mock_resp


class OpenAIProviderMockedTests(unittest.TestCase):
    def setUp(self):
        self.config = ProviderConfig(
            provider_id="openai-mock",
            provider_type="openai_compatible",
            base_url="https://api.openai.com/v1",
            model_id="gpt-4",
            credential_ref="OPENAI_MOCK_KEY",
            timeout_ms=5000,
        )
        self.cred_ref = CredentialRef(
            credential_ref_id="OPENAI_MOCK_KEY",
            storage_backend="env",
            redacted_display="sk-***mock",
            scope="provider:openai",
        )
        self.boundary = CredentialBoundary(backend="env")
        self.audit = ProviderAuditRecorder()
        os.environ["OPENAI_MOCK_KEY"] = "sk-test-mock-key"

    def tearDown(self):
        os.environ.pop("OPENAI_MOCK_KEY", None)

    def test_successful_response_parsing(self):
        import json as _json
        mock_factory = MockUrlopenFactory({
            "id": "chatcmpl-test123",
            "choices": [{"message": {"content": "Hello world"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5},
        })
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        import harness_core.dispatch.provider.openai_provider as mod
        original_urlopen = mod.urllib.request.urlopen
        mod.urllib.request.urlopen = mock_factory
        try:
            engine = DispatchEngine()
            bundle = engine.dispatch("Say hello")
            result = provider.execute(bundle.decision, "Say hello", bundle.record.dispatch_id)
            self.assertEqual(result.status, "provider_completed")
            self.assertEqual(result.output, "Hello world")
            self.assertEqual(result.input_tokens, 10)
            self.assertEqual(result.output_tokens, 5)
            self.assertEqual(result.provider_request_id, "chatcmpl-test123")
            self.assertEqual(result.finish_reason, "stop")
        finally:
            mod.urllib.request.urlopen = original_urlopen

    def test_max_tokens_uses_decision_value(self):
        import json as _json
        mock_factory = MockUrlopenFactory({
            "id": "chatcmpl-test456",
            "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 1},
        })
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        import harness_core.dispatch.provider.openai_provider as mod
        original_urlopen = mod.urllib.request.urlopen
        mod.urllib.request.urlopen = mock_factory
        try:
            engine = DispatchEngine()
            bundle = engine.dispatch("Short request")
            result = provider.execute(bundle.decision, "Short request", bundle.record.dispatch_id)
            self.assertEqual(result.status, "provider_completed")
            max_tokens_in_payload = mock_factory.last_payload.get("max_tokens")
            expected = bundle.decision.max_output_tokens or 1024
            self.assertEqual(max_tokens_in_payload, expected)
        finally:
            mod.urllib.request.urlopen = original_urlopen

    def test_malformed_json_returns_error(self):
        import io
        def bad_urlopen(req, timeout=None):
            resp = type('R', (), {
                'status': 200,
                'read': lambda s: b'not json at all',
                '__enter__': lambda s: s,
                '__exit__': lambda s, *a: None,
            })()
            return resp
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        import harness_core.dispatch.provider.openai_provider as mod
        original_urlopen = mod.urllib.request.urlopen
        mod.urllib.request.urlopen = bad_urlopen
        try:
            engine = DispatchEngine()
            bundle = engine.dispatch("Test")
            result = provider.execute(bundle.decision, "Test", bundle.record.dispatch_id)
            self.assertEqual(result.status, "failed")
            self.assertEqual(result.error_domain, "provider_error")
            self.assertIn("parse error", result.error_message.lower())
        finally:
            mod.urllib.request.urlopen = original_urlopen

    def test_missing_choices_returns_error(self):
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        import harness_core.dispatch.provider.openai_provider as mod
        mod.urllib.request.urlopen = MockUrlopenFactory({"id": "x"})
        try:
            engine = DispatchEngine()
            bundle = engine.dispatch("Test")
            result = provider.execute(bundle.decision, "Test", bundle.record.dispatch_id)
            self.assertEqual(result.status, "failed")
            self.assertEqual(result.error_domain, "provider_error")
        finally:
            mod.urllib.request.urlopen = None

    def test_missing_content_returns_error(self):
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        import harness_core.dispatch.provider.openai_provider as mod
        mod.urllib.request.urlopen = MockUrlopenFactory({
            "id": "x", "choices": [{"message": {}, "finish_reason": "stop"}],
        })
        try:
            engine = DispatchEngine()
            bundle = engine.dispatch("Test")
            result = provider.execute(bundle.decision, "Test", bundle.record.dispatch_id)
            self.assertEqual(result.status, "failed")
            self.assertEqual(result.error_domain, "provider_error")
        finally:
            mod.urllib.request.urlopen = None

    def test_audit_events_no_secrets_on_success_path(self):
        provider = OpenAIProvider(self.config, self.boundary, self.cred_ref, self.audit)
        import harness_core.dispatch.provider.openai_provider as mod
        mod.urllib.request.urlopen = MockUrlopenFactory({
            "id": "chatcmpl-audit",
            "choices": [{"message": {"content": "response with sk-12345 secret"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5},
        })
        try:
            engine = DispatchEngine()
            bundle = engine.dispatch("Test with api_key sk-12345 and Authorization Bearer sk-abc")
            result = provider.execute(bundle.decision, "Test with api_key sk-12345 and Authorization Bearer sk-abc", bundle.record.dispatch_id)
            self.assertEqual(result.status, "provider_completed")
            for event in self.audit.list_all():
                d = event.to_dict()
                for key, val in d.items():
                    if isinstance(val, str):
                        self.assertNotIn("sk-12345", val, f"Secret leaked in audit field {key}")
                        self.assertNotIn("sk-abc", val, f"Secret leaked in audit field {key}")
                        self.assertNotIn("Authorization", val, f"Auth header leaked in audit field {key}")
        finally:
            mod.urllib.request.urlopen = None

    def test_user_negated_provider_blocks_execution(self):
        counting = CountingProvider()
        engine = DispatchEngine(executor=counting)
        bundle = engine.dispatch("Summarize this without provider calls")
        self.assertIn("no_provider_call", bundle.decision.hard_constraints)
        self.assertEqual(counting.calls, 0)
        self.assertEqual(bundle.execution_result.status, "not_executed")

    def test_disabled_provider_no_network_call(self):
        config = ProviderConfig(
            provider_id="openai-disabled",
            provider_type="openai_compatible",
            base_url="https://api.openai.com/v1",
            model_id="gpt-4",
            credential_ref="OPENAI_DISABLED_KEY",
            timeout_ms=5000,
            enabled=False,
        )
        provider = OpenAIProvider(config, self.boundary, self.cred_ref, self.audit)
        engine = DispatchEngine()
        bundle = engine.dispatch("Test")
        result = provider.execute(bundle.decision, "Test", bundle.record.dispatch_id)
        self.assertEqual(result.status, "not_executed")
        self.assertEqual(result.error_domain, "provider_disabled")
        self.assertEqual(result.attempt_number, 0)
        events = self.audit.list_events(bundle.record.dispatch_id)
        self.assertTrue(any(e.event_type == "error" and e.error_domain == "provider_disabled" for e in events))


if __name__ == "__main__":
    unittest.main()
