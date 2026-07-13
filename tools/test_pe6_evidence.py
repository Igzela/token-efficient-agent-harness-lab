from __future__ import annotations

import unittest
from pathlib import Path

from scripts.fault_drill_contract import ContractError
from scripts.fault_drill_registry import scenario_ids_for


ROOT = Path(__file__).resolve().parents[1]


class FaultDrillEvidenceTests(unittest.TestCase):
    def test_existing_ci_runs_tool_registry_and_rust_pg_owners(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "tests.yml").read_text(encoding="utf-8")
        self.assertIn('python -m unittest discover -s tools -p "test_*.py"', workflow)
        self.assertIn("cargo test -p engine", workflow)
        self.assertIn("cargo test -p engine --features pg-tests -- --test-threads=1", workflow)
        self.assertNotIn("fault_drill_harness.py --arbitrary-command", workflow)

    def test_only_named_suites_or_registered_ids_are_selectable(self) -> None:
        self.assertTrue(scenario_ids_for(suite="core"))
        with self.assertRaises(ContractError):
            scenario_ids_for(suite="all; rm -rf /")
        with self.assertRaises(ContractError):
            scenario_ids_for(scenario_id="pe6.storage.sqlite.atomicity.v1 --shell")


if __name__ == "__main__":
    unittest.main()
