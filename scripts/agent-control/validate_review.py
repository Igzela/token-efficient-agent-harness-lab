#!/usr/bin/env python3
"""Validate Codex review structured output against the JSON schema and output verdict to GITHUB_OUTPUT."""

import json
import os
import sys
import pathlib
import re
import uuid


SCHEMA_PATH = pathlib.Path(__file__).resolve().parent / "review_schema.json"
MAX_REVIEW_ARTIFACT_BYTES = 64 * 1024


def load_schema():
    with open(SCHEMA_PATH) as f:
        return json.load(f)


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


def _write_github_output(handle, key, value):
    text = str(value)
    delimiter = f"agent_output_{uuid.uuid4().hex}"
    while delimiter in text:
        delimiter = f"agent_output_{uuid.uuid4().hex}"
    handle.write(f"{key}<<{delimiter}\n{text}\n{delimiter}\n")


def write_output(verdict, summary, reviewed_sha, verdict_details):
    output_file = os.environ.get("GITHUB_OUTPUT")
    if output_file:
        with open(output_file, "a") as f:
            _write_github_output(f, "verdict", verdict)
            _write_github_output(f, "summary", summary)
            _write_github_output(f, "reviewed_sha", reviewed_sha)
            if verdict_details:
                _write_github_output(f, "blockers", json.dumps(verdict_details.get("blockers", [])))

    print(f"verdict={verdict}")
    print(f"summary={json.dumps(summary)}")
    print(f"reviewed_sha={reviewed_sha}")


def main():
    if len(sys.argv) < 4:
        print("Usage: validate_review.py <output-file> <pr-number> <expected-head-sha>", file=sys.stderr)
        sys.exit(1)

    output_path = sys.argv[1]
    pr_number = int(sys.argv[2])
    expected_sha = sys.argv[3]

    if not os.path.exists(output_path):
        print(f"FATAL: Output file not found: {output_path}", file=sys.stderr)
        write_output("FAIL", "Codex produced no output", expected_sha, {})
        sys.exit(1)

    if os.path.getsize(output_path) > MAX_REVIEW_ARTIFACT_BYTES:
        print(f"FATAL: Output file exceeds {MAX_REVIEW_ARTIFACT_BYTES} bytes", file=sys.stderr)
        write_output("FAIL", "Codex output exceeded the review artifact bound", expected_sha, {})
        sys.exit(1)

    with open(output_path) as f:
        content = f.read().strip()

    if not content:
        print(f"FATAL: Output file is empty: {output_path}", file=sys.stderr)
        write_output("FAIL", "Codex produced empty output", expected_sha, {})
        sys.exit(1)

    try:
        data = json.loads(content)
    except json.JSONDecodeError as e:
        print(f"FATAL: Invalid JSON in output: {e}", file=sys.stderr)
        write_output("FAIL", f"Invalid JSON output: {e}", expected_sha, {})
        sys.exit(1)

    schema = load_schema()
    errors = validate(data, schema)

    result = data if isinstance(data, dict) else {}
    verdict = result.get("verdict", "FAIL")
    summary = result.get("summary", "No summary provided")
    reviewed_sha = result.get("reviewed_head_sha", expected_sha)

    if isinstance(summary, str) and ("\0" in summary or "\r" in summary):
        errors.append("Summary contains an unsupported control character")

    if verdict == "PASS":
        if result.get("blockers"):
            errors.append("PASS must not contain blockers")
        for field in ("ci_green", "security_ok", "rollback_ok"):
            if result.get(field) is not True:
                errors.append(f"PASS requires {field}=true")

    if reviewed_sha != expected_sha:
        errors.append(f"Reviewed SHA mismatch: expected {expected_sha}, got {reviewed_sha}")
        verdict = "FAIL"

    if errors:
        print(f"Validation errors:", file=sys.stderr)
        for err in errors:
            print(f"  - {err}", file=sys.stderr)
        if verdict not in ("BLOCKED", "FAIL"):
            verdict = "BLOCKED"

    write_output(verdict, summary, reviewed_sha, result)

    if verdict in ("BLOCKED", "FAIL"):
        sys.exit(1)


if __name__ == "__main__":
    main()
