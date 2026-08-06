#!/usr/bin/env python3
"""Validate bounded Codex review output without authorizing its content.

Schema-valid exact-head verdicts are normal business outcomes.  The caller
receives a small, trusted validation sidecar for durable recording; malformed
or unavailable artifacts instead produce a bounded failure sidecar and a
non-zero exit status.  Raw model output is never copied to GitHub workflow
outputs.
"""

import hashlib
import json
import os
import pathlib
import re
import sys
import uuid


SCHEMA_PATH = pathlib.Path(__file__).resolve().parent / "review_schema.json"
MAX_REVIEW_ARTIFACT_BYTES = 64 * 1024
VALID_VERDICTS = frozenset({"PASS", "PASS_WITH_NOTES", "BLOCKED", "FAIL"})
# Align with state_manager.MAX_REPAIR_ATTEMPTS: R1 + at most one autonomous repair head / R2.
MAX_AUTONOMOUS_REVIEW_REPAIR_ROUNDS = 2


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
        elif expected_type == "array":
            if not isinstance(value, list):
                errors.append(f"{key} must be an array")
                continue
            if len(value) > int(rule.get("maxItems", len(value))):
                errors.append(f"{key} has too many items")
            item_rule = rule.get("items", {})
            for item in value:
                if item_rule.get("type") == "string" and not isinstance(item, str):
                    errors.append(f"{key} items must be strings")
                    continue
                if isinstance(item, str) and len(item) > int(
                    item_rule.get("maxLength", len(item))
                ):
                    errors.append(f"{key} item is too long")
        else:
            errors.append(f"unsupported schema type for {key}")
    return errors


def _contains_unsupported_control_characters(value):
    if isinstance(value, str):
        return "\0" in value or "\r" in value
    if isinstance(value, list):
        return any(_contains_unsupported_control_characters(item) for item in value)
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
        "version": 1,
        "classification": "invalid_artifact",
        "pr_number": pr_number,
        "reviewed_head_sha": expected_sha,
        "failure_code": failure_code,
        "artifact_sha256": artifact_sha256,
        "review_workflow_run_id": _workflow_run_id(),
    }


def _valid_sidecar(pr_number, result, artifact_sha256):
    return {
        "kind": "agent-orchestrator-review-validation",
        "version": 1,
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
        for key in ("summary", "blockers", "major_notes", "minor_notes"):
            if _contains_unsupported_control_characters(result.get(key)):
                errors.append(f"{key} contains an unsupported control character")
    if errors:
        return invalid_result(validation_path, pr_number, expected_sha, "artifact_schema_invalid", artifact_sha256)

    # The schema has established these types.  Keep semantic authorization
    # checks separate: only exact PASS is merge-authorizing. PASS may carry
    # deferred major_notes/minor_notes; PASS_WITH_NOTES remains non-authorizing.
    if result["reviewed_head_sha"] != expected_sha:
        return invalid_result(validation_path, pr_number, expected_sha, "reviewed_head_mismatch", artifact_sha256)
    if result["verdict"] == "PASS":
        if result.get("blockers"):
            return invalid_result(validation_path, pr_number, expected_sha, "pass_has_blockers", artifact_sha256)
        if any(result[field] is not True for field in ("ci_green", "security_ok", "rollback_ok")):
            return invalid_result(validation_path, pr_number, expected_sha, "pass_proof_missing", artifact_sha256)
    elif result["verdict"] == "PASS_WITH_NOTES":
        # Schema-valid recording only. Blockers would make the record contradictory.
        if result.get("blockers"):
            return invalid_result(
                validation_path, pr_number, expected_sha, "pass_with_notes_has_blockers", artifact_sha256
            )
    elif result["verdict"] == "BLOCKED":
        blockers = result.get("blockers") or []
        if not blockers:
            return invalid_result(
                validation_path, pr_number, expected_sha, "blocked_without_blockers", artifact_sha256
            )
    if result["verdict"] not in VALID_VERDICTS:
        return invalid_result(validation_path, pr_number, expected_sha, "unsupported_verdict", artifact_sha256)

    write_validation_sidecar(validation_path, _valid_sidecar(pr_number, result, artifact_sha256))
    write_workflow_outputs("valid_verdict", result["verdict"], expected_sha, artifact_sha256)
    return 0


if __name__ == "__main__":
    sys.exit(main())
