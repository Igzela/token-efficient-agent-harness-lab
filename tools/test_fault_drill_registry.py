from __future__ import annotations

import unittest

from scripts.fault_drill_contract import ContractError
from scripts.fault_drill_harness import fixed_command
from scripts.fault_drill_registry import (
    REGISTRY,
    SCENARIOS_BY_ID,
    SUITES,
    get_scenario,
    scenario_for,
    scenario_ids_for,
    validate_registry,
)


class FaultDrillRegistryTests(unittest.TestCase):
    def test_registry_is_unique_bounded_and_owner_bound(self) -> None:
        validate_registry()
        self.assertEqual(len(REGISTRY), len(SCENARIOS_BY_ID))
        for spec in REGISTRY:
            self.assertTrue(spec.owner)
            self.assertGreater(spec.timeout_ms, 0)
            command = fixed_command(spec)
            self.assertTrue(command)
            self.assertFalse(any(token in command for token in (";", "|", "&&", "`", "$")))
            scenario = scenario_for(spec, source_head="a" * 40, seed=3, worker_id=0)
            self.assertTrue(all(resource["disposable"] for resource in scenario["resources"]))
            self.assertEqual(
                set(scenario["environment"]["capabilities"]),
                set(spec.required_capabilities),
            )

    def test_allowlisted_suites_and_filters_have_no_arbitrary_input(self) -> None:
        self.assertEqual(set(scenario_ids_for(suite="all")), set(SCENARIOS_BY_ID))
        self.assertEqual(
            scenario_ids_for(scenario_id="pe6.release.provenance_rollback.v2"),
            ("pe6.release.provenance_rollback.v2",),
        )
        for suite, ids in SUITES.items():
            self.assertTrue(suite)
            self.assertTrue(ids)
            for scenario_id in ids:
                self.assertEqual(get_scenario(scenario_id).scenario_id, scenario_id)
        with self.assertRaises(ContractError):
            scenario_ids_for(scenario_id="../../run-anything")
        with self.assertRaises(ContractError):
            scenario_ids_for(suite="not-registered")

    def test_worker_identity_is_deterministic_but_isolated(self) -> None:
        spec = get_scenario("pe6.storage.sqlite.atomicity.v2")
        first = scenario_for(spec, source_head="a" * 40, seed=4, worker_id=1)
        repeated = scenario_for(spec, source_head="a" * 40, seed=4, worker_id=1)
        other_worker = scenario_for(spec, source_head="a" * 40, seed=4, worker_id=2)
        self.assertEqual(first, repeated)
        self.assertNotEqual(first["resources"], other_worker["resources"])


if __name__ == "__main__":
    unittest.main()
