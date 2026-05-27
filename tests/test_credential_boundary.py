"""Tests for credential_boundary.py — CredentialBoundary env resolution and redaction."""

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.provider.credential_boundary import CredentialBoundary
from harness_core.dispatch.provider.provider_config import CredentialRef


class CredentialBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.boundary = CredentialBoundary(backend="env")
        self.ref = CredentialRef(
            credential_ref_id="TEST_SECRET_KEY",
            storage_backend="env",
            redacted_display="ts-***abc",
            scope="provider:test",
        )

    def test_resolve_from_env(self):
        os.environ["TEST_SECRET_KEY"] = "super-secret-value-123"
        try:
            value = self.boundary.resolve(self.ref)
            self.assertEqual(value, "super-secret-value-123")
        finally:
            del os.environ["TEST_SECRET_KEY"]

    def test_resolve_missing_raises(self):
        os.environ.pop("TEST_SECRET_KEY", None)
        with self.assertRaises(ValueError):
            self.boundary.resolve(self.ref)

    def test_validate_returns_true_when_set(self):
        os.environ["TEST_SECRET_KEY"] = "secret"
        try:
            self.assertTrue(self.boundary.validate(self.ref))
        finally:
            del os.environ["TEST_SECRET_KEY"]

    def test_validate_returns_false_when_missing(self):
        os.environ.pop("TEST_SECRET_KEY", None)
        self.assertFalse(self.boundary.validate(self.ref))

    def test_redact_display_long(self):
        result = CredentialBoundary.redact_display("sk-1234567890abcdef")
        self.assertEqual(result, "sk-***def")

    def test_redact_display_short(self):
        result = CredentialBoundary.redact_display("ab")
        self.assertEqual(result, "***")

    def test_unsupported_backend_raises(self):
        with self.assertRaises(ValueError):
            CredentialBoundary(backend="keyring")


if __name__ == "__main__":
    unittest.main()
