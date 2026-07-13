#!/usr/bin/env python3
"""Versioned, bounded contracts for PE-6 recovery-drill evidence.

The module is deliberately dependency-free and test-only.  It describes the
evidence emitted by the existing storage, workflow, provider, and release
owners; it does not become a runtime recovery or persistence authority.
"""

from __future__ import annotations

import hashlib
import json
import math
import re
from pathlib import Path
from typing import Any, Mapping


SCENARIO_SCHEMA_VERSION = "fault_scenario.v1"
RESULT_SCHEMA_VERSION = "fault_drill_result.v1"
RECOVERY_EVIDENCE_SCHEMA_VERSION = "fault_recovery_evidence.v1"
CLEANUP_EVIDENCE_SCHEMA_VERSION = "fault_cleanup_evidence.v1"
REPORT_SCHEMA_VERSION = "fault_drill_report.v1"
REGISTRY_SCHEMA_VERSION = "fault_registry.v1"

MAX_JSON_BYTES = 1024 * 1024
MAX_STRING_BYTES = 2048
MAX_ARRAY_ITEMS = 256
MAX_OBJECT_FIELDS = 64
MAX_JSON_DEPTH = 12
MAX_EVIDENCE_REFS = 64
MAX_DURATION_MS = 120_000
MAX_RETRIES = 3
MAX_PROCESSES = 4
MAX_FILES = 256
MAX_BYTES = 4 * 1024 * 1024
MAX_EVENTS = 512

SCENARIO_ID_RE = re.compile(r"^[a-z0-9][a-z0-9._:-]{2,127}$")
RESOURCE_ID_RE = re.compile(r"^[a-z0-9][a-z0-9._:-]{2,127}$")
SOURCE_HEAD_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")

STATUSES = frozenset(
    {
        "passed",
        "failed_recovery",
        "failed_rollback",
        "cleanup_failed",
        "unsupported",
        "invalid_scenario",
        "aborted",
    }
)
RESOURCE_KINDS = frozenset(
    {
        "temp_dir",
        "sqlite_db",
        "postgres_container",
        "synthetic_repo",
        "controlled_process",
        "release_bundle",
        "fake_provider",
    }
)
CAPABILITIES = frozenset(
    {
        "filesystem",
        "sqlite",
        "postgres",
        "process",
        "rust_engine",
        "fake_provider",
        "release",
    }
)
FAULT_POINTS = frozenset(
    {
        "before_commit",
        "during_transaction",
        "after_commit_ack",
        "duplicate_write",
        "concurrent_conflict",
        "migration_interrupt",
        "integrity_tamper",
        "backup_interrupt",
        "restore_tamper",
        "stale_lease",
        "executor_timeout",
        "worker_loss",
        "retry_exhaustion",
        "approval_race",
        "provider_timeout",
        "provider_malformed",
        "provider_audit_failure",
        "provider_kill",
        "release_artifact_tamper",
        "release_evidence_missing",
        "release_activation_failure",
        "rollback_interrupt",
        "cleanup_failure",
    }
)

REASON_CODES = frozenset(
    {
        "DRILL_PASSED",
        "RECOVERY_VERIFIED",
        "ROLLBACK_VERIFIED",
        "INTEGRITY_VERIFIED",
        "AUDIT_VERIFIED",
        "RESTART_IDEMPOTENT",
        "CLEANUP_VERIFIED",
        "UNSUPPORTED_ENVIRONMENT",
        "UNKNOWN_SCENARIO",
        "UNKNOWN_FAULT_POINT",
        "NON_DISPOSABLE_RESOURCE",
        "RESOURCE_ID_CONFLICT",
        "EVIDENCE_TAMPERED",
        "CLEANUP_OMITTED",
        "CLEANUP_FAILED",
        "DRILL_TIMEOUT",
        "DRILL_ABORTED",
        "OWNER_TEST_FAILED",
        "COMMAND_NOT_ALLOWLISTED",
        "SCENARIO_BOUNDS_EXCEEDED",
        "REPORT_BOUNDS_EXCEEDED",
        "DUPLICATE_SCENARIO",
        "INVALID_EVIDENCE",
        "INVARIANT_AMBIGUOUS",
    }
)

_FORBIDDEN_EVIDENCE_FRAGMENTS = (
    "raw_prompt",
    "raw_output",
    "transcript",
    "credential",
    "password",
    "private_key",
    "secret",
    "/home/",
    "/users/",
    "c:\\users\\",
)


class ContractError(ValueError):
    """Raised when bounded drill input or evidence is not safe to consume."""


def canonical_json_bytes(value: Any) -> bytes:
    """Return the one newline-terminated representation used for evidence hashes."""

    try:
        rendered = json.dumps(
            value,
            ensure_ascii=True,
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        )
    except (TypeError, ValueError) as exc:
        raise ContractError(f"value is not canonicalizable: {exc}") from exc
    return (rendered + "\n").encode("utf-8")


def sha256_value(value: Any) -> str:
    return hashlib.sha256(canonical_json_bytes(value)).hexdigest()


def _reject_duplicate_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _check_bounds(value: Any, depth: int = 0) -> None:
    if depth > MAX_JSON_DEPTH:
        raise ContractError("JSON depth exceeds PE-6 bound")
    if isinstance(value, str):
        if len(value.encode("utf-8")) > MAX_STRING_BYTES:
            raise ContractError("JSON string exceeds PE-6 bound")
    elif isinstance(value, bool) or value is None:
        return
    elif isinstance(value, int):
        if abs(value) > 2**63 - 1:
            raise ContractError("integer exceeds PE-6 bound")
    elif isinstance(value, float):
        if not math.isfinite(value):
            raise ContractError("non-finite number is not allowed")
    elif isinstance(value, list):
        if len(value) > MAX_ARRAY_ITEMS:
            raise ContractError("array exceeds PE-6 bound")
        for item in value:
            _check_bounds(item, depth + 1)
    elif isinstance(value, dict):
        if len(value) > MAX_OBJECT_FIELDS:
            raise ContractError("object exceeds PE-6 bound")
        for key, item in value.items():
            _check_bounds(key, depth + 1)
            _check_bounds(item, depth + 1)
    else:
        raise ContractError(f"unsupported JSON value: {type(value).__name__}")


def parse_json_bytes(raw: bytes) -> Any:
    if len(raw) > MAX_JSON_BYTES:
        raise ContractError("JSON input exceeds PE-6 bound")
    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=_reject_duplicate_pairs)
    except (UnicodeDecodeError, json.JSONDecodeError, ContractError) as exc:
        raise ContractError(f"invalid JSON evidence: {exc}") from exc
    _check_bounds(value)
    return value


def read_json(path: Path) -> Any:
    try:
        if not path.is_file():
            raise ContractError("evidence file is missing")
        if path.stat().st_size > MAX_JSON_BYTES:
            raise ContractError("evidence file exceeds PE-6 bound")
        return parse_json_bytes(path.read_bytes())
    except OSError as exc:
        raise ContractError(f"cannot read evidence: {exc}") from exc


def write_canonical_json(path: Path, value: Any) -> None:
    _check_bounds(value)
    encoded = canonical_json_bytes(value)
    if len(encoded) > MAX_JSON_BYTES:
        raise ContractError("evidence output exceeds PE-6 bound")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(encoded)


def _required_string(mapping: Mapping[str, Any], key: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str) or not value:
        raise ContractError(f"required string is missing: {key}")
    return value


def _bounded_id(value: Any, pattern: re.Pattern[str], field: str) -> str:
    if not isinstance(value, str) or not pattern.fullmatch(value):
        raise ContractError(f"invalid {field}")
    return value


def _required_sha(value: Any, field: str) -> str:
    if not isinstance(value, str) or not SHA256_RE.fullmatch(value):
        raise ContractError(f"invalid {field}")
    return value


def _require_fields(mapping: Mapping[str, Any], fields: set[str], label: str) -> None:
    if set(mapping) != fields:
        missing = sorted(fields - set(mapping))
        extra = sorted(set(mapping) - fields)
        detail = []
        if missing:
            detail.append(f"missing={','.join(missing)}")
        if extra:
            detail.append(f"extra={','.join(extra)}")
        raise ContractError(f"invalid {label} fields ({'; '.join(detail)})")


def _bounded_string_list(value: Any, field: str, *, allow_empty: bool = True) -> list[str]:
    if not isinstance(value, list) or (not allow_empty and not value):
        raise ContractError(f"{field} must be a non-empty string array")
    if len(value) > MAX_ARRAY_ITEMS:
        raise ContractError(f"{field} exceeds bound")
    result: list[str] = []
    for item in value:
        if not isinstance(item, str) or not item:
            raise ContractError(f"{field} contains an invalid string")
        result.append(item)
    return result


def _validate_resource(resource: Any) -> dict[str, Any]:
    if not isinstance(resource, dict):
        raise ContractError("resource must be an object")
    _require_fields(
        resource,
        {"kind", "resource_id", "capability", "disposable", "created_by"},
        "resource",
    )
    if resource["kind"] not in RESOURCE_KINDS:
        raise ContractError("resource kind is not disposable/allowlisted")
    _bounded_id(resource["resource_id"], RESOURCE_ID_RE, "resource_id")
    if resource["capability"] not in CAPABILITIES:
        raise ContractError("resource capability is not allowlisted")
    if resource["disposable"] is not True:
        raise ContractError("non-disposable resources are forbidden")
    if resource["created_by"] != "pe6-harness":
        raise ContractError("resource creator is not the PE-6 harness")
    return resource


def _validate_environment(environment: Any) -> dict[str, Any]:
    if not isinstance(environment, dict):
        raise ContractError("environment must be an object")
    _require_fields(environment, {"name", "capabilities"}, "environment")
    _required_string(environment, "name")
    capabilities = _bounded_string_list(environment.get("capabilities"), "capabilities", allow_empty=False)
    if any(capability not in CAPABILITIES for capability in capabilities):
        raise ContractError("environment has an unknown capability")
    if len(set(capabilities)) != len(capabilities):
        raise ContractError("environment capabilities must be unique")
    return environment


def _validate_invariants(invariants: Any) -> dict[str, str]:
    required = {
        "normal",
        "detection",
        "recovery",
        "rollback",
        "integrity",
        "audit",
        "restart_concurrency_idempotency",
        "abort",
        "cleanup",
    }
    if not isinstance(invariants, dict):
        raise ContractError("invariants must be an object")
    _require_fields(invariants, required, "invariants")
    for field in required:
        _required_string(invariants, field)
    return invariants


def validate_scenario(scenario: Mapping[str, Any]) -> dict[str, Any]:
    if not isinstance(scenario, dict):
        raise ContractError("scenario must be an object")
    _require_fields(
        scenario,
        {
            "schema_version",
            "scenario_id",
            "scenario_version",
            "seed",
            "worker_id",
            "source_head",
            "subsystem",
            "owner",
            "environment",
            "resources",
            "fault",
            "invariants",
            "timeout_ms",
            "max_retries",
            "max_processes",
            "max_files",
            "max_bytes",
            "max_events",
            "max_evidence_refs",
        },
        "scenario",
    )
    if scenario["schema_version"] != SCENARIO_SCHEMA_VERSION:
        raise ContractError("unsupported fault scenario schema")
    _bounded_id(scenario["scenario_id"], SCENARIO_ID_RE, "scenario_id")
    if not isinstance(scenario["scenario_version"], str) or not re.fullmatch(r"v[0-9]+", scenario["scenario_version"]):
        raise ContractError("invalid scenario_version")
    for field in ("seed", "worker_id"):
        value = scenario[field]
        if not isinstance(value, int) or isinstance(value, bool) or value < 0 or value > 2**32 - 1:
            raise ContractError(f"invalid {field}")
    source_head = _required_string(scenario, "source_head")
    if not SOURCE_HEAD_RE.fullmatch(source_head):
        raise ContractError("source_head must be a full commit SHA")
    _required_string(scenario, "subsystem")
    _required_string(scenario, "owner")
    environment = _validate_environment(scenario["environment"])
    environment_capabilities = set(environment["capabilities"])
    resources = scenario["resources"]
    if not isinstance(resources, list) or not resources or len(resources) > MAX_ARRAY_ITEMS:
        raise ContractError("scenario resources must be bounded and non-empty")
    resource_ids: set[str] = set()
    for resource in resources:
        validated = _validate_resource(resource)
        if validated["capability"] not in environment_capabilities:
            raise ContractError("resource capability is absent from the environment binding")
        if validated["resource_id"] in resource_ids:
            raise ContractError("duplicate resource identity")
        resource_ids.add(validated["resource_id"])

    fault = scenario["fault"]
    if not isinstance(fault, dict):
        raise ContractError("fault must be an object")
    _require_fields(fault, {"fault_id", "injection_point", "mode"}, "fault")
    _bounded_id(fault["fault_id"], SCENARIO_ID_RE, "fault_id")
    if fault["injection_point"] not in FAULT_POINTS:
        raise ContractError("unknown fault injection point")
    if fault["mode"] not in {"error", "interrupt", "timeout", "tamper", "duplicate", "race", "kill"}:
        raise ContractError("unknown fault mode")
    _validate_invariants(scenario["invariants"])

    numeric_bounds = {
        "timeout_ms": (1, MAX_DURATION_MS),
        "max_retries": (0, MAX_RETRIES),
        "max_processes": (1, MAX_PROCESSES),
        "max_files": (1, MAX_FILES),
        "max_bytes": (1, MAX_BYTES),
        "max_events": (1, MAX_EVENTS),
        "max_evidence_refs": (1, MAX_EVIDENCE_REFS),
    }
    for field, (lower, upper) in numeric_bounds.items():
        value = scenario[field]
        if not isinstance(value, int) or isinstance(value, bool) or not lower <= value <= upper:
            raise ContractError(f"{field} is outside its bound")
    _check_bounds(scenario)
    return dict(scenario)


def scenario_sha256(scenario: Mapping[str, Any]) -> str:
    return sha256_value(validate_scenario(scenario))


def _evidence_without_hash(evidence: Mapping[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in evidence.items() if key != "sha256"}


def seal_evidence(
    *,
    kind: str,
    evidence_id: str,
    outcome: str,
    checks: list[Mapping[str, Any]],
    observations: list[str],
    actions: list[str],
    reason_codes: list[str],
) -> dict[str, Any]:
    schema_version = CLEANUP_EVIDENCE_SCHEMA_VERSION if kind == "cleanup" else RECOVERY_EVIDENCE_SCHEMA_VERSION
    evidence = {
        "schema_version": schema_version,
        "evidence_id": evidence_id,
        "kind": kind,
        "outcome": outcome,
        "checks": [dict(check) for check in checks],
        "observations": list(observations),
        "actions": list(actions),
        "reason_codes": list(reason_codes),
    }
    evidence["sha256"] = sha256_value(evidence)
    validate_evidence(evidence)
    return evidence


def validate_evidence(evidence: Mapping[str, Any]) -> dict[str, Any]:
    if not isinstance(evidence, dict):
        raise ContractError("evidence must be an object")
    required = {"schema_version", "evidence_id", "kind", "outcome", "checks", "observations", "actions", "reason_codes", "sha256"}
    _require_fields(evidence, required, "evidence")
    kind = evidence["kind"]
    if not isinstance(kind, str) or kind not in {"recovery", "rollback", "integrity", "audit", "restart", "cleanup"}:
        raise ContractError("unknown evidence kind")
    expected_schema = CLEANUP_EVIDENCE_SCHEMA_VERSION if kind == "cleanup" else RECOVERY_EVIDENCE_SCHEMA_VERSION
    if evidence["schema_version"] != expected_schema:
        raise ContractError("evidence schema does not match evidence kind")
    _bounded_id(evidence["evidence_id"], RESOURCE_ID_RE, "evidence_id")
    allowed_outcomes = {"passed", "failed", "unsupported", "aborted", "cleaned"} if kind == "cleanup" else {"passed", "failed", "unsupported", "aborted"}
    if evidence["outcome"] not in allowed_outcomes:
        raise ContractError("invalid evidence outcome")
    checks = evidence["checks"]
    if not isinstance(checks, list) or not checks or len(checks) > MAX_EVENTS:
        raise ContractError("evidence checks are missing or oversized")
    seen_checks: set[str] = set()
    for check in checks:
        if not isinstance(check, dict):
            raise ContractError("evidence check must be an object")
        _require_fields(check, {"name", "passed"}, "evidence check")
        name = _required_string(check, "name")
        if name in seen_checks or not isinstance(check["passed"], bool):
            raise ContractError("evidence checks must be unique booleans")
        seen_checks.add(name)
    for field in ("observations", "actions"):
        _bounded_string_list(evidence[field], field)
    reason_codes = _bounded_string_list(evidence["reason_codes"], "reason_codes")
    if any(code not in REASON_CODES for code in reason_codes):
        raise ContractError("unknown evidence reason code")
    rendered = canonical_json_bytes(_evidence_without_hash(evidence)).decode("utf-8").lower()
    if any(fragment in rendered for fragment in _FORBIDDEN_EVIDENCE_FRAGMENTS):
        raise ContractError("evidence contains a forbidden raw or secret-shaped field")
    if evidence["sha256"] != sha256_value(_evidence_without_hash(evidence)):
        raise ContractError("evidence hash does not match canonical content")
    _check_bounds(evidence)
    return dict(evidence)


def _validate_injection(injection: Any) -> None:
    if not isinstance(injection, dict):
        raise ContractError("injection must be an object")
    _require_fields(injection, {"fault_id", "injection_point", "observation"}, "injection")
    _bounded_id(injection["fault_id"], SCENARIO_ID_RE, "injection fault_id")
    if injection["injection_point"] not in FAULT_POINTS:
        raise ContractError("injection point is not registered")
    _required_string(injection, "observation")


def _validate_detection(detection: Any) -> None:
    if not isinstance(detection, dict):
        raise ContractError("detection must be an object")
    _require_fields(detection, {"detected", "reason_code", "timeout_ms", "abort_condition"}, "detection")
    if not isinstance(detection["detected"], bool):
        raise ContractError("detection must be explicit")
    if detection["reason_code"] not in REASON_CODES:
        raise ContractError("detection reason is not bounded")
    if not isinstance(detection["timeout_ms"], int) or not 0 <= detection["timeout_ms"] <= MAX_DURATION_MS:
        raise ContractError("detection timeout is outside its bound")
    _required_string(detection, "abort_condition")


def _validate_evidence_refs(value: Any, evidence: list[Mapping[str, Any]], max_refs: int) -> None:
    if not isinstance(value, list) or len(value) > max_refs:
        raise ContractError("evidence_refs are missing or oversized")
    expected: dict[str, tuple[str, str]] = {}
    for item in evidence:
        evidence_id = item["evidence_id"]
        if evidence_id in expected:
            raise ContractError("evidence IDs must be unique")
        expected[evidence_id] = (item["sha256"], item["kind"])
    seen: set[str] = set()
    for ref in value:
        if not isinstance(ref, dict):
            raise ContractError("evidence reference must be an object")
        _require_fields(ref, {"evidence_id", "sha256", "kind"}, "evidence reference")
        evidence_id = _bounded_id(ref["evidence_id"], RESOURCE_ID_RE, "evidence reference id")
        if evidence_id in seen or evidence_id not in expected:
            raise ContractError("evidence reference is duplicate or unknown")
        _required_sha(ref["sha256"], "evidence reference sha256")
        expected_sha, expected_kind = expected[evidence_id]
        if expected_sha != ref["sha256"] or expected_kind != ref["kind"]:
            raise ContractError("evidence reference hash mismatch")
        seen.add(evidence_id)
        _required_string(ref, "kind")
    if seen != set(expected):
        raise ContractError("not all evidence is referenced")


def validate_result(result: Mapping[str, Any], scenario: Mapping[str, Any] | None = None) -> dict[str, Any]:
    if not isinstance(result, dict):
        raise ContractError("drill result must be an object")
    _require_fields(
        result,
        {
            "schema_version",
            "scenario_id",
            "scenario_version",
            "scenario_sha256",
            "seed",
            "worker_id",
            "source_head",
            "environment",
            "resources",
            "injection",
            "detection",
            "recovery_evidence",
            "rollback_evidence",
            "integrity_evidence",
            "audit_evidence",
            "restart_evidence",
            "cleanup_evidence",
            "status",
            "duration_ms",
            "reason_codes",
            "evidence_refs",
        },
        "drill result",
    )
    if result["schema_version"] != RESULT_SCHEMA_VERSION:
        raise ContractError("unsupported drill result schema")
    _bounded_id(result["scenario_id"], SCENARIO_ID_RE, "scenario_id")
    if not isinstance(result["scenario_version"], str) or not re.fullmatch(r"v[0-9]+", result["scenario_version"]):
        raise ContractError("invalid result scenario version")
    _required_sha(result["scenario_sha256"], "scenario_sha256")
    if not isinstance(result["seed"], int) or isinstance(result["seed"], bool) or not 0 <= result["seed"] <= 2**32 - 1:
        raise ContractError("invalid result seed")
    if not isinstance(result["worker_id"], int) or isinstance(result["worker_id"], bool) or not 0 <= result["worker_id"] <= 2**32 - 1:
        raise ContractError("invalid result worker_id")
    if not SOURCE_HEAD_RE.fullmatch(str(result["source_head"])):
        raise ContractError("invalid result source_head")
    environment = _validate_environment(result["environment"])
    resources = result["resources"]
    if not isinstance(resources, list) or not resources:
        raise ContractError("result resources are missing")
    resource_ids: set[str] = set()
    for resource in resources:
        validated_resource = _validate_resource(resource)
        if validated_resource["capability"] not in set(environment["capabilities"]):
            raise ContractError("result resource capability is absent from the environment binding")
        if validated_resource["resource_id"] in resource_ids:
            raise ContractError("result contains duplicate resource identity")
        resource_ids.add(validated_resource["resource_id"])
    _validate_injection(result["injection"])
    _validate_detection(result["detection"])

    evidence_fields = (
        "recovery_evidence",
        "rollback_evidence",
        "integrity_evidence",
        "audit_evidence",
        "restart_evidence",
        "cleanup_evidence",
    )
    evidence = [validate_evidence(result[field]) for field in evidence_fields]
    expected_kinds = ("recovery", "rollback", "integrity", "audit", "restart", "cleanup")
    if any(item["kind"] != expected for item, expected in zip(evidence, expected_kinds)):
        raise ContractError("result evidence field is bound to the wrong evidence kind")
    _validate_evidence_refs(result["evidence_refs"], evidence, MAX_EVIDENCE_REFS)

    status = result["status"]
    if status not in STATUSES:
        raise ContractError("unknown drill status")
    if not isinstance(result["duration_ms"], int) or not 0 <= result["duration_ms"] <= MAX_DURATION_MS:
        raise ContractError("duration is outside its bound")
    reason_codes = _bounded_string_list(result["reason_codes"], "reason_codes")
    if any(code not in REASON_CODES for code in reason_codes):
        raise ContractError("unknown result reason code")
    if len(set(reason_codes)) != len(reason_codes):
        raise ContractError("result reason codes must be unique")

    cleanup = result["cleanup_evidence"]
    if status == "passed":
        if cleanup["outcome"] != "cleaned" or not all(check["passed"] for check in cleanup["checks"]):
            raise ContractError("a passed drill must prove cleanup")
        for field in evidence_fields[:-1]:
            if result[field]["outcome"] != "passed" or not all(check["passed"] for check in result[field]["checks"]):
                raise ContractError("a passed drill must prove every invariant")
        required_reasons = {"DRILL_PASSED", "RECOVERY_VERIFIED", "ROLLBACK_VERIFIED", "INTEGRITY_VERIFIED", "AUDIT_VERIFIED", "CLEANUP_VERIFIED"}
        if not required_reasons.issubset(reason_codes):
            raise ContractError("passed drill is missing bounded acceptance reasons")
    if cleanup["outcome"] not in {"cleaned", "failed"}:
        raise ContractError("cleanup outcome is ambiguous")
    if cleanup["outcome"] == "failed" and status != "cleanup_failed":
        raise ContractError("cleanup failure must be the result status")
    if status == "unsupported" and "UNSUPPORTED_ENVIRONMENT" not in reason_codes:
        raise ContractError("unsupported result needs an explicit reason")
    if status == "invalid_scenario" and not ({"UNKNOWN_SCENARIO", "UNKNOWN_FAULT_POINT", "NON_DISPOSABLE_RESOURCE", "SCENARIO_BOUNDS_EXCEEDED"} & set(reason_codes)):
        raise ContractError("invalid scenario result needs a bounded reason")
    if scenario is not None:
        validated_scenario = validate_scenario(scenario)
        if result["scenario_id"] != validated_scenario["scenario_id"] or result["scenario_version"] != validated_scenario["scenario_version"]:
            raise ContractError("result is bound to a different scenario")
        if result["scenario_sha256"] != scenario_sha256(validated_scenario):
            raise ContractError("scenario hash binding mismatch")
        if result["seed"] != validated_scenario["seed"] or result["worker_id"] != validated_scenario["worker_id"]:
            raise ContractError("result seed/worker binding mismatch")
        if result["source_head"] != validated_scenario["source_head"] or environment != validated_scenario["environment"]:
            raise ContractError("result environment/source binding mismatch")
        if result["resources"] != validated_scenario["resources"]:
            raise ContractError("result resource binding mismatch")
        if result["injection"]["fault_id"] != validated_scenario["fault"]["fault_id"] or result["injection"]["injection_point"] != validated_scenario["fault"]["injection_point"]:
            raise ContractError("result fault binding mismatch")
    _check_bounds(result)
    return dict(result)


def build_report(
    *,
    suite: str,
    source_head: str,
    seed: int,
    worker_id: int,
    environment: Mapping[str, Any],
    results: list[Mapping[str, Any]],
) -> dict[str, Any]:
    if not isinstance(suite, str) or not suite:
        raise ContractError("suite is required")
    if not SOURCE_HEAD_RE.fullmatch(source_head):
        raise ContractError("report source_head is invalid")
    if not isinstance(seed, int) or not 0 <= seed <= 2**32 - 1:
        raise ContractError("report seed is invalid")
    if not isinstance(worker_id, int) or not 0 <= worker_id <= 2**32 - 1:
        raise ContractError("report worker_id is invalid")
    validated_environment = _validate_environment(dict(environment))
    normalized = [validate_result(result) for result in results]
    normalized.sort(key=lambda item: item["scenario_id"])
    if not normalized or len(normalized) > MAX_ARRAY_ITEMS:
        raise ContractError("report must contain a bounded result set")
    if any(result["source_head"] != source_head or result["seed"] != seed or result["worker_id"] != worker_id for result in normalized):
        raise ContractError("report result binding mismatch")
    counts = {status: 0 for status in sorted(STATUSES)}
    for result in normalized:
        counts[result["status"]] += 1
    report = {
        "schema_version": REPORT_SCHEMA_VERSION,
        "registry_schema_version": REGISTRY_SCHEMA_VERSION,
        "suite": suite,
        "source_head": source_head,
        "seed": seed,
        "worker_id": worker_id,
        "environment": validated_environment,
        "results": normalized,
        "summary": {"counts": counts, "total": len(normalized)},
    }
    report["report_sha256"] = sha256_value(report)
    _check_bounds(report)
    if len(canonical_json_bytes(report)) > MAX_JSON_BYTES:
        raise ContractError("report exceeds PE-6 bound")
    return report


def validate_report(report: Mapping[str, Any]) -> dict[str, Any]:
    if not isinstance(report, dict):
        raise ContractError("report must be an object")
    required = {"schema_version", "registry_schema_version", "suite", "source_head", "seed", "worker_id", "environment", "results", "summary", "report_sha256"}
    _require_fields(report, required, "report")
    if report["schema_version"] != REPORT_SCHEMA_VERSION or report["registry_schema_version"] != REGISTRY_SCHEMA_VERSION:
        raise ContractError("unsupported report schema")
    _required_string(report, "suite")
    _validate_environment(report["environment"])
    if not SOURCE_HEAD_RE.fullmatch(str(report["source_head"])):
        raise ContractError("invalid report source head")
    if not isinstance(report["seed"], int) or not 0 <= report["seed"] <= 2**32 - 1:
        raise ContractError("invalid report seed")
    if not isinstance(report["worker_id"], int) or not 0 <= report["worker_id"] <= 2**32 - 1:
        raise ContractError("invalid report worker")
    results = report["results"]
    if not isinstance(results, list) or not results:
        raise ContractError("report results are missing")
    seen_scenarios: set[str] = set()
    for result in results:
        validated_result = validate_result(result)
        if validated_result["scenario_id"] in seen_scenarios:
            raise ContractError("report contains duplicate scenario results")
        seen_scenarios.add(validated_result["scenario_id"])
        if (
            validated_result["source_head"] != report["source_head"]
            or validated_result["seed"] != report["seed"]
            or validated_result["worker_id"] != report["worker_id"]
        ):
            raise ContractError("report result is not bound to its source/seed/worker")
    summary = report["summary"]
    if not isinstance(summary, dict) or set(summary) != {"counts", "total"}:
        raise ContractError("report summary is invalid")
    if summary["total"] != len(results) or not isinstance(summary["counts"], dict):
        raise ContractError("report summary count mismatch")
    expected_counts = {status: 0 for status in sorted(STATUSES)}
    for result in results:
        expected_counts[result["status"]] += 1
    if summary["counts"] != expected_counts:
        raise ContractError("report status counts mismatch")
    if report["report_sha256"] != sha256_value({key: value for key, value in report.items() if key != "report_sha256"}):
        raise ContractError("report hash mismatch")
    _check_bounds(report)
    if len(canonical_json_bytes(report)) > MAX_JSON_BYTES:
        raise ContractError("report exceeds PE-6 bound")
    return dict(report)
