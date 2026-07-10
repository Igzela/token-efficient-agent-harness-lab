from __future__ import annotations

import copy
import importlib.util
import json
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = ROOT / "scripts" / "token_efficiency_regression.py"
FIXTURE_PATH = ROOT / "tests" / "fixtures" / "token_efficiency_regression" / "registry.json"
SPEC = importlib.util.spec_from_file_location("token_efficiency_regression", SCRIPT_PATH)
assert SPEC is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def load_script(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


NATIVE_PILOT = load_script(
    "pe1_native_stateful_experiment_pilot",
    ROOT / "scripts" / "native_stateful_experiment_pilot.py",
)
LOCAL_RUNNER = load_script(
    "pe1_provider_gated_real_runner",
    ROOT / "scripts" / "provider_gated_real_runner.py",
)


class TokenEfficiencyRegressionRegistryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.raw_registry = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))

    def test_fixed_registry_is_bounded_report_only_and_deterministic(self) -> None:
        registry = MODULE.load_registry(FIXTURE_PATH)
        repeated = MODULE.load_registry(FIXTURE_PATH)

        self.assertEqual(registry, repeated)
        self.assertEqual(registry["schema_version"], "token_efficiency_regression_registry.v1")
        self.assertEqual(len(registry["scenarios"]), 3)
        self.assertEqual(
            {scenario["scenario_id"] for scenario in registry["scenarios"]},
            {
                "langgraph_offline_state_retention_pilot_2026_07_10",
                "native_remember_dont_reread_pilot",
                "provider_gated_remember_dont_reread_runner",
            },
        )
        for scenario in registry["scenarios"]:
            self.assertEqual(set(scenario["evidence_roles"]), {"baseline", "candidate"})
            self.assertEqual(scenario["best_known_role"], "candidate")
            self.assertEqual(scenario["comparison_metadata"]["evidence_kind"], "bounded_summary_only")
            self.assertTrue(scenario["comparison_metadata"]["report_only"])
            self.assertEqual(scenario["comparison_metadata"]["provider_calls"], "disabled")
            self.assertEqual(scenario["comparison_metadata"]["mutation_authority"], "none")
            self.assertEqual(
                set(scenario["allowed_regressions"]),
                {
                    "total_tokens",
                    "repeated_context_ratio",
                    "state_bytes",
                    "estimated_cost_usd",
                    "duration_ms",
                    "retry_count",
                    "quality_score",
                },
            )

    def test_registry_hash_rejects_tampering(self) -> None:
        tampered = copy.deepcopy(self.raw_registry)
        tampered["scenarios"][0]["quality"]["threshold"] = 0.5

        with self.assertRaisesRegex(MODULE.RegressionRegistryError, "registry_sha256"):
            MODULE.validate_registry(tampered)

    def test_registry_contracts_match_existing_fixed_evidence_producers(self) -> None:
        scenarios = {
            scenario["scenario_id"]: scenario for scenario in MODULE.load_registry(FIXTURE_PATH)["scenarios"]
        }
        langgraph = json.loads(
            (ROOT / "tests" / "fixtures" / "langgraph_pilot" / "stateless_reread.artifact.json").read_text(
                encoding="utf-8"
            )
        )["scorecard"]
        native = NATIVE_PILOT.build_pair()[0]
        runner_config = LOCAL_RUNNER.RunnerConfig(
            live=False,
            provider_kind="stub",
            model="stub-deterministic",
            limits=LOCAL_RUNNER.RunnerLimits(
                iterations=10,
                max_calls=40,
                max_tokens=120_000,
                timeout_seconds=30.0,
                run_cost_cap_usd=0.25,
                daily_cost_cap_usd=1.0,
                pass_threshold=0.94,
            ),
        )
        local_runner = LOCAL_RUNNER.build_pair(runner_config)[0]

        for scorecard in (langgraph, native, local_runner):
            with self.subTest(scenario_id=scorecard["scenario_id"]):
                scenario = scenarios[scorecard["scenario_id"]]
                contract = scorecard["comparison_contract"]
                self.assertEqual(scenario["scenario_digest"], contract["scenario_digest"])
                self.assertEqual(scenario["task_digest"], contract["task_digest"])
                self.assertEqual(scenario["quality"]["method"], contract["quality_method"])
                self.assertEqual(scenario["quality"]["threshold"], contract["quality_threshold"])

    def test_registry_rejects_duplicate_scenarios(self) -> None:
        tampered = copy.deepcopy(self.raw_registry)
        tampered["scenarios"][1]["scenario_id"] = tampered["scenarios"][0]["scenario_id"]
        tampered["registry_sha256"] = MODULE.registry_sha256(tampered)

        with self.assertRaisesRegex(MODULE.RegressionRegistryError, "unique"):
            MODULE.validate_registry(tampered)

    def test_registry_rejects_role_or_safety_boundary_changes(self) -> None:
        for mutate, expected in (
            (lambda value: value["scenarios"][0]["evidence_roles"].pop("baseline"), "evidence_roles"),
            (lambda value: value["scenarios"][0]["comparison_metadata"].update({"report_only": False}), "report_only"),
            (lambda value: value["scenarios"][0]["comparison_metadata"].update({"provider_calls": "enabled"}), "provider_calls"),
        ):
            with self.subTest(expected=expected):
                tampered = copy.deepcopy(self.raw_registry)
                mutate(tampered)
                tampered["registry_sha256"] = MODULE.registry_sha256(tampered)
                with self.assertRaisesRegex(MODULE.RegressionRegistryError, expected):
                    MODULE.validate_registry(tampered)

    def test_registry_rejects_sensitive_or_unbounded_content(self) -> None:
        tampered = copy.deepcopy(self.raw_registry)
        tampered["scenarios"][0]["comparison_metadata"]["raw_prompt"] = "not allowed"
        tampered["registry_sha256"] = MODULE.registry_sha256(tampered)

        with self.assertRaisesRegex(MODULE.RegressionRegistryError, "raw or sensitive"):
            MODULE.validate_registry(tampered)


if __name__ == "__main__":
    unittest.main()
