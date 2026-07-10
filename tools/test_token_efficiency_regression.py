from __future__ import annotations

import argparse
import copy
import importlib.util
import json
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = ROOT / "scripts" / "token_efficiency_regression.py"
FIXTURE_PATH = ROOT / "tests" / "fixtures" / "token_efficiency_regression" / "registry.json"
EVIDENCE_ROOT = FIXTURE_PATH.parent
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


def fixed_evidence_paths(scenario_id: str) -> tuple[Path, Path]:
    scenario_dir = (
        ROOT / "tests" / "fixtures" / "langgraph_pilot"
        if scenario_id == "langgraph_offline_state_retention_pilot_2026_07_10"
        else EVIDENCE_ROOT / scenario_id
    )
    return (
        scenario_dir / "stateless_reread.artifact.json",
        scenario_dir / "stateful_store.artifact.json",
    )


def fixed_evidence(scenario_id: str) -> tuple[dict, dict]:
    baseline_path, candidate_path = fixed_evidence_paths(scenario_id)
    return (
        json.loads(baseline_path.read_text(encoding="utf-8")),
        json.loads(candidate_path.read_text(encoding="utf-8")),
    )


def local_stub_config():
    return LOCAL_RUNNER.build_config(
        argparse.Namespace(
            iterations=10,
            max_calls=40,
            max_tokens=120_000,
            timeout_seconds=30.0,
            run_cost_cap_usd=0.25,
            daily_cost_cap_usd=1.0,
            pass_threshold=0.94,
            live=False,
            provider="stub",
        )
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

    def test_all_registered_scenarios_have_checked_fixed_evidence_pairs(self) -> None:
        regenerated = {
            NATIVE_PILOT.SCENARIO_ID: NATIVE_PILOT.build_pair(),
            LOCAL_RUNNER.SCENARIO_ID: LOCAL_RUNNER.build_pair(local_stub_config()),
        }
        expected_reports = {
            "langgraph_offline_state_retention_pilot_2026_07_10": (
                "pass",
                [],
                "d2090f6cb5b9507a3d51d414d04ea2e1d81ba047b297396a57ad0cc22af1063a",
            ),
            NATIVE_PILOT.SCENARIO_ID: (
                "regression",
                ["baseline.state_bytes"],
                "aba0481c81461eb64b68187a0a8819a2191971f26b61be5f13c38675ab1dcee9",
            ),
            LOCAL_RUNNER.SCENARIO_ID: (
                "regression",
                ["baseline.state_bytes"],
                "c0281ba653d7f6d5f5efe40de109008378ec639503adcabeedde37ff486681ea",
            ),
        }

        for scenario in self.registry["scenarios"]:
            scenario_id = scenario["scenario_id"]
            baseline_path, candidate_path = fixed_evidence_paths(scenario_id)
            with self.subTest(scenario_id=scenario_id):
                baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
                candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
                self.assertEqual(baseline["scorecard"]["mode"], "stateless_reread")
                self.assertEqual(candidate["scorecard"]["mode"], "stateful_store")
                self.assertEqual(
                    baseline["scorecard"]["comparison_contract"]["task_digest"],
                    scenario["task_digest"],
                )
                self.assertEqual(
                    candidate["scorecard"]["comparison_contract"]["task_digest"],
                    scenario["task_digest"],
                )
                report = MODULE.build_regression_report(
                    self.registry,
                    scenario_id,
                    current=candidate,
                    baseline=baseline,
                    best_known=candidate,
                )
                expected_outcome, expected_reasons, expected_hash = expected_reports[scenario_id]
                self.assertEqual(report["outcome"], expected_outcome)
                self.assertEqual(report["reason_codes"], expected_reasons)
                self.assertEqual(report["report_sha256"], expected_hash)
                self.assertFalse(
                    any(
                        metric["regressed"]
                        for metric in report["comparisons"]["best_known"]["metrics"].values()
                    )
                )
                self.assertEqual(report, MODULE.validate_regression_report(report))

                if scenario_id in regenerated:
                    expected_baseline, expected_candidate = regenerated[scenario_id]
                    rebuilt_baseline = NATIVE_EXPORT.build_artifact(expected_baseline)
                    rebuilt_candidate = NATIVE_EXPORT.build_artifact(expected_candidate)
                    rebuilt_baseline["created_at"] = baseline["created_at"]
                    rebuilt_candidate["created_at"] = candidate["created_at"]
                    self.assertEqual(baseline, rebuilt_baseline)
                    self.assertEqual(candidate, rebuilt_candidate)


class TokenEfficiencyRegressionBatchTests(unittest.TestCase):
    def setUp(self) -> None:
        self.registry = MODULE.load_registry(FIXTURE_PATH)

    def evidence(self) -> dict[str, dict]:
        result = {}
        for scenario in reversed(self.registry["scenarios"]):
            baseline, candidate = fixed_evidence(scenario["scenario_id"])
            result[scenario["scenario_id"]] = {
                "current": candidate,
                "baseline": baseline,
                "best_known": candidate,
            }
        return result

    def test_batch_is_deterministic_complete_sorted_and_report_only(self) -> None:
        evidence = self.evidence()

        batch = MODULE.build_regression_batch(self.registry, evidence)
        repeated = MODULE.build_regression_batch(
            self.registry, dict(reversed(list(evidence.items())))
        )

        self.assertEqual(batch, repeated)
        self.assertEqual(batch["schema_version"], "token_efficiency_regression_batch.v1")
        self.assertEqual(batch["scenario_count"], 3)
        self.assertEqual(batch["outcome_counts"], {"pass": 1, "regression": 2})
        self.assertEqual(
            batch["batch_sha256"],
            "1a0ae8b187915f7e16bb904af2b2073ef6dafb59197a8568abf34c39c971090a",
        )
        self.assertEqual(
            [report["scenario_id"] for report in batch["reports"]],
            sorted(evidence),
        )
        self.assertTrue(batch["read_only"])
        self.assertTrue(batch["report_only"])
        self.assertEqual(batch["provider_calls"], "disabled")
        self.assertEqual(batch["mutation_authority"], "none")
        self.assertEqual(batch, MODULE.validate_regression_batch(batch))

    def test_batch_requires_exact_registry_coverage(self) -> None:
        evidence = self.evidence()
        evidence.pop(next(iter(evidence)))

        with self.assertRaisesRegex(MODULE.RegressionRegistryError, "coverage"):
            MODULE.build_regression_batch(self.registry, evidence)

        evidence = self.evidence()
        evidence["unknown-scenario"] = evidence[next(iter(evidence))]
        with self.assertRaisesRegex(MODULE.RegressionRegistryError, "coverage"):
            MODULE.build_regression_batch(self.registry, evidence)

    def test_batch_aggregates_missing_baseline_without_dropping_scenario(self) -> None:
        evidence = self.evidence()
        first_scenario = self.registry["scenarios"][0]["scenario_id"]
        evidence[first_scenario]["baseline"] = None

        batch = MODULE.build_regression_batch(self.registry, evidence)

        self.assertEqual(
            batch["outcome_counts"],
            {"missing_baseline": 1, "regression": 2},
        )
        self.assertEqual(len(batch["reports"]), batch["scenario_count"])

    def test_batch_validation_rejects_nested_report_tampering(self) -> None:
        batch = MODULE.build_regression_batch(self.registry, self.evidence())
        batch["reports"][0]["outcome"] = "regression"
        batch["batch_sha256"] = MODULE.regression_batch_sha256(batch)

        with self.assertRaisesRegex(MODULE.RegressionRegistryError, "report_sha256"):
            MODULE.validate_regression_batch(batch)

    def test_batch_hash_rejects_aggregate_tampering(self) -> None:
        batch = MODULE.build_regression_batch(self.registry, self.evidence())
        batch["outcome_counts"] = {"pass": 3}

        with self.assertRaisesRegex(MODULE.RegressionRegistryError, "batch_sha256"):
            MODULE.validate_regression_batch(batch)

    def test_batch_validation_rejects_invalid_nested_scenario_id_cleanly(self) -> None:
        batch = MODULE.build_regression_batch(self.registry, self.evidence())
        report_payload = dict(batch["reports"][0])
        report_payload.pop("report_sha256")
        report_payload["scenario_id"] = 7
        batch["reports"][0] = MODULE._finalize_report(report_payload)
        batch["batch_sha256"] = MODULE.regression_batch_sha256(batch)

        with self.assertRaisesRegex(MODULE.RegressionRegistryError, "scenario_id"):
            MODULE.validate_regression_batch(batch)


if __name__ == "__main__":
    unittest.main()
