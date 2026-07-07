from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = ROOT / "scripts" / "native_stateful_experiment_pilot.py"
VALIDATOR_PATH = ROOT / "scripts" / "token_efficiency_scorecard.py"


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


MODULE = load_module("native_stateful_experiment_pilot", SCRIPT_PATH)
VALIDATOR = load_module("token_efficiency_scorecard_for_native_pilot_tests", VALIDATOR_PATH)


class NativeStatefulExperimentPilotTests(unittest.TestCase):
    def test_stateless_and_stateful_scorecards_validate(self) -> None:
        stateless, stateful = MODULE.build_pair(iterations=10)

        self.assertEqual(stateless, VALIDATOR.import_scorecard(stateless))
        self.assertEqual(stateful, VALIDATOR.import_scorecard(stateful))
        self.assertEqual(stateless["scenario_id"], stateful["scenario_id"])
        self.assertEqual(stateless["mode"], "stateless_reread")
        self.assertEqual(stateful["mode"], "stateful_store")
        self.assertEqual(stateless["state_strategy"], "full_history")
        self.assertEqual(stateful["state_strategy"], "durable_state")

    def test_stateful_uses_fewer_tokens_and_repeated_context(self) -> None:
        stateless, stateful = MODULE.build_pair(iterations=10)

        self.assertEqual(stateless["status"], "pass")
        self.assertEqual(stateful["status"], "pass")
        self.assertNotEqual(stateless["quality_method"], "none")
        self.assertNotEqual(stateful["quality_method"], "none")
        self.assertLess(
            stateful["derived_metrics"]["total_tokens"],
            stateless["derived_metrics"]["total_tokens"],
        )
        self.assertLess(
            stateful["derived_metrics"]["repeated_context_ratio"],
            stateless["derived_metrics"]["repeated_context_ratio"],
        )

    def test_comparison_reports_token_reduction_ratio(self) -> None:
        comparison = MODULE.build_comparison(iterations=10)

        self.assertTrue(comparison["read_only"])
        self.assertEqual(comparison["scenario_id"], MODULE.SCENARIO_ID)
        self.assertEqual(comparison["baseline_mode"], "stateless_reread")
        self.assertIn("token_reduction_ratio", comparison["fields"])
        self.assertEqual(comparison["rows"][0]["mode"], "stateless_reread")
        self.assertEqual(comparison["rows"][1]["mode"], "stateful_store")
        self.assertEqual(comparison["rows"][0]["token_reduction_ratio"], 0.0)
        self.assertGreater(comparison["rows"][1]["token_reduction_ratio"], 0.0)

    def test_failed_runs_have_null_passing_metrics(self) -> None:
        stateless, stateful = MODULE.build_pair(iterations=4, status="fail")

        for scorecard in [stateless, stateful]:
            self.assertEqual(scorecard["status"], "fail")
            self.assertIsNone(scorecard["derived_metrics"]["tokens_per_passing_run"])
            self.assertIsNone(scorecard["derived_metrics"]["cost_per_passing_run"])

    def test_rejects_raw_and_sensitive_shapes(self) -> None:
        summary: dict[str, Any] = MODULE.build_bounded_summary("stateful_store")
        summary["raw" + "_prompt"] = "do not keep this"
        with self.assertRaisesRegex(VALIDATOR.ScorecardError, "raw or sensitive"):
            VALIDATOR.import_scorecard(summary)

        summary = MODULE.build_bounded_summary("stateful_store")
        summary["private" + "_path"] = "hidden"
        with self.assertRaisesRegex(VALIDATOR.ScorecardError, "raw or sensitive"):
            VALIDATOR.import_scorecard(summary)

    def test_cli_output_dir_and_compare(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "pilot"
            comparison_path = Path(tmp) / "comparison-only.json"

            self.assertEqual(MODULE.main(["--output-dir", str(output_dir), "--iterations", "6"]), 0)
            stateless = json.loads((output_dir / "stateless_reread.scorecard.json").read_text(encoding="utf-8"))
            stateful = json.loads((output_dir / "stateful_store.scorecard.json").read_text(encoding="utf-8"))
            comparison = json.loads((output_dir / "comparison.json").read_text(encoding="utf-8"))
            self.assertEqual(stateless["mode"], "stateless_reread")
            self.assertEqual(stateful["mode"], "stateful_store")
            self.assertEqual(len(comparison["rows"]), 2)

            self.assertEqual(
                MODULE.main([
                    "--compare",
                    "--iterations",
                    "6",
                    "--output",
                    str(comparison_path),
                ]),
                0,
            )
            comparison_only = json.loads(comparison_path.read_text(encoding="utf-8"))
            self.assertEqual(comparison_only["pilot_kind"], "native_stateful_vs_stateless_deterministic")

    def test_invalid_iterations_fail_closed(self) -> None:
        with self.assertRaisesRegex(MODULE.NativeStatefulPilotError, "iterations"):
            MODULE.build_pair(iterations=1)


if __name__ == "__main__":
    unittest.main()
