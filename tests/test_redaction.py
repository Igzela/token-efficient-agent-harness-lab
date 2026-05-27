"""Tests for redaction.py — secret stripping from text and audit fields."""

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.provider.credential_boundary import CredentialBoundary
from harness_core.dispatch.provider.provider_config import CredentialRef
from harness_core.dispatch.provider.redaction import redact_secrets, redact_audit_fields


class RedactSecretsTests(unittest.TestCase):
    def setUp(self):
        self.boundary = CredentialBoundary(backend="env")
        self.ref = CredentialRef(
            credential_ref_id="TEST_REDACT_KEY",
            storage_backend="env",
            redacted_display="tr-***xyz",
            scope="provider:test",
        )

    def test_redacts_secret_from_text(self):
        os.environ["TEST_REDACT_KEY"] = "my-secret-api-key-xyz"
        try:
            text = "Used the API key my-secret-api-key-xyz to authenticate"
            result = redact_secrets(text, [self.ref], self.boundary)
            self.assertNotIn("my-secret-api-key-xyz", result)
            self.assertIn("***", result)
        finally:
            del os.environ["TEST_REDACT_KEY"]

    def test_text_without_secret_unchanged(self):
        os.environ["TEST_REDACT_KEY"] = "my-secret-api-key-xyz"
        try:
            text = "No secrets here"
            result = redact_secrets(text, [self.ref], self.boundary)
            self.assertEqual(result, "No secrets here")
        finally:
            del os.environ["TEST_REDACT_KEY"]

    def test_missing_credential_handled_gracefully(self):
        os.environ.pop("TEST_REDACT_KEY", None)
        text = "Some text"
        result = redact_secrets(text, [self.ref], self.boundary)
        self.assertEqual(result, "Some text")


class RedactAuditFieldsTests(unittest.TestCase):
    def test_redacts_sensitive_keys(self):
        data = {
            "api_key": "sk-secret123",
            "name": "test",
            "nested": {"token": "bearer-xyz", "safe": "ok"},
        }
        result = redact_audit_fields(data)
        self.assertEqual(result["api_key"], "***")
        self.assertEqual(result["name"], "test")
        self.assertEqual(result["nested"]["token"], "***")
        self.assertEqual(result["nested"]["safe"], "ok")

    def test_preserves_non_sensitive(self):
        data = {"cost": 0.005, "tokens": 100, "status": "ok"}
        result = redact_audit_fields(data)
        self.assertEqual(result, data)


if __name__ == "__main__":
    unittest.main()
