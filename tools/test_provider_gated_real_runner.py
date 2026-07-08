from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = ROOT / "scripts" / "provider_gated_real_runner.py"
VALIDATOR_PATH = ROOT / "scripts" / "token_efficiency_scorecard.py"


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


MODULE = load_module("provider_gated_real_runner", SCRIPT_PATH)
VALIDATOR = load_module("token_efficiency_scorecard_for_provider_runner_tests", VALIDATOR_PATH)


def args(**overrides: Any) -> argparse.Namespace:
    values = {
        "provider": "stub",
        "live": False,
        "iterations": 10,
        "max_calls": 40,
        "max_tokens": 120000,
        "timeout_seconds": 30.0,
        "run_cost_cap_usd": 0.25,
        "daily_cost_cap_usd": 1.0,
        "pass_threshold": 0.94,
        "output_dir": None,
        "compare": False,
        "output": None,
        "compact": False,
    }
    values.update(overrides)
    return argparse.Namespace(**values)


class ProviderGatedRealRunnerTests(unittest.TestCase):
    def test_stub_scorecards_validate_and_compare(self) -> None:
        config = MODULE.build_config(args())
        stateless, stateful = MODULE.build_pair(config)

        self.assertEqual(stateless, VALIDATOR.import_scorecard(stateless))
        self.assertEqual(stateful, VALIDATOR.import_scorecard(stateful))
        self.assertEqual(stateless["scenario_id"], stateful["scenario_id"])
        self.assertEqual(stateless["mode"], "stateless_reread")
        self.assertEqual(stateful["mode"], "stateful_store")
        self.assertEqual(stateless["state_strategy"], "full_history")
        self.assertEqual(stateful["state_strategy"], "durable_state")

    def test_stateful_uses_less_context_and_repeated_context(self) -> None:
        config = MODULE.build_config(args(iterations=10))
        stateless, stateful = MODULE.build_pair(config)

        self.assertEqual(stateless["status"], "pass")
        self.assertEqual(stateful["status"], "pass")
        self.assertLess(
            stateful["derived_metrics"]["total_tokens"],
            stateless["derived_metrics"]["total_tokens"],
        )
        self.assertLess(
            stateful["derived_metrics"]["repeated_context_ratio"],
            stateless["derived_metrics"]["repeated_context_ratio"],
        )

    def test_comparison_is_read_only_and_reports_reduction(self) -> None:
        config = MODULE.build_config(args(iterations=8))
        comparison = MODULE.build_comparison(config)

        self.assertTrue(comparison["read_only"])
        self.assertEqual(comparison["scenario_id"], MODULE.SCENARIO_ID)
        self.assertEqual(comparison["baseline_mode"], "stateless_reread")
        self.assertEqual(comparison["rows"][0]["mode"], "stateless_reread")
        self.assertEqual(comparison["rows"][1]["mode"], "stateful_store")
        self.assertGreater(comparison["rows"][1]["token_reduction_ratio"], 0.0)

    def test_live_and_non_stub_provider_resolves_rust_binary(self) -> None:
        """Non-stub/live provider delegates to the Rust local-runner-exec binary."""
        config = MODULE.build_config(args(provider="openai_compatible"), env={})
        self.assertTrue(config.live or config.provider_kind != "stub")

        config = MODULE.build_config(args(provider="openai_compatible", live=True), env={})
        self.assertTrue(config.live)

        config = MODULE.build_config(args(live=True), env={})
        self.assertTrue(config.live)

    def test_limits_fail_closed(self) -> None:
        with self.assertRaisesRegex(MODULE.ProviderGatedRunnerError, "iterations"):
            MODULE.build_config(args(iterations=1))
        with self.assertRaisesRegex(MODULE.ProviderGatedRunnerError, "max calls"):
            MODULE.build_config(args(iterations=10, max_calls=1))
        with self.assertRaisesRegex(MODULE.ProviderGatedRunnerError, "cost"):
            MODULE.build_config(args(run_cost_cap_usd=2.0, daily_cost_cap_usd=1.0))

    def test_runtime_kill_switch_checked_during_run(self) -> None:
        config = MODULE.build_config(args())
        with patch.dict(os.environ, {"ACP_REAL_RUNNER_KILL_SWITCH": "1"}, clear=False):
            with self.assertRaisesRegex(MODULE.ProviderGatedRunnerError, "kill switch"):
                MODULE.run_mode("stateful_store", config, MODULE.StubProvider())

    def test_cli_output_dir_and_compare(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "runner"
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
            self.assertEqual(comparison_only["runner_kind"], "provider_gated_real_runner")


if __name__ == "__main__":
    unittest.main()
