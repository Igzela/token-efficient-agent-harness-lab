#!/usr/bin/env python3
"""Build a canonical hash-bound final live-acceptance seal.

The aggregator consumes bounded evidence envelopes, not command logs or raw
artifacts. Missing, skipped, unsupported, stale, or non-PASS evidence always
produces a BLOCKED seal. Publication is impossible without an explicit flag.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import re
import sys
from datetime import datetime, timezone
from typing import Any, Sequence


COMPONENTS = (
    "orchestrator",
    "target_output",
    "lifecycle",
    "fault_drills",
    "provider",
    "pe2_pe4",
    "agent_runtime_memory",
    "langgraph_benchmark",
)
STATUSES = frozenset({"PASS", "BLOCKED", "FAIL", "SKIP", "UNSUPPORTED"})
ENVIRONMENTS = frozenset({"fixture", "local", "staging", "live"})
RELEASE_STATES = frozenset({"RELEASE_READY_NOT_PUBLISHED", "PUBLISHED", "BLOCKED"})
MAX_EVIDENCE_BYTES = 64 * 1024
MAX_REASON_CODES = 20
MAX_ARTIFACT_HASHES = 32
SHA_RE = re.compile(r"[0-9a-f]{40}|[0-9a-f]{64}")
HASH_RE = re.compile(r"[0-9a-f]{64}")
ID_RE = re.compile(r"[a-z0-9][a-z0-9_.-]{0,127}")
REASON_RE = re.compile(r"[a-z0-9][a-z0-9_-]{0,95}")
ALLOWED_FIELDS = frozenset({
    "schema_version",
    "component",
    "source_head",
    "status",
    "command_id",
    "environment_class",
    "artifact_hashes",
    "reason_codes",
    "completed_at",
})


class AcceptanceError(ValueError):
    """Raised for unsafe or malformed evidence and output configuration."""


def _canonical(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")


def _parse_timestamp(value: Any) -> str:
    if not isinstance(value, str) or len(value) > 40 or not value.endswith("Z"):
        raise AcceptanceError("timestamp_invalid")
    try:
        datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError:
        raise AcceptanceError("timestamp_invalid") from None
    return value


def _validate_string_list(value: Any, *, limit: int, pattern: re.Pattern[str], code: str) -> list[str]:
    if not isinstance(value, list) or len(value) > limit:
        raise AcceptanceError(code)
    if any(not isinstance(item, str) or pattern.fullmatch(item) is None for item in value):
        raise AcceptanceError(code)
    if len(set(value)) != len(value):
        raise AcceptanceError(code)
    return sorted(value)


def _read_evidence(path: pathlib.Path, expected_component: str, source_head: str) -> tuple[dict[str, Any], str]:
    if path.is_symlink() or not path.is_file():
        raise AcceptanceError("evidence_not_regular_file")
    size = path.stat().st_size
    if size <= 0 or size > MAX_EVIDENCE_BYTES:
        raise AcceptanceError("evidence_size_invalid")
    raw = path.read_bytes()
    if len(raw) != size:
        raise AcceptanceError("evidence_changed_during_read")
    try:
        value = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError):
        raise AcceptanceError("evidence_json_invalid") from None
    if not isinstance(value, dict) or set(value) != ALLOWED_FIELDS:
        raise AcceptanceError("evidence_schema_invalid")
    if value["schema_version"] != "final-live-acceptance-evidence.v1":
        raise AcceptanceError("evidence_version_invalid")
    if value["component"] != expected_component:
        raise AcceptanceError("evidence_component_mismatch")
    if value["source_head"] != source_head:
        raise AcceptanceError("evidence_source_head_mismatch")
    if value["status"] not in STATUSES:
        raise AcceptanceError("evidence_status_invalid")
    if not isinstance(value["command_id"], str) or ID_RE.fullmatch(value["command_id"]) is None:
        raise AcceptanceError("evidence_command_id_invalid")
    if value["environment_class"] not in ENVIRONMENTS:
        raise AcceptanceError("evidence_environment_invalid")
    hashes = _validate_string_list(
        value["artifact_hashes"], limit=MAX_ARTIFACT_HASHES, pattern=HASH_RE,
        code="evidence_artifact_hashes_invalid",
    )
    reasons = _validate_string_list(
        value["reason_codes"], limit=MAX_REASON_CODES, pattern=REASON_RE,
        code="evidence_reason_codes_invalid",
    )
    if value["status"] != "PASS" and not reasons:
        raise AcceptanceError("evidence_failure_reason_missing")
    completed_at = _parse_timestamp(value["completed_at"])
    summary = {
        "artifact_hashes": hashes,
        "command_id": value["command_id"],
        "completed_at": completed_at,
        "environment_class": value["environment_class"],
        "evidence_sha256": hashlib.sha256(raw).hexdigest(),
        "reason_codes": reasons,
        "status": value["status"],
    }
    return summary, summary["evidence_sha256"]


def build_report(
    *,
    source_head: str,
    evidence: dict[str, pathlib.Path],
    required_components: Sequence[str] = COMPONENTS,
    generated_at: str,
    requested_release_state: str = "RELEASE_READY_NOT_PUBLISHED",
    publication_authorized: bool = False,
    max_evidence_age_hours: int = 24,
) -> dict[str, Any]:
    if SHA_RE.fullmatch(source_head) is None:
        raise AcceptanceError("source_head_invalid")
    if requested_release_state not in RELEASE_STATES:
        raise AcceptanceError("release_state_invalid")
    if requested_release_state == "PUBLISHED" and not publication_authorized:
        raise AcceptanceError("publication_not_authorized")
    generated_at = _parse_timestamp(generated_at)
    if type(max_evidence_age_hours) is not int or not 1 <= max_evidence_age_hours <= 720:
        raise AcceptanceError("evidence_freshness_window_invalid")
    generated_time = datetime.fromisoformat(generated_at[:-1] + "+00:00")
    required = tuple(required_components)
    if not required or len(set(required)) != len(required) or any(item not in COMPONENTS for item in required):
        raise AcceptanceError("required_components_invalid")
    unknown = set(evidence) - set(required)
    if unknown:
        raise AcceptanceError("unexpected_evidence_component")

    summaries: dict[str, dict[str, Any]] = {}
    for component in required:
        path = evidence.get(component)
        if path is None:
            summaries[component] = {
                "artifact_hashes": [],
                "command_id": "not-run",
                "completed_at": generated_at,
                "environment_class": "local",
                "evidence_sha256": None,
                "reason_codes": ["missing_evidence"],
                "status": "BLOCKED",
            }
            continue
        summary, _ = _read_evidence(path, component, source_head)
        completed_time = datetime.fromisoformat(summary["completed_at"][:-1] + "+00:00")
        age_seconds = (generated_time - completed_time).total_seconds()
        if age_seconds < 0 or age_seconds > max_evidence_age_hours * 3600:
            summary["status"] = "BLOCKED"
            summary["reason_codes"] = sorted(set(summary["reason_codes"] + ["stale_evidence"]))
        summaries[component] = summary

    passed = all(item["status"] == "PASS" for item in summaries.values())
    overall = "PASS" if passed else "BLOCKED"
    release_state = requested_release_state if passed else "BLOCKED"
    report: dict[str, Any] = {
        "components": summaries,
        "generated_at": generated_at,
        "kind": "final-live-acceptance-seal",
        "max_evidence_age_hours": max_evidence_age_hours,
        "publication_authorized": publication_authorized,
        "release_state": release_state,
        "source_head": source_head,
        "status": overall,
        "version": 1,
    }
    report["report_sha256"] = hashlib.sha256(_canonical(report)).hexdigest()
    return report


def _evidence_arguments(values: Sequence[str]) -> dict[str, pathlib.Path]:
    result: dict[str, pathlib.Path] = {}
    for value in values:
        component, separator, raw_path = value.partition("=")
        if not separator or component not in COMPONENTS or not raw_path or component in result:
            raise AcceptanceError("evidence_argument_invalid")
        result[component] = pathlib.Path(raw_path)
    return result


def _write_report(output_root: pathlib.Path, output_name: str, report: dict[str, Any]) -> pathlib.Path:
    if pathlib.Path(output_name).is_absolute() or pathlib.Path(output_name).name != output_name:
        raise AcceptanceError("output_name_invalid")
    root = output_root.resolve()
    if not root.is_dir():
        raise AcceptanceError("output_root_invalid")
    output = root / output_name
    if output.is_symlink():
        raise AcceptanceError("output_symlink_refused")
    temporary = root / f".{output_name}.{os.getpid()}.tmp"
    try:
        temporary.write_bytes(_canonical(report))
        temporary.replace(output)
    finally:
        temporary.unlink(missing_ok=True)
    return output


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-head", required=True)
    parser.add_argument("--evidence", action="append", default=[], metavar="COMPONENT=FILE")
    parser.add_argument("--required-component", action="append", choices=COMPONENTS)
    parser.add_argument("--output-root", required=True)
    parser.add_argument("--output-name", default="final-live-acceptance.json")
    parser.add_argument(
        "--release-state", choices=sorted(RELEASE_STATES),
        default="RELEASE_READY_NOT_PUBLISHED",
    )
    parser.add_argument("--publication-authorized", action="store_true")
    parser.add_argument("--max-evidence-age-hours", type=int, default=24)
    parser.add_argument("--generated-at")
    args = parser.parse_args(argv)
    generated_at = args.generated_at or datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
    try:
        report = build_report(
            source_head=args.source_head,
            evidence=_evidence_arguments(args.evidence),
            required_components=args.required_component or COMPONENTS,
            generated_at=generated_at,
            requested_release_state=args.release_state,
            publication_authorized=args.publication_authorized,
            max_evidence_age_hours=args.max_evidence_age_hours,
        )
        output = _write_report(pathlib.Path(args.output_root), args.output_name, report)
    except (AcceptanceError, OSError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(json.dumps({
        "output_name": output.name,
        "release_state": report["release_state"],
        "report_sha256": report["report_sha256"],
        "status": report["status"],
    }, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
