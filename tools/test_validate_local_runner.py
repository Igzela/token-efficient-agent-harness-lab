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

    def test_run_validation_can_emit_storage_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "runner"
            artifact_dir = Path(tmp) / "artifacts"
            result = VALIDATION.run_validation(
                output_dir,
                iterations=8,
                keep_output=False,
                artifact_dir=artifact_dir,
            )

            self.assertFalse(output_dir.exists())
            self.assertEqual(len(result.artifact_paths), 2)
            self.assertEqual(
                sorted(path.name for path in result.artifact_paths),
                ["stateful_store.artifact.json", "stateless_reread.artifact.json"],
            )
            for path in result.artifact_paths:
                artifact = json.loads(path.read_text(encoding="utf-8"))
                self.assertEqual(artifact["schema_version"], "native_scorecard_artifact.v1")
                self.assertEqual(artifact["artifact_kind"], "token_efficiency_scorecard")
                self.assertEqual(artifact["scorecard_schema_version"], "token_efficiency_scorecard.v1")
                self.assertEqual(artifact["storage"], "app_owned_artifact_json_export")
                self.assertTrue(artifact["read_only"])
                self.assertTrue(artifact["metadata_only"])
                self.assertEqual(artifact["target_repository_writes"], "disabled")
                self.assertEqual(len(artifact["content_sha256"]), 64)
                self.assertTrue(artifact["artifact_id"].startswith("scorecard-"))
                self.assertEqual(artifact["scorecard"]["redaction_status"], "not_needed")

    def test_build_scorecard_artifact_is_deterministic_for_same_scorecard(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "runner"
            self.assertEqual(RUNNER.main(["--output-dir", str(output_dir), "--iterations", "8"]), 0)
            scorecard = json.loads((output_dir / "stateful_store.scorecard.json").read_text(encoding="utf-8"))

            first = VALIDATION.build_scorecard_artifact(scorecard)
            second = VALIDATION.build_scorecard_artifact(scorecard)

            self.assertEqual(first["artifact_id"], second["artifact_id"])
            self.assertEqual(first["content_sha256"], second["content_sha256"])
            self.assertEqual(first["scorecard"]["mode"], "stateful_store")

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
            self.assertEqual(summary["artifact_count"], 0)
            self.assertTrue(output_dir.exists())

    def test_main_can_emit_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "runner"
            artifact_dir = Path(tmp) / "artifacts"
            with patch("builtins.print") as mocked_print:
                self.assertEqual(
                    VALIDATION.main([
                        "--output-dir",
                        str(output_dir),
                        "--artifact-dir",
                        str(artifact_dir),
                        "--iterations",
                        "8",
                    ]),
                    0,
                )
            printed = mocked_print.call_args.args[0]
            self.assertIn("emitted 2 storage artifacts", printed)
            self.assertTrue((artifact_dir / "stateless_reread.artifact.json").exists())
            self.assertTrue((artifact_dir / "stateful_store.artifact.json").exists())

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
