"""Unit tests for the no-provider public demo proof contract."""

from __future__ import annotations

import importlib.util
import os
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "demo_no_provider.py"


def _load_demo():
    spec = importlib.util.spec_from_file_location("demo_no_provider", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules["demo_no_provider"] = module
    spec.loader.exec_module(module)
    return module


class TestDemoNoProviderProof(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.demo = _load_demo()

    def test_matching_revision_accepted(self):
        proof = self.demo.build_proof(
            source_revision="c" * 40,
            base_url="http://127.0.0.1:1",
            dispatch_id="disp-1",
            executor_type="noop",
            health_status="healthy",
        )
        self.demo.verify_proof_against_revision(proof, "c" * 40)

    def test_stale_revision_rejected(self):
        proof = self.demo.build_proof(
            source_revision="c" * 40,
            base_url="http://127.0.0.1:1",
            dispatch_id="disp-1",
            executor_type="noop",
            health_status="healthy",
        )
        with self.assertRaisesRegex(RuntimeError, "stale-head rejected"):
            self.demo.verify_proof_against_revision(proof, "d" * 40)

    def test_provider_flag_must_be_false(self):
        proof = self.demo.build_proof(
            source_revision="c" * 40,
            base_url="http://127.0.0.1:1",
            dispatch_id="disp-1",
            executor_type="noop",
            health_status="healthy",
        )
        proof["provider_calls"] = True
        with self.assertRaisesRegex(RuntimeError, "no provider"):
            self.demo.verify_proof_against_revision(proof, "c" * 40)

    def test_self_test_cli_exits_zero(self):
        import subprocess

        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--self-test"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("self-test ok", result.stdout)


if __name__ == "__main__":
    unittest.main()
