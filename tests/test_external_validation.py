"""Unit tests for clean-environment external validation contracts."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "external_validation.py"
SHELL = ROOT / "scripts" / "external_validation.sh"


def _load():
    spec = importlib.util.spec_from_file_location("external_validation", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules["external_validation"] = module
    spec.loader.exec_module(module)
    return module


class TestExternalValidation(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.mod = _load()

    def test_report_kind_and_boundaries(self):
        report = self.mod.build_report(
            source_revision="c" * 40,
            versions={
                "os": "Linux",
                "arch": "x86_64",
                "python": "3.11.0",
                "cargo": "cargo 1.80",
                "secret_home": "/home/someone",  # must be filtered out of tool_versions
            },
            stages=[{"name": "detect_tools", "status": "pass", "reason": "ok"}],
            status="pass",
            reason="ok",
            elapsed_ms=12,
            demo_proof={"provider_calls": False, "stale_head_rejected": True},
            exact_head={"label": "self-validation", "not_external_adoption": True},
        )
        self.assertEqual(report["kind"], "external_validation_report.v1")
        self.assertFalse(report["provider_calls"])
        self.assertFalse(report["target_repo_writes"])
        self.assertFalse(report["external_adoption_claimed"])
        self.assertEqual(report["source_revision"], "c" * 40)
        self.assertNotIn("secret_home", report["tool_versions"])
        blob = json.dumps(report)
        self.assertNotIn("/home/someone", blob)

    def test_install_contract_on_repo(self):
        stages: list = []
        self.mod.verify_install_contract(ROOT, stages)
        self.assertEqual(stages[-1]["status"], "pass")

    def test_self_test_cli(self):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--self-test"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("self-test ok", result.stdout)

    def test_shell_wrapper_self_test(self):
        result = subprocess.run(
            ["bash", str(SHELL), "--self-test"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("self-test ok", result.stdout)

    def test_skip_demo_writes_report(self):
        with tempfile.TemporaryDirectory() as tmp:
            report_path = Path(tmp) / "report.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--skip-demo",
                    "--report",
                    str(report_path),
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            self.assertTrue(report_path.exists())
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["kind"], "external_validation_report.v1")
            self.assertEqual(report["status"], "pass")
            self.assertFalse(report["external_adoption_claimed"])
            names = [s["name"] for s in report["stages"]]
            self.assertIn("install_contract", names)
            self.assertIn("exact_head_action_match", names)
            self.assertIn("exact_head_action_stale", names)
            self.assertIn("exact_head_action_fork", names)
            self.assertIn("cleanup", names)

    def test_workflow_matrix_hosts_documented(self):
        workflow = (ROOT / ".github" / "workflows" / "external-validation.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("ubuntu-latest", workflow)
        self.assertIn("macos-latest", workflow)
        self.assertNotIn("windows-latest", workflow)
        self.assertIn("external_validation", workflow)
        # Action pins must be commit SHAs (checked globally by pin script).
        self.assertIn("actions/checkout@", workflow)


if __name__ == "__main__":
    unittest.main()
