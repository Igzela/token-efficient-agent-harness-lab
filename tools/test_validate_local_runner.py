from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = ROOT / "scripts" / "validate_local_runner.py"
RUNNER_PATH = ROOT / "scripts" / "provider_gated_real_runner.py"


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


VALIDATION = load_module("validate_local_runner", SCRIPT_PATH)
RUNNER = load_module("provider_gated_real_runner_for_validation_tests", RUNNER_PATH)


class ValidateLocalRunnerTests(unittest.TestCase):
    def test_run_validation_writes_and_validates_outputs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "runner"
            result = VALIDATION.run_validation(output_dir, iterations=10, keep_output=True)

            self.assertTrue((output_dir / "stateless_reread.scorecard.json").exists())
            self.assertTrue((output_dir / "stateful_store.scorecard.json").exists())
            self.assertTrue((output_dir / "comparison.json").exists())
            self.assertLess(result.stateful_total_tokens, result.stateless_total_tokens)
            self.assertGreater(result.token_reduction_ratio, 0.0)

    def test_run_validation_cleans_outputs_by_default(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "runner"
            result = VALIDATION.run_validation(output_dir, iterations=8, keep_output=False)

            self.assertGreater(result.token_reduction_ratio, 0.0)
            self.assertFalse(output_dir.exists())

    def test_validate_output_rejects_tampered_comparison(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "runner"
            config_args = ["--output-dir", str(output_dir), "--iterations", "8"]
            self.assertEqual(RUNNER.main(config_args), 0)

            comparison_path = output_dir / "comparison.json"
            comparison = json.loads(comparison_path.read_text(encoding="utf-8"))
            comparison["rows"][1]["token_reduction_ratio"] = 0.0
            comparison_path.write_text(json.dumps(comparison), encoding="utf-8")

            with self.assertRaisesRegex(VALIDATION.LocalRunnerValidationError, "comparison rows"):
                VALIDATION.validate_output_dir(output_dir)

    def test_main_json_summary(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "runner"
            with patch("builtins.print") as mocked_print:
                self.assertEqual(
                    VALIDATION.main([
                        "--output-dir",
                        str(output_dir),
                        "--iterations",
                        "8",
                        "--keep-output",
                        "--json",
                    ]),
                    0,
                )
            printed = mocked_print.call_args.args[0]
            summary = json.loads(printed)
            self.assertEqual(summary["status"], "pass")
            self.assertGreater(summary["token_reduction_ratio"], 0.0)
            self.assertTrue(output_dir.exists())

    def test_main_returns_nonzero_on_runner_failure(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "runner"
            self.assertEqual(
                VALIDATION.main([
                    "--output-dir",
                    str(output_dir),
                    "--iterations",
                    "1",
                ]),
                1,
            )


if __name__ == "__main__":
    unittest.main()
