from __future__ import annotations

import unittest
from pathlib import Path

from scripts.fault_drill_registry import SCENARIOS_BY_ID, scenario_ids_for


ROOT = Path(__file__).resolve().parents[1]


class PE6CloseoutAuditTests(unittest.TestCase):
    def test_all_v2_owner_drills_and_existing_ci_wiring_are_present(self) -> None:
        expected_ids = {
            "pe6.harness.timeout_cleanup.v2",
            "pe6.storage.sqlite.atomicity.v2",
            "pe6.storage.sqlite.backup_restore.v2",
            "pe6.storage.postgres.atomicity.v2",
            "pe6.workflow.recovery.v2",
            "pe6.provider.safety.v2",
            "pe6.release.provenance_rollback.v2",
        }
        self.assertEqual(set(SCENARIOS_BY_ID), expected_ids)
        self.assertEqual(set(scenario_ids_for(suite="all")), expected_ids)
        owner_sources = {
            "pe6.harness.timeout_cleanup.v2": ("tools/test_pe6_harness_drill.py", "test_owner_timeout_and_cleanup_evidence"),
            "pe6.storage.sqlite.atomicity.v2": ("engine/tests/test_pe6_fault_drills.rs", "pe6_sqlite_atomicity_and_integrity"),
            "pe6.storage.sqlite.backup_restore.v2": ("engine/tests/test_pe6_fault_drills.rs", "pe6_sqlite_backup_restore_and_cleanup"),
            "pe6.storage.postgres.atomicity.v2": ("engine/tests/test_pe6_fault_drills.rs", "inject_pg_config_transaction_failure_for_test"),
            "pe6.workflow.recovery.v2": ("engine/tests/test_pe6_fault_drills.rs", "pe6_workflow_timeout_retry_concurrency_and_restart"),
            "pe6.provider.safety.v2": ("engine/tests/test_pe6_fault_drills.rs", "pe6_provider_timeout_retry_budget_audit_and_redaction"),
            "pe6.release.provenance_rollback.v2": ("tools/test_pe6_release_drill.py", "UPGRADE_FAILED_ROLLBACK_SUCCEEDED"),
        }
        for scenario_id, (relative_path, marker) in owner_sources.items():
            self.assertIn(marker, (ROOT / relative_path).read_text(encoding="utf-8"), scenario_id)
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
