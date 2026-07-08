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
NATIVE_EXPORT_PATH = ROOT / "scripts" / "native_scorecard_export.py"


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
NATIVE_EXPORT = load_module("native_scorecard_export_for_provider_runner_tests", NATIVE_EXPORT_PATH)

FORBIDDEN_ARTIFACT_FRAGMENTS = (
    "raw_prompt",
    "raw_output",
    "transcript",
    "conversation",
    "message_history",
    "credential",
    "repository_text",
    "repo_full_text",
    "repo_content",
    "private_path",
    "secret",
    "password",
    "/home/",
    "/Users/",
    "C:\\Users\\",
)


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
    def _rust_runner_completed_process(self, cmd: list[str], **_kwargs: Any) -> Any:
        output_dir = Path(cmd[cmd.index("--output-dir") + 1])
        iterations = int(cmd[cmd.index("--iterations") + 1])
        stub_config = MODULE.build_config(args(provider="stub", iterations=iterations))
        stateless, stateful = MODULE.build_pair(stub_config)
        output_dir.mkdir(parents=True, exist_ok=True)
        (output_dir / "stateless_reread.scorecard.json").write_text(
            json.dumps(stateless), encoding="utf-8"
        )
        (output_dir / "stateful_store.scorecard.json").write_text(
            json.dumps(stateful), encoding="utf-8"
        )
        return MODULE.subprocess.CompletedProcess(cmd, 0, stdout="", stderr="")

    def assert_artifact_is_bounded_metadata(self, artifact: dict[str, Any]) -> None:
        rendered = json.dumps(artifact, sort_keys=True)
        for fragment in FORBIDDEN_ARTIFACT_FRAGMENTS:
            self.assertNotIn(fragment, rendered)
        self.assertEqual(artifact["schema_version"], NATIVE_EXPORT.EXPORT_SCHEMA_VERSION)
        self.assertEqual(artifact["artifact_kind"], "token_efficiency_scorecard")
        self.assertEqual(artifact["storage"], "app_owned_artifact_json_export")
        self.assertTrue(artifact["read_only"])
        self.assertEqual(artifact["scorecard_schema_version"], VALIDATOR.SCHEMA_VERSION)
        self.assertEqual(len(artifact["content_sha256"]), 64)
        self.assertEqual(artifact["scorecard"], VALIDATOR.import_scorecard(artifact["scorecard"]))

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

    def test_non_stub_provider_skips_binary_check_at_config(self) -> None:
        """Non-stub/live provider config does not check binary existence."""
        config = MODULE.build_config(args(provider="fake"))
        self.assertEqual(config.provider_kind, "fake")
        self.assertFalse(config.live)

        config = MODULE.build_config(args(provider="live"))
        self.assertEqual(config.provider_kind, "live")
        self.assertTrue(config.live)

        config = MODULE.build_config(args(live=True))
        self.assertEqual(config.provider_kind, "stub")
        self.assertTrue(config.live)

    def test_pair_reports_rust_runner_failure_without_binary_requirement(self) -> None:
        """Live delegation surfaces Rust failures without resolving a real binary."""
        config = MODULE.build_config(args(provider="live"))
        with patch.object(MODULE, "_find_rust_binary", return_value=Path("/tmp/local-runner-exec")):
            with patch.object(
                MODULE.subprocess,
                "run",
                return_value=MODULE.subprocess.CompletedProcess(
                    ["/tmp/local-runner-exec"], 1, stdout="", stderr="gated failure"
                ),
            ):
                with self.assertRaises(MODULE.ProviderGatedRunnerError) as ctx:
                    MODULE.build_pair(config)
        self.assertIn("Rust runner failed", str(ctx.exception))
        self.assertIn("gated failure", str(ctx.exception))

    def test_non_stub_delegation_mocks_rust_binary_and_subprocess(self) -> None:
        """Non-stub/live tests do not require target/debug/local-runner-exec in CI."""
        config = MODULE.build_config(args(provider="live", iterations=6))
        with patch.object(MODULE, "_find_rust_binary", return_value=Path("/tmp/local-runner-exec")) as find_binary:
            with patch.object(
                MODULE.subprocess,
                "run",
                side_effect=self._rust_runner_completed_process,
            ) as run:
                stateless, stateful = MODULE.build_pair(config)

        find_binary.assert_called_once_with()
        run.assert_called_once()
        command = run.call_args.args[0]
        self.assertEqual(command[0], "/tmp/local-runner-exec")
        self.assertIn("--provider", command)
        self.assertEqual(command[command.index("--provider") + 1], "live")
        self.assertEqual(stateless["mode"], "stateless_reread")
        self.assertEqual(stateful["mode"], "stateful_store")

    def test_fake_config_matches_rust_binary_choices(self) -> None:
        """Provider choices align with Rust local-runner-exec."""
        for kind in ("stub", "fake", "live"):
            config = MODULE.build_config(args(provider=kind))
            self.assertEqual(config.provider_kind, kind)
            self.assertTrue(config.model.endswith("-deterministic") or config.model == "live-provider")

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

    def test_cli_output_dir_can_emit_importable_artifact_envelopes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "runner"

            self.assertEqual(
                MODULE.main([
                    "--output-dir",
                    str(output_dir),
                    "--iterations",
                    "6",
                    "--emit-artifacts",
                ]),
                0,
            )

            stateless_scorecard = json.loads((output_dir / "stateless_reread.scorecard.json").read_text(encoding="utf-8"))
            stateful_scorecard = json.loads((output_dir / "stateful_store.scorecard.json").read_text(encoding="utf-8"))
            stateless_artifact = json.loads((output_dir / "stateless_reread.artifact.json").read_text(encoding="utf-8"))
            stateful_artifact = json.loads((output_dir / "stateful_store.artifact.json").read_text(encoding="utf-8"))

            self.assertEqual(stateless_artifact["scorecard"], stateless_scorecard)
            self.assertEqual(stateful_artifact["scorecard"], stateful_scorecard)
            self.assert_artifact_is_bounded_metadata(stateless_artifact)
            self.assert_artifact_is_bounded_metadata(stateful_artifact)
            self.assertLess(
                stateful_artifact["scorecard"]["derived_metrics"]["total_tokens"],
                stateless_artifact["scorecard"]["derived_metrics"]["total_tokens"],
            )
            self.assertLess(
                stateful_artifact["scorecard"]["derived_metrics"]["repeated_context_ratio"],
                stateless_artifact["scorecard"]["derived_metrics"]["repeated_context_ratio"],
            )


if __name__ == "__main__":
    unittest.main()
