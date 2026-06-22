from __future__ import annotations

import importlib.util
import io
import subprocess
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch


def load_handoff_checker():
    repo_root = Path(__file__).resolve().parents[1]
    script = repo_root / "scripts" / "check_agent_handoff.py"
    spec = importlib.util.spec_from_file_location("check_agent_handoff", script)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def completed(command: list[str], returncode: int = 0, stdout: str = ""):
    return subprocess.CompletedProcess(command, returncode, stdout=stdout, stderr="")


class CheckAgentHandoffTests(unittest.TestCase):
    def test_handoff_guard_runs_full_secret_scan(self) -> None:
        checker = load_handoff_checker()
        commands: list[list[str]] = []

        def fake_run(command, **_kwargs):
            commands.append([str(part) for part in command])
            return completed(commands[-1])

        with patch.object(checker.subprocess, "run", side_effect=fake_run):
            self.assertEqual(checker.main(), 0)

        self.assertTrue(
            any(command[-1].endswith("scripts/acp_secret_scan.py") for command in commands),
            commands,
        )

    def test_handoff_guard_fails_when_secret_scan_fails(self) -> None:
        checker = load_handoff_checker()

        def fake_run(command, **_kwargs):
            normalized = [str(part) for part in command]
            if normalized[-1].endswith("scripts/acp_secret_scan.py"):
                return completed(normalized, returncode=1, stdout="Secret scan findings:\n- x")
            return completed(normalized)

        output = io.StringIO()
        with patch.object(checker.subprocess, "run", side_effect=fake_run):
            with redirect_stdout(output):
                self.assertEqual(checker.main(), 1)

        self.assertIn("Agent handoff check FAILED — secret scan:", output.getvalue())


if __name__ == "__main__":
    unittest.main()
