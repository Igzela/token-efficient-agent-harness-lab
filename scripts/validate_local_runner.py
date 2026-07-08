#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import shutil
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
NATIVE_SCORECARD_ARTIFACT_SCHEMA_VERSION = "native_scorecard_artifact.v1"
TOKEN_EFFICIENCY_SCORECARD_SCHEMA_VERSION = "token_efficiency_scorecard.v1"
DEFAULT_ARTIFACT_CREATED_AT = "1970-01-01T00:00:00Z"


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
    artifact_paths: tuple[Path, ...] = ()


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


def _canonical_json(value: dict[str, Any]) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def build_scorecard_artifact(scorecard: dict[str, Any], *, created_at: str = DEFAULT_ARTIFACT_CREATED_AT) -> dict[str, Any]:
    normalized = VALIDATOR.import_scorecard(scorecard)
    canonical = _canonical_json(normalized)
    content_sha256 = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
    run_id = normalized["adapter_run_id"]
    return {
        "schema_version": NATIVE_SCORECARD_ARTIFACT_SCHEMA_VERSION,
        "artifact_kind": "token_efficiency_scorecard",
        "storage": "app_owned_artifact_json_export",
        "read_only": True,
        "created_at": created_at,
        "artifact_id": f"scorecard-{run_id}-{content_sha256[:12]}",
        "content_sha256": content_sha256,
        "scorecard_schema_version": TOKEN_EFFICIENCY_SCORECARD_SCHEMA_VERSION,
        "scorecard": normalized,
        "metadata_only": True,
        "target_repository_writes": "disabled",
    }


def write_scorecard_artifacts(output_dir: Path, artifact_dir: Path) -> tuple[Path, ...]:
    stateless = VALIDATOR.import_scorecard(_read_json(output_dir / "stateless_reread.scorecard.json"))
    stateful = VALIDATOR.import_scorecard(_read_json(output_dir / "stateful_store.scorecard.json"))
    artifacts = [
        ("stateless_reread.artifact.json", build_scorecard_artifact(stateless)),
        ("stateful_store.artifact.json", build_scorecard_artifact(stateful)),
    ]
    if artifact_dir.exists():
        shutil.rmtree(artifact_dir)
    artifact_dir.mkdir(parents=True, exist_ok=True)
    paths: list[Path] = []
    for filename, artifact in artifacts:
        path = artifact_dir / filename
        path.write_text(_canonical_json(artifact) + "\n", encoding="utf-8")
        paths.append(path)
    return tuple(paths)


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


def run_validation(output_dir: Path, iterations: int, keep_output: bool, artifact_dir: Path | None = None) -> ValidationResult:
    if output_dir.exists():
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    code = RUNNER.main(["--output-dir", str(output_dir), "--iterations", str(iterations), "--compact"])
    _require(code == 0, "local runner exited with non-zero status")
    result = validate_output_dir(output_dir)
    artifact_paths: tuple[Path, ...] = ()
    if artifact_dir is not None:
        artifact_paths = write_scorecard_artifacts(output_dir, artifact_dir)
    if not keep_output:
        shutil.rmtree(output_dir)
    return ValidationResult(
        output_dir=result.output_dir,
        stateless_total_tokens=result.stateless_total_tokens,
        stateful_total_tokens=result.stateful_total_tokens,
        token_reduction_ratio=result.token_reduction_ratio,
        stateless_repeated_context_ratio=result.stateless_repeated_context_ratio,
        stateful_repeated_context_ratio=result.stateful_repeated_context_ratio,
        artifact_paths=artifact_paths,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Validate local stateful-vs-stateless runner outputs.")
    parser.add_argument("--iterations", type=int, default=10)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--artifact-dir", type=Path)
    parser.add_argument("--keep-output", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    try:
        if args.output_dir is None:
            with tempfile.TemporaryDirectory(prefix="acp-local-runner-") as tmp:
                result = run_validation(
                    Path(tmp) / "runner",
                    args.iterations,
                    keep_output=False,
                    artifact_dir=args.artifact_dir,
                )
        else:
            result = run_validation(
                args.output_dir,
                args.iterations,
                keep_output=args.keep_output,
                artifact_dir=args.artifact_dir,
            )

        summary = {
            "status": "pass",
            "stateless_total_tokens": result.stateless_total_tokens,
            "stateful_total_tokens": result.stateful_total_tokens,
            "token_reduction_ratio": result.token_reduction_ratio,
            "stateless_repeated_context_ratio": result.stateless_repeated_context_ratio,
            "stateful_repeated_context_ratio": result.stateful_repeated_context_ratio,
            "artifact_count": len(result.artifact_paths),
        }
        if args.json:
            print(json.dumps(summary, sort_keys=True))
        else:
            artifact_suffix = f"; emitted {len(result.artifact_paths)} storage artifacts" if result.artifact_paths else ""
            print(
                "local runner validation passed: "
                f"stateful tokens {result.stateful_total_tokens} < stateless tokens {result.stateless_total_tokens}; "
                f"token reduction {result.token_reduction_ratio:.3f}"
                f"{artifact_suffix}"
            )
        return 0
    except (LocalRunnerValidationError, RUNNER.ProviderGatedRunnerError, VALIDATOR.ScorecardError, COMPARISON.ScorecardComparisonError) as exc:
        print(f"local runner validation failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
