"""Tests for provider_config.py — ProviderConfig, CredentialRef, RetryPolicy schemas."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.provider.provider_config import (
    ProviderConfig,
    CredentialRef,
    RetryPolicy,
    PROVIDER_TYPES,
    CREDENTIAL_STORAGE_BACKENDS,
    BACKOFF_STRATEGIES,
)


class ProviderConfigTests(unittest.TestCase):
    def test_create_config(self):
        config = ProviderConfig(
            provider_id="openai-1",
            provider_type="openai_compatible",
            base_url="https://api.openai.com/v1",
            model_id="gpt-4",
            credential_ref="OPENAI_API_KEY",
        )
        self.assertEqual(config.provider_id, "openai-1")
        self.assertEqual(config.provider_type, "openai_compatible")
        self.assertTrue(config.enabled)
        self.assertEqual(config.timeout_ms, 30_000)
        self.assertEqual(config.max_retries, 3)

    def test_to_dict_roundtrip(self):
        config = ProviderConfig(
            provider_id="test-1",
            provider_type="openai_compatible",
            base_url="https://api.openai.com/v1",
            model_id="gpt-4",
            credential_ref="KEY",
        )
        d = config.to_dict()
        self.assertEqual(d["provider_id"], "test-1")
        self.assertIn("schema_version", d)
        self.assertEqual(d["schema_version"], "provider_config.v1")

    def test_provider_types_constant(self):
        self.assertIn("openai_compatible", PROVIDER_TYPES)
        self.assertIn("anthropic", PROVIDER_TYPES)
        self.assertIn("local", PROVIDER_TYPES)


class CredentialRefTests(unittest.TestCase):
    def test_create_ref(self):
        ref = CredentialRef(
            credential_ref_id="OPENAI_API_KEY",
            storage_backend="env",
            redacted_display="sk-***abc",
            scope="provider:openai",
        )
        self.assertEqual(ref.credential_ref_id, "OPENAI_API_KEY")
        self.assertEqual(ref.storage_backend, "env")

    def test_to_dict_roundtrip(self):
        ref = CredentialRef(
            credential_ref_id="KEY",
            storage_backend="env",
            redacted_display="sk-***abc",
            scope="provider:openai",
        )
        d = ref.to_dict()
        self.assertEqual(d["credential_ref_id"], "KEY")
        self.assertIn("schema_version", d)


class RetryPolicyTests(unittest.TestCase):
    def test_create_policy(self):
        policy = RetryPolicy(policy_id="default-retry")
        self.assertEqual(policy.max_retries, 3)
        self.assertEqual(policy.backoff_strategy, "exponential")
        self.assertTrue(policy.budget_check_per_retry)

    def test_to_dict_roundtrip(self):
        policy = RetryPolicy(policy_id="test-retry", max_retries=5)
        d = policy.to_dict()
        self.assertEqual(d["max_retries"], 5)
        self.assertIsInstance(d["retryable_error_domains"], list)

    def test_retryable_domains_include_rate_limit(self):
        policy = RetryPolicy(policy_id="default-retry")
        self.assertIn("provider_rate_limit", policy.retryable_error_domains)
        self.assertIn("provider_timeout", policy.retryable_error_domains)


if __name__ == "__main__":
    unittest.main()
