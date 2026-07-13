#!/usr/bin/env python3
"""Validate Codex review structured output against the JSON schema and output verdict to GITHUB_OUTPUT."""

import json
import os
import sys
import pathlib

from jsonschema import Draft202012Validator


SCHEMA_PATH = pathlib.Path(__file__).resolve().parent / "review_schema.json"


def load_schema():
    with open(SCHEMA_PATH) as f:
        return json.load(f)


def validate(data, schema):
    validator = Draft202012Validator(schema)
    return [error.message for error in sorted(validator.iter_errors(data), key=str)]


def write_output(verdict, summary, reviewed_sha, verdict_details):
    output_file = os.environ.get("GITHUB_OUTPUT")
    if output_file:
        with open(output_file, "a") as f:
            f.write(f"verdict={verdict}\n")
            f.write(f"summary={summary}\n")
            f.write(f"reviewed_sha={reviewed_sha}\n")
            if verdict_details:
                f.write(f"blockers={json.dumps(verdict_details.get('blockers', []))}\n")

    print(f"verdict={verdict}")
    print(f"summary={summary}")
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

    verdict = data.get("verdict", "FAIL")
    summary = data.get("summary", "No summary provided")
    reviewed_sha = data.get("reviewed_head_sha", expected_sha)

    if reviewed_sha != expected_sha:
        errors.append(f"Reviewed SHA mismatch: expected {expected_sha}, got {reviewed_sha}")
        verdict = "FAIL"

    if errors:
        print(f"Validation errors:", file=sys.stderr)
        for err in errors:
            print(f"  - {err}", file=sys.stderr)
        if verdict not in ("BLOCKED", "FAIL"):
            verdict = "BLOCKED"

    write_output(verdict, summary, reviewed_sha, data)

    if verdict in ("BLOCKED", "FAIL"):
        sys.exit(1)


if __name__ == "__main__":
    main()
