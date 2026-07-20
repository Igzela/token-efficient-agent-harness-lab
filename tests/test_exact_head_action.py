"""Contract tests for actions/exact-head-check without network."""

from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACTION = ROOT / "actions" / "exact-head-check"
VERIFY = ACTION / "verify.sh"
LOCAL_TEST = ACTION / "test_verify_local.sh"


class TestExactHeadAction(unittest.TestCase):
    def test_action_yml_exists_and_is_composite(self):
        text = (ACTION / "action.yml").read_text(encoding="utf-8")
        self.assertIn("using: composite", text)
        self.assertIn("expected-head", text)
        self.assertIn("pull-request", text)
        self.assertIn("github-token", text)
        self.assertIn("verify.sh", text)

    def test_verify_script_is_executable_contract(self):
        text = VERIFY.read_text(encoding="utf-8")
        self.assertIn("exact-head-check-proof.v1", text)
        self.assertIn("head_moved", text)
        self.assertIn("INPUT_ALLOW_FORK_HEAD", text)
        self.assertIn("GITHUB_STEP_SUMMARY", text)
        self.assertIn("merges: false", text)
        self.assertIn("model_calls: false", text)

    def test_local_validation_script_passes(self):
        result = subprocess.run(
            ["bash", str(LOCAL_TEST)],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("passed", result.stdout)

    def test_example_workflow_present(self):
        example = ROOT / "examples" / "github-actions" / "exact-head-check.yml"
        text = example.read_text(encoding="utf-8")
        self.assertIn("exact-head-check", text)
        self.assertIn("expected-head", text)
        self.assertIn("upload-artifact", text)


if __name__ == "__main__":
    unittest.main()
