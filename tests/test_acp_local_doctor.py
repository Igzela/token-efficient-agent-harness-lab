"""Focused doctor checks for auth and live self-test gate presence."""

from __future__ import annotations

import importlib.util
import os
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def _load_doctor():
    import sys

    path = ROOT / "scripts" / "acp_local_doctor.py"
    spec = importlib.util.spec_from_file_location("acp_local_doctor", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class TestAcpLocalDoctor(unittest.TestCase):
    def setUp(self):
        self.doctor = _load_doctor()
        self._env_keys = (
            "ACP_REQUIRE_AUTH",
            "ACP_ADMIN_API_KEY",
            "ACP_SELF_TEST_LIVE_CONFIRM",
            "ACP_SELF_TEST_CREDENTIAL_ENV",
            "OPENAI_API_KEY",
        )
        self._saved = {key: os.environ.get(key) for key in self._env_keys}
        for key in self._env_keys:
            os.environ.pop(key, None)

    def tearDown(self):
        for key, value in self._saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value

    def test_auth_warn_when_protected_mode_off(self):
        check = self.doctor.auth_check()
        self.assertEqual(check.status, "warn")
        self.assertIn("fixture desktop/work still OK", check.message)
        self.assertIn("bootstrap-auth", check.action or "")

    def test_live_gate_warn_names_missing_env_without_values(self):
        check = self.doctor.live_self_test_gate_check()
        self.assertEqual(check.status, "warn")
        self.assertIn("OPENAI_API_KEY", check.action or "")
        self.assertIn("ACP_SELF_TEST_LIVE_CONFIRM", check.action or "")
        # Never echo a secret-shaped value into the action string.
        self.assertNotIn("sk-", check.action or "")
        self.assertNotIn("harness_", check.action or "")

    def test_live_gate_ok_when_present(self):
        os.environ["OPENAI_API_KEY"] = "test-placeholder-not-a-real-secret"
        os.environ["ACP_SELF_TEST_LIVE_CONFIRM"] = self.doctor.LIVE_CONFIRMATION
        check = self.doctor.live_self_test_gate_check()
        self.assertEqual(check.status, "ok")
        self.assertIn("OPENAI_API_KEY", check.message)
        self.assertNotIn("test-placeholder", check.message)


if __name__ == "__main__":
    unittest.main()
