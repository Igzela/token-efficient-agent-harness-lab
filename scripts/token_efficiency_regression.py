#!/usr/bin/env python3
"""Validate the report-only PE-1 token-efficiency scenario registry.

The registry describes bounded summary comparisons. This module does not run
providers, mutate policy, block CI, persist artifacts, or read target repos.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
VALIDATOR_PATH = ROOT / "scripts" / "token_efficiency_scorecard.py"
VALIDATOR_SPEC = importlib.util.spec_from_file_location("token_efficiency_scorecard", VALIDATOR_PATH)
assert VALIDATOR_SPEC is not None
VALIDATOR = importlib.util.module_from_spec(VALIDATOR_SPEC)
sys.modules.setdefault(VALIDATOR_SPEC.name, VALIDATOR)
assert VALIDATOR_SPEC.loader is not None
VALIDATOR_SPEC.loader.exec_module(VALIDATOR)

SCHEMA_VERSION = "token_efficiency_regression_registry.v1"
REQUIRED_METRICS = {
    "total_tokens",
    "repeated_context_ratio",
    "state_bytes",
    "estimated_cost_usd",
    "duration_ms",
    "retry_count",
    "quality_score",
}
SUPPORTED_ARTIFACT_SCHEMAS = {
    "native_scorecard_artifact.v1",
    "scorecard_artifact.v2",
}
IDENTIFIER = re.compile(r"^[a-z0-9][a-z0-9_-]{0,127}$")
HEX_DIGEST = re.compile(r"^[0-9a-f]{64}$")


class RegressionRegistryError(ValueError):
    """Raised when a PE-1 registry violates its bounded contract."""


def _mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise RegressionRegistryError(f"{label} must be an object")
    return value


def _string(value: dict[str, Any], field: str, label: str) -> str:
    item = value.get(field)
    if not isinstance(item, str) or not item.strip():
        raise RegressionRegistryError(f"{label}.{field} must be a non-empty string")
    return item


def _fraction(value: Any, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise RegressionRegistryError(f"{label} must be a number between 0.0 and 1.0")
    result = float(value)
    if not 0.0 <= result <= 1.0:
        raise RegressionRegistryError(f"{label} must be between 0.0 and 1.0")
    return result


def registry_sha256(registry: dict[str, Any]) -> str:
    """Hash the complete registry except its self-referential hash field."""

    canonical = dict(_mapping(registry, "registry"))
    canonical.pop("registry_sha256", None)
    rendered = json.dumps(canonical, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(rendered.encode("utf-8")).hexdigest()


def _validate_scenario(value: Any, index: int) -> dict[str, Any]:
    label = f"registry.scenarios[{index}]"
    scenario = _mapping(value, label)
    scenario_id = _string(scenario, "scenario_id", label)
    if not IDENTIFIER.fullmatch(scenario_id):
        raise RegressionRegistryError(f"{label}.scenario_id has an invalid shape")
    for field in ("scenario_digest", "task_digest"):
        if not HEX_DIGEST.fullmatch(_string(scenario, field, label)):
            raise RegressionRegistryError(f"{label}.{field} must be 64 lowercase hex chars")

    quality = _mapping(scenario.get("quality"), f"{label}.quality")
    method = _string(quality, "method", f"{label}.quality")
    if method not in VALIDATOR.ALLOWED_QUALITY_METHODS or method == "none":
        raise RegressionRegistryError(f"{label}.quality.method is unsupported")
    _fraction(quality.get("threshold"), f"{label}.quality.threshold")

    roles = _mapping(scenario.get("evidence_roles"), f"{label}.evidence_roles")
    if set(roles) != {"baseline", "candidate"}:
        raise RegressionRegistryError(f"{label}.evidence_roles must define baseline and candidate")
    expected_modes = {"baseline": "stateless_reread", "candidate": "stateful_store"}
    for role, expected_mode in expected_modes.items():
        role_value = _mapping(roles[role], f"{label}.evidence_roles.{role}")
        if role_value.get("mode") != expected_mode:
            raise RegressionRegistryError(
                f"{label}.evidence_roles.{role}.mode must be {expected_mode}"
            )
    best_known_role = _string(scenario, "best_known_role", label)
    if best_known_role not in roles:
        raise RegressionRegistryError(f"{label}.best_known_role must name an evidence role")

    regressions = _mapping(scenario.get("allowed_regressions"), f"{label}.allowed_regressions")
    if set(regressions) != REQUIRED_METRICS:
        raise RegressionRegistryError(
            f"{label}.allowed_regressions must define all PE-1 comparison metrics"
        )
    for metric, limit in regressions.items():
        _fraction(limit, f"{label}.allowed_regressions.{metric}")

    metadata = _mapping(scenario.get("comparison_metadata"), f"{label}.comparison_metadata")
    for field in ("runtime_kind", "runtime_version"):
        _string(metadata, field, f"{label}.comparison_metadata")
    if metadata.get("evidence_kind") != "bounded_summary_only":
        raise RegressionRegistryError(f"{label}.comparison_metadata.evidence_kind must be bounded_summary_only")
    if metadata.get("report_only") is not True:
        raise RegressionRegistryError(f"{label}.comparison_metadata.report_only must be true")
    if metadata.get("provider_calls") != "disabled":
        raise RegressionRegistryError(f"{label}.comparison_metadata.provider_calls must be disabled")
    if metadata.get("mutation_authority") != "none":
        raise RegressionRegistryError(f"{label}.comparison_metadata.mutation_authority must be none")
    schema_versions = metadata.get("supported_artifact_schema_versions")
    if (
        not isinstance(schema_versions, list)
        or not schema_versions
        or len(schema_versions) != len(set(schema_versions))
        or any(version not in SUPPORTED_ARTIFACT_SCHEMAS for version in schema_versions)
    ):
        raise RegressionRegistryError(
            f"{label}.comparison_metadata.supported_artifact_schema_versions is invalid"
        )
    return scenario


def validate_registry(value: dict[str, Any]) -> dict[str, Any]:
    """Return a normalized registry or raise RegressionRegistryError."""

    registry = _mapping(value, "registry")
    try:
        VALIDATOR._validate_json_bounds(registry)
        VALIDATOR._reject_raw_trace_keys(registry)
    except VALIDATOR.ScorecardError as exc:
        raise RegressionRegistryError(str(exc)) from exc

    if registry.get("schema_version") != SCHEMA_VERSION:
        raise RegressionRegistryError(f"registry.schema_version must be {SCHEMA_VERSION}")
    registry_id = _string(registry, "registry_id", "registry")
    if not IDENTIFIER.fullmatch(registry_id):
        raise RegressionRegistryError("registry.registry_id has an invalid shape")
    supplied_hash = _string(registry, "registry_sha256", "registry")
    if not HEX_DIGEST.fullmatch(supplied_hash):
        raise RegressionRegistryError("registry.registry_sha256 must be 64 lowercase hex chars")
    expected_hash = registry_sha256(registry)
    if supplied_hash != expected_hash:
        raise RegressionRegistryError("registry.registry_sha256 does not match canonical content")

    scenarios = registry.get("scenarios")
    if not isinstance(scenarios, list) or not 3 <= len(scenarios) <= 100:
        raise RegressionRegistryError("registry.scenarios must contain between 3 and 100 scenarios")
    validated = [_validate_scenario(scenario, index) for index, scenario in enumerate(scenarios)]
    scenario_ids = [scenario["scenario_id"] for scenario in validated]
    scenario_digests = [scenario["scenario_digest"] for scenario in validated]
    if len(scenario_ids) != len(set(scenario_ids)) or len(scenario_digests) != len(set(scenario_digests)):
        raise RegressionRegistryError("registry scenario IDs and digests must be unique")

    return json.loads(json.dumps(registry, sort_keys=True, separators=(",", ":")))


def load_registry(path: Path) -> dict[str, Any]:
    try:
        return validate_registry(VALIDATOR.load_json(path))
    except VALIDATOR.ScorecardError as exc:
        raise RegressionRegistryError(str(exc)) from exc


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Validate a bounded report-only PE-1 scenario registry.")
    parser.add_argument("registry", type=Path)
    parser.add_argument("--compact", action="store_true")
    args = parser.parse_args(argv)
    try:
        registry = load_registry(args.registry)
    except RegressionRegistryError as exc:
        print(f"token-efficiency regression registry failed: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(registry, sort_keys=True, separators=(",", ":") if args.compact else None, indent=None if args.compact else 2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
