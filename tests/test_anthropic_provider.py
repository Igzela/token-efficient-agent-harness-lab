"""Tests for AnthropicProvider adapter."""

from __future__ import annotations

import json
import unittest
from unittest.mock import MagicMock, patch

from harness_core.dispatch.dispatch_decision import DispatchDecision
from harness_core.dispatch.executor_adapter import ExecutionResult
from harness_core.dispatch.provider.anthropic_provider import (
    AnthropicConnectionError,
    AnthropicHTTPError,
    AnthropicProvider,
    AnthropicProviderRequest,
)
from harness_core.dispatch.provider.audit_recorder import ProviderAuditRecorder
from harness_core.dispatch.provider.credential_boundary import CredentialBoundary
from harness_core.dispatch.provider.provider_config import CredentialRef, ProviderConfig


def _make_decision(**overrides: object) -> DispatchDecision:
    from harness_core.dispatch.dispatch_decision import BudgetReservation
    defaults: dict[str, object] = {
        "decision_id": "dec-test",
        "analysis_id": "analysis-test",
        "analysis_snapshot": {},
        "selected_tier": "standard",
        "fallback_tier": "cheap",
        "routing_reason": "test",
        "quality_requirement": "standard",
        "expected_quality_band": "medium",
        "confidence": 0.8,
        "confidence_label": "high",
        "budget_reservation": BudgetReservation(
            reservation_id="r-1",
            decision_id="dec-test",
            currency="USD",
            pre_budget=10000,
            reserved_input_tokens=4000,
            reserved_output_tokens=3000,
            reserved_total_tokens=7000,
            reserved_cost=0.05,
            status="reserved",
            created_at="2026-01-01T00:00:00Z",
            updated_at="2026-01-01T00:00:00Z",
        ),
        "decision_status": "decided",
        "hard_constraints": ("no_target_write",),
        "execution_policy": {"executor_type": "provider"},
        "execution_gates": (),
        "max_output_tokens": 512,
        "created_at": "2026-01-01T00:00:00Z",
    }
    defaults.update(overrides)
    return DispatchDecision(**defaults)  # type: ignore[arg-type]


def _make_config(**overrides: object) -> ProviderConfig:
    defaults = {
        "provider_id": "mimo-test",
        "provider_type": "anthropic",
        "base_url": "https://api.example.com/anthropic",
        "model_id": "mimo-v2.5",
        "credential_ref": "MIMO_API_KEY",
        "enabled": True,
        "created_at": "2026-01-01T00:00:00Z",
    }
    defaults.update(overrides)
    return ProviderConfig(**defaults)  # type: ignore[arg-type]


class TestAnthropicProviderDisabled(unittest.TestCase):
    def test_disabled_returns_not_executed(self) -> None:
        config = _make_config(enabled=False)
        boundary = MagicMock(spec=CredentialBoundary)
        cred_ref = MagicMock(spec=CredentialRef)
        provider = AnthropicProvider(config, boundary, cred_ref)

        decision = _make_decision()
        result = provider.execute(decision, "hello", "disp-1")

        self.assertEqual(result.status, "not_executed")
        self.assertEqual(result.error_domain, "provider_disabled")
        self.assertEqual(result.executor_type, "provider")


class TestAnthropicProviderAuth(unittest.TestCase):
    def test_missing_key_returns_failed(self) -> None:
        config = _make_config()
        boundary = MagicMock(spec=CredentialBoundary)
        boundary.resolve.side_effect = ValueError("env var not set")
        cred_ref = MagicMock(spec=CredentialRef)
        provider = AnthropicProvider(config, boundary, cred_ref)

        decision = _make_decision()
        result = provider.execute(decision, "hello", "disp-1")

        self.assertEqual(result.status, "failed")
        self.assertEqual(result.error_domain, "provider_auth")


class TestAnthropicProviderSuccess(unittest.TestCase):
    @patch("harness_core.dispatch.provider.anthropic_provider.anthropic_urlopen")
    def test_successful_call(self, mock_urlopen: MagicMock) -> None:
        response_body = json.dumps({
            "id": "msg-123",
            "content": [{"type": "text", "text": "Hello from mimo!"}],
            "usage": {"input_tokens": 10, "output_tokens": 5},
            "stop_reason": "end_turn",
        }).encode()

        mock_resp = MagicMock()
        mock_resp.status = 200
        mock_resp.read.return_value = response_body
        mock_resp.__enter__ = MagicMock(return_value=mock_resp)
        mock_resp.__exit__ = MagicMock(return_value=False)
        mock_urlopen.return_value = mock_resp

        config = _make_config(input_cost_per_1k=0.01, output_cost_per_1k=0.03)
        boundary = MagicMock(spec=CredentialBoundary)
        boundary.resolve.return_value = "test-key-123"
        cred_ref = MagicMock(spec=CredentialRef)
        provider = AnthropicProvider(config, boundary, cred_ref)

        decision = _make_decision()
        result = provider.execute(decision, "Fix the bug", "disp-1")

        self.assertEqual(result.status, "provider_completed")
        self.assertEqual(result.output, "Hello from mimo!")
        self.assertEqual(result.input_tokens, 10)
        self.assertEqual(result.output_tokens, 5)
        self.assertIsNotNone(result.estimated_cost)
        self.assertAlmostEqual(result.estimated_cost, 0.00025, places=5)
        self.assertEqual(result.provider_request_id, "msg-123")
        self.assertEqual(result.finish_reason, "end_turn")

        call_args = mock_urlopen.call_args
        req = call_args[0][0]
        self.assertIn("/v1/messages", req.url)
        self.assertEqual(req.headers["x-api-key"], "test-key-123")
        self.assertEqual(req.headers["anthropic-version"], "2023-06-01")
        body = json.loads(req.data)
        self.assertEqual(body["model"], "mimo-v2.5")
        self.assertEqual(body["messages"][0]["content"], "Fix the bug")

    @patch("harness_core.dispatch.provider.anthropic_provider.anthropic_urlopen")
    def test_http_error_maps_domain(self, mock_urlopen: MagicMock) -> None:
        mock_urlopen.side_effect = AnthropicHTTPError(429, "rate limited")

        config = _make_config()
        boundary = MagicMock(spec=CredentialBoundary)
        boundary.resolve.return_value = "key"
        cred_ref = MagicMock(spec=CredentialRef)
        provider = AnthropicProvider(config, boundary, cred_ref)

        result = provider.execute(_make_decision(), "hello", "disp-1")

        self.assertEqual(result.status, "failed")
        self.assertEqual(result.error_domain, "provider_rate_limit")

    @patch("harness_core.dispatch.provider.anthropic_provider.anthropic_urlopen")
    def test_connection_error(self, mock_urlopen: MagicMock) -> None:
        mock_urlopen.side_effect = AnthropicConnectionError("timeout")

        config = _make_config()
        boundary = MagicMock(spec=CredentialBoundary)
        boundary.resolve.return_value = "key"
        cred_ref = MagicMock(spec=CredentialRef)
        provider = AnthropicProvider(config, boundary, cred_ref)

        result = provider.execute(_make_decision(), "hello", "disp-1")

        self.assertEqual(result.status, "failed")
        self.assertEqual(result.error_domain, "provider_timeout")

    @patch("harness_core.dispatch.provider.anthropic_provider.anthropic_urlopen")
    def test_invalid_json_response(self, mock_urlopen: MagicMock) -> None:
        mock_resp = MagicMock()
        mock_resp.status = 200
        mock_resp.read.return_value = b"not json"
        mock_resp.__enter__ = MagicMock(return_value=mock_resp)
        mock_resp.__exit__ = MagicMock(return_value=False)
        mock_urlopen.return_value = mock_resp

        config = _make_config()
        boundary = MagicMock(spec=CredentialBoundary)
        boundary.resolve.return_value = "key"
        cred_ref = MagicMock(spec=CredentialRef)
        provider = AnthropicProvider(config, boundary, cred_ref)

        result = provider.execute(_make_decision(), "hello", "disp-1")

        self.assertEqual(result.status, "failed")
        self.assertEqual(result.error_domain, "provider_error")


class TestAnthropicProviderAudit(unittest.TestCase):
    @patch("harness_core.dispatch.provider.anthropic_provider.anthropic_urlopen")
    def test_audit_events_recorded(self, mock_urlopen: MagicMock) -> None:
        response_body = json.dumps({
            "id": "msg-1",
            "content": [{"type": "text", "text": "ok"}],
            "usage": {"input_tokens": 5, "output_tokens": 2},
            "stop_reason": "end_turn",
        }).encode()

        mock_resp = MagicMock()
        mock_resp.status = 200
        mock_resp.read.return_value = response_body
        mock_resp.__enter__ = MagicMock(return_value=mock_resp)
        mock_resp.__exit__ = MagicMock(return_value=False)
        mock_urlopen.return_value = mock_resp

        audit = MagicMock(spec=ProviderAuditRecorder)
        config = _make_config()
        boundary = MagicMock(spec=CredentialBoundary)
        boundary.resolve.return_value = "key"
        cred_ref = MagicMock(spec=CredentialRef)
        provider = AnthropicProvider(config, boundary, cred_ref, audit_recorder=audit)

        provider.execute(_make_decision(), "hello", "disp-1")

        self.assertEqual(audit.create_and_record.call_count, 2)
        event_types = [
            call.kwargs.get("event_type") or call[1].get("event_type")
            for call in audit.create_and_record.call_args_list
        ]
        self.assertIn("request_sent", event_types)
        self.assertIn("response_received", event_types)


if __name__ == "__main__":
    unittest.main()
