from __future__ import annotations

import os
import subprocess
import unittest
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from unittest.mock import Mock, patch

from scripts.fault_drill_contract import ContractError
from scripts.fault_drill_harness import (
    CommandOutcome,
    _execute_fixed_command,
    fixed_command,
    run_scenario,
)
from scripts.fault_drill_registry import get_scenario


class HarnessOwnerDrillTests(unittest.TestCase):
    def test_registered_timeout_and_cleanup(self) -> None:
        source_head = "a" * 40

        def timeout(_command, **_kwargs):
            return CommandOutcome(returncode=None, timed_out=True)

        timeout_result = run_scenario(
            "pe6.harness.timeout_cleanup.v1",
            source_head=source_head,
            seed=17,
            worker_id=1,
            command_executor=timeout,
        )
        self.assertEqual(timeout_result["status"], "aborted")
        self.assertIn("DRILL_TIMEOUT", timeout_result["reason_codes"])
        self.assertEqual(timeout_result["cleanup_evidence"]["outcome"], "cleaned")

        def success(_command, **_kwargs):
            return CommandOutcome(returncode=0)

        cleanup_result = run_scenario(
            "pe6.harness.timeout_cleanup.v1",
            source_head=source_head,
            seed=18,
            worker_id=1,
            command_executor=success,
            fail_cleanup=True,
        )
        self.assertEqual(cleanup_result["status"], "cleanup_failed")
        self.assertEqual(cleanup_result["cleanup_evidence"]["outcome"], "failed")

        command = fixed_command(get_scenario("pe6.harness.timeout_cleanup.v1"))
        self.assertNotIn(";", command)
        self.assertNotIn("|", command)

    def test_unknown_scenario_is_refused(self) -> None:
        with self.assertRaises(ContractError):
            run_scenario("pe6.harness.not-registered.v1", source_head="a" * 40)

    def test_non_ci_postgres_database_is_unsupported(self) -> None:
        with patch.dict(
            os.environ,
            {
                "ACP_TEST_DATABASE_URL": "postgres://developer:password@localhost:5432/app",
                "GITHUB_ACTIONS": "false",
            },
            clear=False,
        ):
            result = run_scenario(
                "pe6.storage.postgres.atomicity.v1",
                source_head="a" * 40,
                seed=19,
                worker_id=1,
            )
        self.assertEqual(result["status"], "unsupported")
        self.assertIn("UNSUPPORTED_ENVIRONMENT", result["reason_codes"])
        self.assertEqual(result["cleanup_evidence"]["outcome"], "cleaned")

    def test_concurrent_workers_get_isolated_resources(self) -> None:
        def success(_command, **_kwargs):
            return CommandOutcome(returncode=0)

        def run(worker: int):
            return run_scenario(
                "pe6.harness.timeout_cleanup.v1",
                source_head="a" * 40,
                seed=21,
                worker_id=worker,
                command_executor=success,
            )

        with ThreadPoolExecutor(max_workers=2) as pool:
            first, second = pool.map(run, (11, 12))
        self.assertEqual(first["status"], "passed")
        self.assertEqual(second["status"], "passed")
        self.assertNotEqual(first["resources"], second["resources"])

    def test_controlled_child_timeout_crash_and_output_bounds(self) -> None:
        command = fixed_command(get_scenario("pe6.harness.timeout_cleanup.v1"))
        process = Mock()
        process.pid = 12345
        process.returncode = -9
        process.communicate.return_value = ("", "")
        with patch("scripts.fault_drill_harness.subprocess.Popen", return_value=process):
            crashed = _execute_fixed_command(command, cwd=Path.cwd(), env={}, timeout_ms=10)
        self.assertEqual(crashed.returncode, -9)
        self.assertFalse(crashed.timed_out)

        timeout_process = Mock()
        timeout_process.pid = 12346
        timeout_process.communicate.side_effect = [
            subprocess.TimeoutExpired(command, 0.01),
            ("", ""),
        ]
        with patch("scripts.fault_drill_harness.subprocess.Popen", return_value=timeout_process):
            with patch("scripts.fault_drill_harness.os.killpg"):
                timed_out = _execute_fixed_command(command, cwd=Path.cwd(), env={}, timeout_ms=10)
        self.assertTrue(timed_out.timed_out)

        output_process = Mock()
        output_process.pid = 12347
        output_process.returncode = 0
        output_process.communicate.return_value = ("x" * (4 * 1024 * 1024 + 1), "")
        with patch("scripts.fault_drill_harness.subprocess.Popen", return_value=output_process):
            oversized = _execute_fixed_command(command, cwd=Path.cwd(), env={}, timeout_ms=10)
        self.assertTrue(oversized.output_exceeded)


if __name__ == "__main__":
    unittest.main()
