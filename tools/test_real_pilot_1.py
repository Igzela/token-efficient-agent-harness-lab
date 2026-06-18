"""Focused tests for the Real Pilot 1 orchestration script."""

from __future__ import annotations

import importlib.util
import shlex
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = ROOT / "scripts" / "real_pilot_1.py"
SPEC = importlib.util.spec_from_file_location("real_pilot_1", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
real_pilot_1 = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = real_pilot_1
SPEC.loader.exec_module(real_pilot_1)


class FakeApi:
    def __init__(self, fail: bool = False) -> None:
        self.fail = fail
        self.calls: list[tuple[str, str, dict[str, object] | None]] = []

    def call(self, method: str, path: str, body=None):
        self.calls.append((method, path, body))
        if method == "POST":
            status = "failed" if self.fail else "completed"
            return {
                "tick": {
                    "action": "node_executed",
                    "result": {"status": status},
                }
            }
        return {"run": {"status": "failed" if self.fail else "completed"}}


class RealPilotOneTests(unittest.TestCase):
    def test_pick_port_requests_kernel_assigned_local_port(self):
        with patch.object(real_pilot_1, "run", return_value="43210") as run_command:
            port = real_pilot_1.pick_port()

        self.assertEqual(port, 43210)
        command = run_command.call_args.args[0]
        self.assertEqual(command[:2], [sys.executable, "-c"])
        self.assertIn("bind(('127.0.0.1', 0))", command[2])

    def test_shell_command_quotes_paths_with_spaces(self):
        command = real_pilot_1.shell_command(
            "git",
            "-C",
            Path("/tmp/acp real pilot"),
            "status",
            "--short",
        )

        self.assertEqual(
            shlex.split(command),
            ["git", "-C", "/tmp/acp real pilot", "status", "--short"],
        )

    def test_worker_uses_relative_command_and_supports_workspace_spaces(self):
        api = FakeApi()
        with tempfile.TemporaryDirectory(prefix="real pilot 1 ") as tmpdir:
            workspace = Path(tmpdir)

            result = real_pilot_1.tick_until_completed(api, "run-1", workspace)

            self.assertEqual(result["run"]["status"], "completed")
            command = api.calls[0][2]["command"]
            self.assertEqual(command, "python3 .acp-real-pilot-worker.py")
            self.assertFalse((workspace / real_pilot_1.WORKER_FILENAME).exists())

    def test_worker_is_removed_after_failed_tick(self):
        api = FakeApi(fail=True)
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace = Path(tmpdir)

            with self.assertRaises(real_pilot_1.PilotError):
                real_pilot_1.tick_until_completed(api, "run-1", workspace)

            self.assertFalse((workspace / real_pilot_1.WORKER_FILENAME).exists())

    def test_api_client_reports_curl_failure(self):
        completed = type(
            "Completed",
            (),
            {"returncode": 7, "stdout": "", "stderr": "connection refused"},
        )()
        with patch.object(real_pilot_1.subprocess, "run", return_value=completed):
            with self.assertRaisesRegex(real_pilot_1.PilotError, "connection refused"):
                real_pilot_1.ApiClient("http://127.0.0.1:1").call("GET", "/health")


if __name__ == "__main__":
    unittest.main()
