from __future__ import annotations

import copy
import unittest
from pathlib import Path

from scripts.fault_drill_contract import (
    ContractError,
    build_report,
    validate_report,
    validate_result,
)
from scripts.fault_drill_harness import _make_result
from scripts.fault_drill_registry import (
    SCENARIOS_BY_ID,
    scenario_for,
    scenario_ids_for,
)


ROOT = Path(__file__).resolve().parents[1]


class PE6CloseoutAuditTests(unittest.TestCase):
    def _sqlite_result(self) -> tuple[dict[str, object], dict[str, object]]:
        scenario = scenario_for(
            SCENARIOS_BY_ID["pe6.storage.sqlite.atomicity.v1"],
            source_head="a" * 40,
            seed=31,
            worker_id=7,
        )
        result = _make_result(
            scenario,
            status="passed",
            reason_codes=[
                "DRILL_PASSED",
                "RECOVERY_VERIFIED",
                "ROLLBACK_VERIFIED",
                "INTEGRITY_VERIFIED",
                "AUDIT_VERIFIED",
                "CLEANUP_VERIFIED",
            ],
            outcome="passed",
            invariant_passed=True,
            cleanup_outcome="cleaned",
            cleanup_passed=True,
            detection_reason="DRILL_PASSED",
            detected=True,
            duration_ms=1,
            observation="fixed owner test completed successfully",
        )
        return scenario, result

    def test_report_binds_aggregate_environment_to_each_result(self) -> None:
        _scenario, result = self._sqlite_result()
        with self.assertRaises(ContractError):
            build_report(
                suite="storage",
                source_head="a" * 40,
                seed=31,
                worker_id=7,
                environment={"name": "incomplete", "capabilities": ["filesystem"]},
                results=[result],
            )

        report = build_report(
            suite="storage",
            source_head="a" * 40,
            seed=31,
            worker_id=7,
            environment={
                "name": "linux-sqlite",
                "capabilities": ["filesystem", "sqlite", "rust_engine"],
            },
            results=[result],
        )
        self.assertEqual(validate_report(report), report)

        tampered = copy.deepcopy(report)
        tampered["environment"]["capabilities"] = ["filesystem"]
        with self.assertRaises(ContractError):
            validate_report(tampered)

    def test_result_obeys_scenario_evidence_reference_limit(self) -> None:
        scenario, result = self._sqlite_result()
        constrained = copy.deepcopy(scenario)
        constrained["max_evidence_refs"] = 1
        with self.assertRaises(ContractError):
            validate_result(result, constrained)

    def test_all_registered_owner_drills_and_existing_ci_wiring_are_present(self) -> None:
        expected_ids = {
            "pe6.harness.timeout_cleanup.v1",
            "pe6.storage.sqlite.atomicity.v1",
            "pe6.storage.sqlite.backup_restore.v1",
            "pe6.storage.postgres.atomicity.v1",
            "pe6.workflow.recovery.v1",
            "pe6.provider.safety.v1",
            "pe6.release.provenance_rollback.v1",
        }
        self.assertEqual(set(SCENARIOS_BY_ID), expected_ids)
        self.assertEqual(set(scenario_ids_for(suite="all")), expected_ids)

        owner_sources = {
            "pe6.harness.timeout_cleanup.v1": ("tools/test_pe6_harness_drill.py", "test_registered_timeout_and_cleanup"),
            "pe6.storage.sqlite.atomicity.v1": ("engine/tests/test_pe6_fault_drills.rs", "pe6_sqlite_atomicity_and_integrity"),
            "pe6.storage.sqlite.backup_restore.v1": ("engine/tests/test_pe6_fault_drills.rs", "pe6_sqlite_backup_restore_and_cleanup"),
            "pe6.storage.postgres.atomicity.v1": ("engine/tests/test_pe6_fault_drills.rs", "pe6_postgres_atomicity_when_service_is_available"),
            "pe6.workflow.recovery.v1": ("engine/tests/test_pe6_fault_drills.rs", "pe6_workflow_timeout_retry_concurrency_and_restart"),
            "pe6.provider.safety.v1": ("engine/tests/test_pe6_fault_drills.rs", "pe6_provider_timeout_kill_budget_audit_and_redaction"),
            "pe6.release.provenance_rollback.v1": ("tools/test_pe6_release_drill.py", "test_release_verification_precedes_activation_and_rolls_back"),
        }
        for scenario_id, (relative_path, marker) in owner_sources.items():
            source = (ROOT / relative_path).read_text(encoding="utf-8")
            self.assertIn(marker, source, scenario_id)
            self.assertTrue(SCENARIOS_BY_ID[scenario_id].owner)

        workflow = (ROOT / ".github/workflows/tests.yml").read_text(encoding="utf-8")
        self.assertIn('python -m unittest discover -s tools -p "test_*.py"', workflow)
        self.assertIn("cargo test -p engine --features pg-tests -- --test-threads=1", workflow)

    def test_harness_has_no_external_release_provider_or_host_authority(self) -> None:
        harness = (ROOT / "scripts/fault_drill_harness.py").read_text(encoding="utf-8")
        runner = (ROOT / "tools/run_fault_drills.py").read_text(encoding="utf-8")
        self.assertNotIn("shell=True", harness)
        self.assertNotIn("gh release", harness + runner)
        self.assertNotIn("git tag", harness + runner)
        self.assertNotIn("curl ", harness + runner)
        self.assertNotIn("ACP_ENABLE_PROVIDER_EXECUTION=1", harness)
        self.assertEqual(list((ROOT / ".github/workflows").glob("*pe6*")), [])


if __name__ == "__main__":
    unittest.main()
