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
REPORT_SCHEMA_VERSION = "token_efficiency_regression_report.v1"
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
LOWER_IS_BETTER = REQUIRED_METRICS - {"quality_score"}


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


def _scenario(registry: dict[str, Any], scenario_id: str) -> dict[str, Any]:
    for scenario in registry["scenarios"]:
        if scenario["scenario_id"] == scenario_id:
            return scenario
    raise RegressionRegistryError(f"scenario is not registered: {scenario_id}")


def _scorecard_evidence(value: Any, label: str) -> tuple[dict[str, Any], dict[str, Any]]:
    evidence = _mapping(value, label)
    try:
        VALIDATOR._validate_json_bounds(evidence)
        VALIDATOR._reject_raw_trace_keys(evidence)
    except VALIDATOR.ScorecardError as exc:
        raise RegressionRegistryError(str(exc)) from exc
    schema_version = evidence.get("schema_version")
    if schema_version in SUPPORTED_ARTIFACT_SCHEMAS:
        raw_scorecard = _mapping(evidence.get("scorecard"), f"{label}.scorecard")
        artifact_schema_version = schema_version
        supplied_hash = _string(evidence, "content_sha256", label)
        if not HEX_DIGEST.fullmatch(supplied_hash):
            raise RegressionRegistryError(f"{label}.content_sha256 must be 64 lowercase hex chars")
    else:
        raw_scorecard = evidence
        artifact_schema_version = "token_efficiency_scorecard.v1"
        supplied_hash = None
    try:
        scorecard = VALIDATOR.import_scorecard(raw_scorecard)
        canonical = VALIDATOR.canonical_scorecard_json(scorecard)
    except VALIDATOR.ScorecardError as exc:
        raise RegressionRegistryError(f"{label}: {exc}") from exc
    content_sha256 = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
    if supplied_hash is not None and supplied_hash != content_sha256:
        raise RegressionRegistryError(f"{label}.content_sha256 does not match canonical scorecard")
    return scorecard, {
        "adapter_run_id": scorecard["adapter_run_id"],
        "artifact_schema_version": artifact_schema_version,
        "content_sha256": content_sha256,
    }


def _report_payload(
    registry: dict[str, Any],
    scenario: dict[str, Any],
    current_evidence: dict[str, Any],
    baseline_evidence: dict[str, Any] | None,
    best_known_evidence: dict[str, Any] | None,
) -> dict[str, Any]:
    return {
        "schema_version": REPORT_SCHEMA_VERSION,
        "registry_id": registry["registry_id"],
        "registry_sha256": registry["registry_sha256"],
        "scenario_id": scenario["scenario_id"],
        "scenario_digest": scenario["scenario_digest"],
        "task_digest": scenario["task_digest"],
        "read_only": True,
        "report_only": True,
        "provider_calls": "disabled",
        "mutation_authority": "none",
        "outcome": "pass",
        "reason_codes": [],
        "evidence": {
            "current": current_evidence,
            "baseline": baseline_evidence,
            "best_known": best_known_evidence,
        },
        "comparisons": {},
    }


def _finalize_report(payload: dict[str, Any]) -> dict[str, Any]:
    rendered = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    report = dict(payload)
    report["report_sha256"] = hashlib.sha256(rendered.encode("utf-8")).hexdigest()
    return report


def validate_regression_report(value: dict[str, Any]) -> dict[str, Any]:
    """Validate report integrity and the report-only safety boundary."""

    report = _mapping(value, "report")
    try:
        VALIDATOR._validate_json_bounds(report)
        VALIDATOR._reject_raw_trace_keys(report)
    except VALIDATOR.ScorecardError as exc:
        raise RegressionRegistryError(str(exc)) from exc
    if report.get("schema_version") != REPORT_SCHEMA_VERSION:
        raise RegressionRegistryError(
            f"report.schema_version must be {REPORT_SCHEMA_VERSION}"
        )
    supplied_hash = _string(report, "report_sha256", "report")
    if not HEX_DIGEST.fullmatch(supplied_hash):
        raise RegressionRegistryError("report.report_sha256 must be 64 lowercase hex chars")
    canonical = dict(report)
    canonical.pop("report_sha256", None)
    expected_hash = hashlib.sha256(
        json.dumps(canonical, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    if supplied_hash != expected_hash:
        raise RegressionRegistryError("report.report_sha256 does not match canonical content")
    if report.get("read_only") is not True or report.get("report_only") is not True:
        raise RegressionRegistryError("report must remain read_only and report_only")
    if report.get("provider_calls") != "disabled":
        raise RegressionRegistryError("report.provider_calls must be disabled")
    if report.get("mutation_authority") != "none":
        raise RegressionRegistryError("report.mutation_authority must be none")
    if report.get("outcome") not in {
        "pass",
        "regression",
        "missing_baseline",
        "missing_best_known",
        "incomparable",
        "quality_failure",
    }:
        raise RegressionRegistryError("report.outcome is unsupported")
    if not isinstance(report.get("reason_codes"), list):
        raise RegressionRegistryError("report.reason_codes must be a list")
    _mapping(report.get("evidence"), "report.evidence")
    _mapping(report.get("comparisons"), "report.comparisons")
    return json.loads(json.dumps(report, sort_keys=True, separators=(",", ":")))


def _contract_reasons(
    scenario: dict[str, Any],
    current: dict[str, Any],
    baseline: dict[str, Any],
    best_known: dict[str, Any],
) -> list[str]:
    reasons: list[str] = []
    expected = {
        "scenario_id": scenario["scenario_id"],
        "runtime_kind": scenario["comparison_metadata"]["runtime_kind"],
        "runtime_version": scenario["comparison_metadata"]["runtime_version"],
    }
    registry_contract = {
        "scenario_digest": scenario["scenario_digest"],
        "task_digest": scenario["task_digest"],
        "quality_method": scenario["quality"]["method"],
        "quality_threshold": scenario["quality"]["threshold"],
    }
    for label, scorecard in (
        ("current", current),
        ("baseline", baseline),
        ("best_known", best_known),
    ):
        for field, value in expected.items():
            if scorecard.get(field) != value:
                reasons.append(f"{label}.{field}_mismatch")
        contract = scorecard.get("comparison_contract", {})
        for field, value in registry_contract.items():
            if contract.get(field) != value:
                reasons.append(f"{label}.{field}_mismatch")
    current_contract = current.get("comparison_contract")
    if baseline.get("comparison_contract") != current_contract:
        reasons.append("baseline.comparison_contract_mismatch")
    if best_known.get("comparison_contract") != current_contract:
        reasons.append("best_known.comparison_contract_mismatch")
    return sorted(set(reasons))


def _quality_qualified(scorecard: dict[str, Any], threshold: float) -> bool:
    score = scorecard.get("quality_score")
    return (
        scorecard.get("status") == "pass"
        and isinstance(score, (int, float))
        and not isinstance(score, bool)
        and float(score) >= threshold
    )


def _metric_values(scorecard: dict[str, Any]) -> dict[str, float | int]:
    derived = scorecard["derived_metrics"]
    state_bytes = sum(
        int(step.get("state_read_bytes", 0)) + int(step.get("state_write_bytes", 0))
        for step in scorecard.get("steps", [])
    )
    return {
        "total_tokens": int(derived["total_tokens"]),
        "repeated_context_ratio": float(derived["repeated_context_ratio"]),
        "state_bytes": state_bytes,
        "estimated_cost_usd": float(scorecard.get("estimated_cost_usd", 0.0)),
        "duration_ms": int(scorecard["duration_ms"]),
        "retry_count": int(scorecard["retry_count"]),
        "quality_score": float(scorecard.get("quality_score", 0.0)),
    }


def _comparison(
    current: dict[str, Any],
    reference: dict[str, Any],
    allowed_regressions: dict[str, Any],
) -> dict[str, Any]:
    current_metrics = _metric_values(current)
    reference_metrics = _metric_values(reference)
    metrics: dict[str, Any] = {}
    for metric in sorted(REQUIRED_METRICS):
        current_value = current_metrics[metric]
        reference_value = reference_metrics[metric]
        delta = round(float(current_value) - float(reference_value), 6)
        harmful_delta = delta if metric in LOWER_IS_BETTER else -delta
        normalized_regression = round(max(0.0, harmful_delta) / max(abs(float(reference_value)), 1.0), 6)
        allowed = float(allowed_regressions[metric])
        metrics[metric] = {
            "current": current_value,
            "reference": reference_value,
            "delta": delta,
            "normalized_regression": normalized_regression,
            "allowed_regression": allowed,
            "regressed": normalized_regression > allowed,
        }
    return {"metrics": metrics}


def build_regression_report(
    registry: dict[str, Any],
    scenario_id: str,
    *,
    current: dict[str, Any],
    baseline: dict[str, Any] | None,
    best_known: dict[str, Any] | None,
) -> dict[str, Any]:
    """Recompute one deterministic report from bounded scorecard evidence."""

    normalized_registry = validate_registry(registry)
    scenario = _scenario(normalized_registry, scenario_id)
    current_scorecard, current_evidence = _scorecard_evidence(current, "current")
    if baseline is None:
        payload = _report_payload(normalized_registry, scenario, current_evidence, None, None)
        payload.update(outcome="missing_baseline", reason_codes=["missing_baseline"])
        return _finalize_report(payload)
    baseline_scorecard, baseline_evidence = _scorecard_evidence(baseline, "baseline")
    if best_known is None:
        payload = _report_payload(
            normalized_registry, scenario, current_evidence, baseline_evidence, None
        )
        payload.update(outcome="missing_best_known", reason_codes=["missing_best_known"])
        return _finalize_report(payload)
    best_known_scorecard, best_known_evidence = _scorecard_evidence(best_known, "best_known")
    payload = _report_payload(
        normalized_registry,
        scenario,
        current_evidence,
        baseline_evidence,
        best_known_evidence,
    )

    reasons = _contract_reasons(
        scenario, current_scorecard, baseline_scorecard, best_known_scorecard
    )
    if reasons:
        payload.update(outcome="incomparable", reason_codes=reasons)
        return _finalize_report(payload)

    threshold = float(scenario["quality"]["threshold"])
    if not _quality_qualified(current_scorecard, threshold):
        payload.update(
            outcome="quality_failure", reason_codes=["current_quality_below_threshold"]
        )
        return _finalize_report(payload)
    reference_quality_failures = [
        f"{label}_quality_below_threshold"
        for label, scorecard in (
            ("baseline", baseline_scorecard),
            ("best_known", best_known_scorecard),
        )
        if not _quality_qualified(scorecard, threshold)
    ]
    if reference_quality_failures:
        payload.update(outcome="incomparable", reason_codes=reference_quality_failures)
        return _finalize_report(payload)

    comparisons = {
        "baseline": _comparison(
            current_scorecard, baseline_scorecard, scenario["allowed_regressions"]
        ),
        "best_known": _comparison(
            current_scorecard, best_known_scorecard, scenario["allowed_regressions"]
        ),
    }
    regression_reasons = sorted(
        f"{reference}.{metric}"
        for reference, comparison in comparisons.items()
        for metric, result in comparison["metrics"].items()
        if result["regressed"]
    )
    payload["comparisons"] = comparisons
    if regression_reasons:
        payload.update(outcome="regression", reason_codes=regression_reasons)
    return _finalize_report(payload)


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
