#!/usr/bin/env python3
"""Validate bounded Codex review output without authorizing its content.

Schema-valid exact-head verdicts are normal business outcomes.  The caller
receives a small, trusted validation sidecar for durable recording; malformed
or unavailable artifacts instead produce a bounded failure sidecar and a
non-zero exit status.  Raw model output is never copied to GitHub workflow
outputs.

Review Convergence Protocol: cross-field verdict rules are enforced by the
single canonical pure owner `review_convergence.py` (severity vs disposition,
exact PASS as the only merge-authorizing verdict, R1/R2 rounds, no autonomous
R3).  `ci_green` is a reviewer observation only and never authorizes PASS;
authoritative CI is read independently from trusted GitHub state by the merge
owner.
"""

import hashlib
import json
import os
import pathlib
import re
import sys
import uuid

import review_convergence as rc


SCHEMA_PATH = pathlib.Path(__file__).resolve().parent / "review_schema.json"
MAX_REVIEW_ARTIFACT_BYTES = 64 * 1024


def load_schema():
    with open(SCHEMA_PATH, encoding="utf-8") as handle:
        return json.load(handle)


def validate(data, schema):
    """Validate the bounded schema subset used by review_schema.json."""

    if not isinstance(data, dict):
        return ["review result is not an object"]
    errors = []
    required = schema.get("required", [])
    properties = schema.get("properties", {})
    for key in required:
        if key not in data:
            errors.append(f"{key} is required")
    if schema.get("additionalProperties") is False:
        for key in sorted(set(data) - set(properties)):
            errors.append(f"unsupported property: {key}")
    for key, value in data.items():
        rule = properties.get(key)
        if not isinstance(rule, dict):
            continue
        expected_type = rule.get("type")
        if expected_type == "string":
            if not isinstance(value, str):
                errors.append(f"{key} must be a string")
                continue
            if len(value) < int(rule.get("minLength", 0)):
                errors.append(f"{key} is too short")
            if len(value) > int(rule.get("maxLength", len(value))):
                errors.append(f"{key} is too long")
            if "enum" in rule and value not in rule["enum"]:
                errors.append(f"{key} is not an allowed value")
            if "pattern" in rule and re.fullmatch(rule["pattern"], value) is None:
                errors.append(f"{key} does not match the required pattern")
        elif expected_type == "boolean":
            if type(value) is not bool:
                errors.append(f"{key} must be a boolean")
        elif expected_type == "integer":
            if type(value) is not int:
                errors.append(f"{key} must be an integer")
            elif value < int(rule.get("minimum", value)) or value > int(
                rule.get("maximum", value)
            ):
                errors.append(f"{key} is outside the allowed range")
        elif expected_type == "array":
            if not isinstance(value, list):
                errors.append(f"{key} must be an array")
                continue
            if len(value) > int(rule.get("maxItems", len(value))):
                errors.append(f"{key} has too many items")
            item_rule = rule.get("items", {})
            if isinstance(item_rule, dict) and item_rule.get("type") == "object":
                for item in value:
                    if not isinstance(item, dict):
                        errors.append(f"{key} items must be objects")
                        continue
                    errors.extend(_validate_finding(item, key))
            elif isinstance(item_rule, dict) and item_rule.get("type") == "string":
                for item in value:
                    if not isinstance(item, str):
                        errors.append(f"{key} items must be strings")
                        continue
                    if len(item) > int(item_rule.get("maxLength", len(item))):
                        errors.append(f"{key} item is too long")
        else:
            errors.append(f"unsupported schema type for {key}")
    return errors


def _validate_finding(item, field):
    """Structural finding validation against the schema's finding object rule."""
    errors = []
    required = {
        "id",
        "axis",
        "evidence",
        "severity",
        "disposition",
        "scope_relation",
        "origin_head",
        "acceptance_condition",
        "status",
    }
    for key in sorted(required - set(item)):
        errors.append(f"{field} finding missing {key}")
    allowed = required | {"admission_reason"}
    for key in sorted(set(item) - allowed):
        errors.append(f"{field} finding has unsupported key {key}")
    enums = {
        "severity": {"blocker", "major", "minor", "note"},
        "disposition": {"block_current_head", "defer", "decision_required"},
        "scope_relation": {"in_packet", "out_of_packet"},
        "status": {"open", "resolved", "deferred"},
    }
    for key, values in enums.items():
        if key in item and item[key] not in values:
            errors.append(f"{field} finding has invalid {key}")
    if "admission_reason" in item and item["admission_reason"] not in rc.ADMISSION_REASONS:
        errors.append(f"{field} finding has invalid admission_reason")
    for key, max_len in (
        ("id", 160),
        ("axis", 200),
        ("evidence", 12000),
        ("acceptance_condition", 4000),
    ):
        value = item.get(key)
        if isinstance(value, str) and len(value) > max_len:
            errors.append(f"{field} finding {key} is too long")
    if "origin_head" in item and re.fullmatch(r"[0-9a-f]{40}", str(item["origin_head"])) is None:
        errors.append(f"{field} finding origin_head must be 40-hex")
    return errors


def _contains_unsupported_control_characters(value):
    if isinstance(value, str):
        return "\0" in value or "\r" in value
    if isinstance(value, list):
        return any(_contains_unsupported_control_characters(item) for item in value)
    if isinstance(value, dict):
        return any(
            _contains_unsupported_control_characters(item) for item in value.values()
        )
    return False


def _write_github_output(handle, key, value):
    text = str(value)
    delimiter = f"agent_output_{uuid.uuid4().hex}"
    while delimiter in text:
        delimiter = f"agent_output_{uuid.uuid4().hex}"
    handle.write(f"{key}<<{delimiter}\n{text}\n{delimiter}\n")


def write_workflow_outputs(classification, verdict, reviewed_sha, artifact_sha256, failure_code=""):
    """Expose only fixed/bounded metadata, never model-authored review text."""

    output_file = os.environ.get("GITHUB_OUTPUT")
    if output_file:
        with open(output_file, "a", encoding="utf-8") as handle:
            _write_github_output(handle, "classification", classification)
            _write_github_output(handle, "verdict", verdict)
            _write_github_output(handle, "reviewed_sha", reviewed_sha)
            _write_github_output(handle, "artifact_sha256", artifact_sha256 or "")
            _write_github_output(handle, "failure_code", failure_code)
    print(json.dumps({
        "classification": classification,
        "verdict": verdict,
        "reviewed_head_sha": reviewed_sha,
        "artifact_sha256": artifact_sha256,
        "failure_code": failure_code,
    }, sort_keys=True))


def write_validation_sidecar(path, payload):
    """Write the trusted bounded classification atomically for the next step."""

    if not path:
        return
    destination = pathlib.Path(path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f".{destination.name}.{uuid.uuid4().hex}.tmp")
    with open(temporary, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, sort_keys=True, separators=(",", ":"))
        handle.write("\n")
    os.replace(temporary, destination)


def _workflow_run_id():
    value = os.environ.get("GITHUB_RUN_ID", "")
    return int(value) if value.isdigit() else None


def _invalid_sidecar(pr_number, expected_sha, failure_code, artifact_sha256=None):
    return {
        "kind": "agent-orchestrator-review-validation",
        "version": 2,
        "classification": "invalid_artifact",
        "pr_number": pr_number,
        "reviewed_head_sha": expected_sha,
        "failure_code": failure_code,
        "artifact_sha256": artifact_sha256,
        "review_workflow_run_id": _workflow_run_id(),
    }


def _valid_sidecar(pr_number, result, artifact_sha256):
    """Trusted bounded sidecar; includes convergence fields when present."""
    sidecar = {
        "kind": "agent-orchestrator-review-validation",
        "version": 2,
        "classification": "valid_verdict",
        "pr_number": pr_number,
        "reviewed_head_sha": result["reviewed_head_sha"],
        "verdict": result["verdict"],
        "summary": result["summary"],
        "blockers": list(result.get("blockers", [])),
        "major_notes": list(result.get("major_notes", [])),
        "minor_notes": list(result.get("minor_notes", [])),
        "artifact_sha256": artifact_sha256,
        "review_workflow_run_id": _workflow_run_id(),
    }
    for key in (
        "review_mode",
        "review_round",
        "reviewed_base",
        "reviewed_range",
        "prior_reviewed_head",
        "findings",
        "finding_ledger_digest",
        "open_blocker_ids",
        "deferred_note_ids",
        "decision_required_ids",
        "observed_ci_status",
    ):
        if result.get(key) not in (None, []):
            sidecar[key] = result[key]
    return sidecar


def invalid_result(validation_path, pr_number, expected_sha, failure_code, artifact_sha256=None):
    sidecar = _invalid_sidecar(pr_number, expected_sha, failure_code, artifact_sha256)
    write_validation_sidecar(validation_path, sidecar)
    write_workflow_outputs(
        "invalid_artifact", "INVALID", expected_sha, artifact_sha256 or "", failure_code
    )
    print(f"FATAL: review artifact rejected ({failure_code})", file=sys.stderr)
    return 1


def main():
    if len(sys.argv) not in (4, 5):
        print(
            "Usage: validate_review.py <output-file> <pr-number> <expected-head-sha> [validation-sidecar]",
            file=sys.stderr,
        )
        return 1

    output_path = pathlib.Path(sys.argv[1])
    try:
        pr_number = int(sys.argv[2])
    except ValueError:
        print("FATAL: invalid PR number", file=sys.stderr)
        return 1
    expected_sha = sys.argv[3]
    validation_path = sys.argv[4] if len(sys.argv) == 5 else None
    if re.fullmatch(r"[0-9a-f]{40}", expected_sha) is None:
        return invalid_result(validation_path, pr_number, expected_sha, "invalid_expected_head")

    try:
        raw = output_path.read_bytes()
    except OSError:
        return invalid_result(validation_path, pr_number, expected_sha, "artifact_missing")
    artifact_sha256 = hashlib.sha256(raw).hexdigest()
    if len(raw) > MAX_REVIEW_ARTIFACT_BYTES:
        return invalid_result(
            validation_path, pr_number, expected_sha, "artifact_too_large", artifact_sha256
        )
    if not raw.strip():
        return invalid_result(validation_path, pr_number, expected_sha, "artifact_empty", artifact_sha256)
    try:
        content = raw.decode("utf-8")
    except UnicodeDecodeError:
        return invalid_result(validation_path, pr_number, expected_sha, "artifact_not_utf8", artifact_sha256)
    try:
        result = json.loads(content)
    except json.JSONDecodeError:
        return invalid_result(validation_path, pr_number, expected_sha, "artifact_invalid_json", artifact_sha256)

    errors = validate(result, load_schema())
    if isinstance(result, dict):
        for key in ("summary", "blockers", "major_notes", "minor_notes", "findings"):
            if _contains_unsupported_control_characters(result.get(key)):
                errors.append(f"{key} contains an unsupported control character")
    if errors:
        return invalid_result(validation_path, pr_number, expected_sha, "artifact_schema_invalid", artifact_sha256)

    # The schema has established types.  Semantic authorization checks are
    # delegated to the canonical pure owner so validator, prompt, and durable
    # state share one cross-field contract.
    if result["reviewed_head_sha"] != expected_sha:
        return invalid_result(validation_path, pr_number, expected_sha, "reviewed_head_mismatch", artifact_sha256)
    try:
        decision = rc.decision_from_legacy_artifact(
            result,
            base_sha=str(result.get("reviewed_base") or ""),
        )
    except rc.ConvergenceError as exc:
        return invalid_result(
            validation_path, pr_number, expected_sha, "convergence_cross_field_invalid", artifact_sha256
        )

    if "findings" in result and result["findings"] is not None:
        expected_digest = rc.ledger_digest(
            tuple(decision.findings)
        )
        provided_digest = result.get("finding_ledger_digest")
        if provided_digest and provided_digest != expected_digest:
            return invalid_result(
                validation_path, pr_number, expected_sha, "finding_ledger_digest_mismatch", artifact_sha256
            )
        result["finding_ledger_digest"] = expected_digest

    result.setdefault("review_mode", decision.review_mode)
    result["review_round"] = decision.review_round
    if isinstance(result.get("reviewed_base"), str) and re.fullmatch(
        r"[0-9a-f]{40}", result["reviewed_base"]
    ):
        # The artifact supplied a real base; keep the derived exact range.
        result["reviewed_range"] = decision.reviewed_range
    else:
        # No trusted base in the artifact: leave binding to the record path,
        # which consults the trusted GitHub PR base (never a zero default).
        result.pop("reviewed_base", None)
        result.pop("reviewed_range", None)
    result["prior_reviewed_head"] = decision.prior_reviewed_head
    result["findings"] = [f.to_dict() for f in decision.findings]
    result["open_blocker_ids"] = list(decision.open_blocker_ids)
    result["deferred_note_ids"] = list(decision.deferred_note_ids)
    result["decision_required_ids"] = list(decision.decision_required_ids)
    result["observed_ci_status"] = decision.observed_ci_status

    write_validation_sidecar(validation_path, _valid_sidecar(pr_number, result, artifact_sha256))
    write_workflow_outputs("valid_verdict", result["verdict"], expected_sha, artifact_sha256)
    return 0


if __name__ == "__main__":
    sys.exit(main())
