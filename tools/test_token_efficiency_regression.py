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
NATIVE_EXPORT = load_script(
    "pe1_native_scorecard_export",
    ROOT / "scripts" / "native_scorecard_export.py",
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


class TokenEfficiencyRegressionReportTests(unittest.TestCase):
    def setUp(self) -> None:
        self.registry = MODULE.load_registry(FIXTURE_PATH)
        fixture_dir = ROOT / "tests" / "fixtures" / "langgraph_pilot"
        self.baseline_artifact = json.loads(
            (fixture_dir / "stateless_reread.artifact.json").read_text(encoding="utf-8")
        )
        self.best_known_artifact = json.loads(
            (fixture_dir / "stateful_store.artifact.json").read_text(encoding="utf-8")
        )
        self.scenario_id = self.best_known_artifact["scorecard"]["scenario_id"]

    def current_scorecard(self) -> dict:
        current = copy.deepcopy(self.best_known_artifact["scorecard"])
        current["adapter_run_id"] = "lg-current-stateful"
        current.pop("derived_metrics", None)
        return current

    def build_report(self, current=None, baseline=None, best_known=None):
        return MODULE.build_regression_report(
            self.registry,
            self.scenario_id,
            current=self.current_scorecard() if current is None else current,
            baseline=self.baseline_artifact if baseline is None else baseline,
            best_known=self.best_known_artifact if best_known is None else best_known,
        )

    def test_report_is_deterministic_read_only_and_compares_both_references(self) -> None:
        report = self.build_report()
        repeated = self.build_report()

        self.assertEqual(report, repeated)
        self.assertEqual(report["schema_version"], "token_efficiency_regression_report.v1")
        self.assertEqual(report["outcome"], "pass")
        self.assertEqual(report["reason_codes"], [])
        self.assertTrue(report["read_only"])
        self.assertTrue(report["report_only"])
        self.assertEqual(report["provider_calls"], "disabled")
        self.assertEqual(report["mutation_authority"], "none")
        self.assertEqual(len(report["report_sha256"]), 64)
        self.assertEqual(set(report["comparisons"]), {"baseline", "best_known"})
        self.assertEqual(report["comparisons"]["baseline"]["metrics"]["total_tokens"]["reference"], 38_452)
        self.assertEqual(report["comparisons"]["best_known"]["metrics"]["total_tokens"]["reference"], 11_294)
        self.assertFalse(report["comparisons"]["best_known"]["metrics"]["total_tokens"]["regressed"])

    def test_report_returns_missing_baseline_without_comparing(self) -> None:
        report = MODULE.build_regression_report(
            self.registry,
            self.scenario_id,
            current=self.current_scorecard(),
            baseline=None,
            best_known=self.best_known_artifact,
        )

        self.assertEqual(report["outcome"], "missing_baseline")
        self.assertEqual(report["reason_codes"], ["missing_baseline"])
        self.assertEqual(report["comparisons"], {})

    def test_report_returns_incomparable_for_contract_drift(self) -> None:
        current = self.current_scorecard()
        current["runtime_version"] = "different-runtime-version"
        current["comparison_contract"]["runtime_version"] = "different-runtime-version"

        report = self.build_report(current=current)

        self.assertEqual(report["outcome"], "incomparable")
        self.assertIn("current.runtime_version_mismatch", report["reason_codes"])
        self.assertEqual(report["comparisons"], {})

    def test_report_returns_quality_failure_before_advantage_claims(self) -> None:
        current = self.current_scorecard()
        current["quality_score"] = 0.5

        report = self.build_report(current=current)

        self.assertEqual(report["outcome"], "quality_failure")
        self.assertEqual(report["reason_codes"], ["current_quality_below_threshold"])
        self.assertEqual(report["comparisons"], {})

    def test_report_applies_allowed_regression_thresholds(self) -> None:
        within = self.current_scorecard()
        within["input_token_total"] += 500
        regressed = self.current_scorecard()
        regressed["input_token_total"] += 600

        passing_report = self.build_report(current=within)
        regression_report = self.build_report(current=regressed)

        self.assertEqual(passing_report["outcome"], "pass")
        self.assertEqual(regression_report["outcome"], "regression")
        self.assertEqual(regression_report["reason_codes"], ["best_known.total_tokens"])
        metric = regression_report["comparisons"]["best_known"]["metrics"]["total_tokens"]
        self.assertGreater(metric["normalized_regression"], metric["allowed_regression"])

    def test_report_accepts_native_v1_and_generic_v2_envelopes(self) -> None:
        baseline_v1 = NATIVE_EXPORT.build_artifact(self.baseline_artifact["scorecard"])
        current_v2 = MODULE.VALIDATOR.build_scorecard_artifact(
            self.current_scorecard(), created_at="2026-07-11T00:00:00Z"
        )

        report = self.build_report(current=current_v2, baseline=baseline_v1)

        self.assertEqual(report["outcome"], "pass")
        self.assertEqual(report["evidence"]["baseline"]["artifact_schema_version"], "native_scorecard_artifact.v1")
        self.assertEqual(report["evidence"]["current"]["artifact_schema_version"], "scorecard_artifact.v2")

    def test_report_hash_rejects_tampering(self) -> None:
        report = self.build_report()
        report["outcome"] = "regression"

        with self.assertRaisesRegex(MODULE.RegressionRegistryError, "report_sha256"):
            MODULE.validate_regression_report(report)

    def test_report_rejects_sensitive_artifact_envelope_fields(self) -> None:
        current = copy.deepcopy(self.best_known_artifact)
        current["raw_prompt"] = "must never be accepted"

        with self.assertRaisesRegex(MODULE.RegressionRegistryError, "raw or sensitive"):
            self.build_report(current=current)


if __name__ == "__main__":
    unittest.main()
