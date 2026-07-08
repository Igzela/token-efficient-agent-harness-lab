#!/usr/bin/env python3
from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]


def _load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


RUNNER = _load_module("provider_gated_real_runner", ROOT / "scripts" / "provider_gated_real_runner.py")
VALIDATOR = _load_module("token_efficiency_scorecard_for_local_runner_validation", ROOT / "scripts" / "token_efficiency_scorecard.py")
COMPARISON = _load_module("scorecard_comparison_for_local_runner_validation", ROOT / "scripts" / "scorecard_comparison.py")


class LocalRunnerValidationError(ValueError):
    pass


@dataclass(frozen=True)
class ValidationResult:
    output_dir: Path
    stateless_total_tokens: int
    stateful_total_tokens: int
    token_reduction_ratio: float
    stateless_repeated_context_ratio: float
    stateful_repeated_context_ratio: float


def _read_json(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise LocalRunnerValidationError(f"cannot read JSON output: {path}") from exc
    if not isinstance(data, dict):
        raise LocalRunnerValidationError(f"expected JSON object at {path}")
    return data


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise LocalRunnerValidationError(message)


def validate_output_dir(output_dir: Path) -> ValidationResult:
    stateless_path = output_dir / "stateless_reread.scorecard.json"
    stateful_path = output_dir / "stateful_store.scorecard.json"
    comparison_path = output_dir / "comparison.json"

    for path in (stateless_path, stateful_path, comparison_path):
        _require(path.exists(), f"missing runner output: {path}")
        _require(path.is_file(), f"runner output is not a file: {path}")

    stateless = VALIDATOR.import_scorecard(_read_json(stateless_path))
    stateful = VALIDATOR.import_scorecard(_read_json(stateful_path))
    comparison = _read_json(comparison_path)
    recomputed = COMPARISON.compare_scorecards([stateless, stateful])

    _require(stateless["scenario_id"] == stateful["scenario_id"], "scorecards must share scenario_id")
    _require(stateless["mode"] == "stateless_reread", "baseline mode must be stateless_reread")
    _require(stateful["mode"] == "stateful_store", "candidate mode must be stateful_store")
    _require(stateless["status"] == "pass", "stateless run must pass")
    _require(stateful["status"] == "pass", "stateful run must pass")
    _require(comparison.get("read_only") is True, "comparison must be read-only evidence")
    _require(comparison.get("scenario_id") == stateless["scenario_id"], "comparison scenario_id mismatch")
    _require(comparison.get("baseline_mode") == "stateless_reread", "comparison baseline mismatch")
    _require(comparison.get("rows") == recomputed.get("rows"), "comparison rows differ from recomputed output")
    _require(comparison.get("runner_kind") == "provider_gated_real_runner", "comparison runner_kind mismatch")

    stateless_total = int(stateless["derived_metrics"]["total_tokens"])
    stateful_total = int(stateful["derived_metrics"]["total_tokens"])
    stateless_repeated = float(stateless["derived_metrics"]["repeated_context_ratio"])
    stateful_repeated = float(stateful["derived_metrics"]["repeated_context_ratio"])
    rows = comparison["rows"]
    _require(len(rows) == 2, "comparison must contain exactly two rows")
    token_reduction = float(rows[1]["token_reduction_ratio"])

    _require(stateful_total < stateless_total, "stateful runner must use fewer total tokens")
    _require(stateful_repeated < stateless_repeated, "stateful runner must use less repeated context")
    _require(token_reduction > 0.0, "stateful runner must report positive token reduction")

    return ValidationResult(
        output_dir=output_dir,
        stateless_total_tokens=stateless_total,
        stateful_total_tokens=stateful_total,
        token_reduction_ratio=token_reduction,
        stateless_repeated_context_ratio=stateless_repeated,
        stateful_repeated_context_ratio=stateful_repeated,
    )


def run_validation(output_dir: Path, iterations: int, keep_output: bool) -> ValidationResult:
    if output_dir.exists():
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    code = RUNNER.main(["--output-dir", str(output_dir), "--iterations", str(iterations), "--compact"])
    _require(code == 0, "local runner exited with non-zero status")
    result = validate_output_dir(output_dir)
    if not keep_output:
        shutil.rmtree(output_dir)
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Validate local stateful-vs-stateless runner outputs.")
    parser.add_argument("--iterations", type=int, default=10)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--keep-output", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    try:
        if args.output_dir is None:
            with tempfile.TemporaryDirectory(prefix="acp-local-runner-") as tmp:
                result = run_validation(Path(tmp) / "runner", args.iterations, keep_output=False)
        else:
            result = run_validation(args.output_dir, args.iterations, keep_output=args.keep_output)

        summary = {
            "status": "pass",
            "stateless_total_tokens": result.stateless_total_tokens,
            "stateful_total_tokens": result.stateful_total_tokens,
            "token_reduction_ratio": result.token_reduction_ratio,
            "stateless_repeated_context_ratio": result.stateless_repeated_context_ratio,
            "stateful_repeated_context_ratio": result.stateful_repeated_context_ratio,
        }
        if args.json:
            print(json.dumps(summary, sort_keys=True))
        else:
            print(
                "local runner validation passed: "
                f"stateful tokens {result.stateful_total_tokens} < stateless tokens {result.stateless_total_tokens}; "
                f"token reduction {result.token_reduction_ratio:.3f}"
            )
        return 0
    except (LocalRunnerValidationError, RUNNER.ProviderGatedRunnerError, VALIDATOR.ScorecardError, COMPARISON.ScorecardComparisonError) as exc:
        print(f"local runner validation failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
