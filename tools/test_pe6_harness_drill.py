from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from unittest.mock import patch

from scripts.fault_drill_contract import ContractError
from scripts.fault_drill_owner import emit_owner_evidence
from scripts.fault_drill_harness import (
    CommandOutcome,
    _execute_fixed_command,
    fixed_command,
    run_scenario,
)
from scripts.fault_drill_registry import get_scenario


class HarnessOwnerDrillTests(unittest.TestCase):
    _environment_lock = threading.Lock()

    def test_owner_timeout_and_cleanup_evidence(self) -> None:
        root = Path(os.environ.get("ACP_PE6_DISPOSABLE_ROOT", tempfile.gettempdir()))
        if not root.is_dir():
            root = Path(tempfile.gettempdir())
        owned = Path(tempfile.mkdtemp(prefix="owner-timeout-", dir=root))
        marker = owned / "before.txt"
        marker.write_text("bounded\n", encoding="utf-8")
        timed_out = False
        try:
            subprocess.run(
                ["sleep", "0.2"], check=False, capture_output=True, text=True, timeout=0.01
            )
        except subprocess.TimeoutExpired:
            timed_out = True
        self.assertTrue(timed_out)
        shutil.rmtree(owned)
        self.assertFalse(owned.exists())
        emit_owner_evidence(
            observed_state_before_fault="a disposable child and owner directory existed before timeout",
            observed_fault="the controlled sleep child exceeded its ten millisecond timeout",
            observed_recovery_or_refusal="the timeout exception terminated the child and cleanup removed the owner directory",
            checks=[
                {"name": "pe6.harness.child_timeout_detected", "category": "recovery", "outcome": "passed", "observation": "the bounded child raised TimeoutExpired"},
                {"name": "pe6.harness.rollback_not_exercised", "category": "rollback", "outcome": "unsupported", "observation": "the harness drill had no deployment rollback target"},
                {"name": "pe6.harness.integrity_marker_bounded", "category": "integrity", "outcome": "passed", "observation": "only the disposable marker existed before cleanup"},
                {"name": "pe6.harness.audit_not_exercised", "category": "audit", "outcome": "unsupported", "observation": "no runtime audit owner participated"},
                {"name": "pe6.harness.restart_not_exercised", "category": "restart", "outcome": "unsupported", "observation": "the timed-out child was not restarted"},
                {"name": "pe6.harness.owner_dir_removed", "category": "cleanup", "outcome": "passed", "observation": "the owner directory was observed absent"},
            ],
            cleanup_outcome="passed",
            cleanup_observation="the owner-created directory was removed and observed absent",
        )

    @staticmethod
    def _successful_owner(_command, **kwargs):
        with HarnessOwnerDrillTests._environment_lock:
            with patch.dict(os.environ, kwargs["env"], clear=False):
                emit_owner_evidence(
                    observed_state_before_fault="a harness-owned disposable command resource was ready",
                    observed_fault="the test executor injected a bounded successful owner completion",
                    observed_recovery_or_refusal="the mock owner returned its scenario-bound checks",
                    checks=[
                        {"name": "pe6.harness.mock_recovery", "category": "recovery", "outcome": "passed", "observation": "the bounded mock completed"},
                        {"name": "pe6.harness.mock_rollback", "category": "rollback", "outcome": "unsupported", "observation": "rollback was outside the mock"},
                        {"name": "pe6.harness.mock_integrity", "category": "integrity", "outcome": "passed", "observation": "scenario bindings were retained"},
                        {"name": "pe6.harness.mock_audit", "category": "audit", "outcome": "unsupported", "observation": "audit was outside the mock"},
                        {"name": "pe6.harness.mock_restart", "category": "restart", "outcome": "unsupported", "observation": "restart was outside the mock"},
                        {"name": "pe6.harness.mock_cleanup", "category": "cleanup", "outcome": "passed", "observation": "no owner resource remained"},
                    ],
                    cleanup_outcome="passed",
                    cleanup_observation="the mock owner retained no external resource",
                )
        return CommandOutcome(returncode=0)

    def test_registered_timeout_and_cleanup(self) -> None:
        source_head = "a" * 40

        def timeout(_command, **_kwargs):
            return CommandOutcome(returncode=None, timed_out=True)

        timeout_result = run_scenario(
            "pe6.harness.timeout_cleanup.v2",
            source_head=source_head,
            seed=17,
            worker_id=1,
            command_executor=timeout,
        )
        self.assertEqual(timeout_result["status"], "aborted")
        self.assertIn("DRILL_TIMEOUT", timeout_result["reason_codes"])
        self.assertEqual(timeout_result["harness_cleanup"]["outcome"], "passed")

        cleanup_result = run_scenario(
            "pe6.harness.timeout_cleanup.v2",
            source_head=source_head,
            seed=18,
            worker_id=1,
            command_executor=self._successful_owner,
            fail_cleanup=True,
        )
        self.assertEqual(cleanup_result["status"], "cleanup_failed")
        self.assertEqual(cleanup_result["harness_cleanup"]["outcome"], "failed")

        command = fixed_command(get_scenario("pe6.harness.timeout_cleanup.v2"))
        self.assertNotIn(";", command)
        self.assertNotIn("|", command)

    def test_unknown_scenario_is_refused(self) -> None:
        with self.assertRaises(ContractError):
            run_scenario("pe6.harness.not-registered.v2", source_head="a" * 40)

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
                "pe6.storage.postgres.atomicity.v2",
                source_head="a" * 40,
                seed=19,
                worker_id=1,
            )
        self.assertEqual(result["status"], "unsupported")
        self.assertIn("UNSUPPORTED_ENVIRONMENT", result["reason_codes"])
        self.assertEqual(result["harness_cleanup"]["outcome"], "passed")

    def test_concurrent_workers_get_isolated_resources(self) -> None:
        def run(worker: int):
            return run_scenario(
                "pe6.harness.timeout_cleanup.v2",
                source_head="a" * 40,
                seed=21,
                worker_id=worker,
                command_executor=self._successful_owner,
            )

        with ThreadPoolExecutor(max_workers=2) as pool:
            first, second = pool.map(run, (11, 12))
        self.assertEqual(first["status"], "passed")
        self.assertEqual(second["status"], "passed")
        self.assertNotEqual(first["resources"], second["resources"])

    def test_controlled_child_timeout_crash_and_output_bounds(self) -> None:
        environment = dict(os.environ)
        crashed = _execute_fixed_command(
            (sys.executable, "-c", "raise SystemExit(9)"),
            cwd=Path.cwd(), env=environment, timeout_ms=2_000,
        )
        self.assertEqual(crashed.returncode, 9)
        self.assertFalse(crashed.timed_out)

        timed_out = _execute_fixed_command(
            (sys.executable, "-c", "import time; time.sleep(1)"),
            cwd=Path.cwd(), env=environment, timeout_ms=10,
        )
        self.assertTrue(timed_out.timed_out)

        oversized = _execute_fixed_command(
            (sys.executable, "-c", "import sys; sys.stdout.write('x' * (4 * 1024 * 1024 + 1))"),
            cwd=Path.cwd(), env=environment, timeout_ms=5_000,
        )
        self.assertTrue(oversized.output_exceeded)

    def test_zero_exit_without_owner_evidence_is_rejected(self) -> None:
        result = run_scenario(
            "pe6.harness.timeout_cleanup.v2",
            source_head="a" * 40,
            command_executor=lambda _command, **_kwargs: CommandOutcome(returncode=0),
        )
        self.assertEqual(result["status"], "failed_recovery")
        self.assertIn("OWNER_EVIDENCE_INVALID", result["reason_codes"])
        self.assertIsNone(result["owner_evidence"])

    def test_observed_duration_uses_monotonic_elapsed_time(self) -> None:
        def delayed(command, **kwargs):
            time.sleep(0.012)
            return self._successful_owner(command, **kwargs)

        result = run_scenario(
            "pe6.harness.timeout_cleanup.v2",
            source_head="a" * 40,
            command_executor=delayed,
        )
        self.assertGreaterEqual(result["observed_duration_ms"], 10)
        self.assertEqual(result["configured_timeout_ms"], 30_000)


if __name__ == "__main__":
    unittest.main()
