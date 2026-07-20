#!/usr/bin/env python3
"""Fail closed when public README reintroduces known-bad install claims."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"

# Patterns that must not reappear on the public entry surface.
FORBIDDEN = [
    (
        re.compile(r"cargo install --git \S+ agent-control-plane\s*$", re.M),
        "cargo install must use package `engine` with --bin agent-control-plane",
    ),
    (
        re.compile(r"igzela/agent-control-plane:latest"),
        "unverified public Docker :latest image must not be advertised",
    ),
    (
        # Allow prose that forbids :latest; reject instructional docker run lines that pin :latest.
        re.compile(r"^\s*(docker run\b[^\n]*:latest)", re.M),
        "docker run …:latest must not be advertised as a verified path",
    ),
    (
        re.compile(r"tests-\d+%20passing|1654|1379\+"),
        "hard-coded test counts must not appear on the public README",
    ),
]

REQUIRED = [
    (re.compile(r"\./scripts/demo\.sh"), "README must point at ./scripts/demo.sh"),
    (
        re.compile(r"\./scripts/external_validation\.sh"),
        "README must point at ./scripts/external_validation.sh",
    ),
    (re.compile(r"engine --bin agent-control-plane"), "cargo install form must select package engine"),
    (re.compile(r"docs/CURRENT_STATUS\.md"), "README must link current status for operational limits"),
]


def main() -> int:
    text = README.read_text(encoding="utf-8")
    failures: list[str] = []
    for pattern, message in FORBIDDEN:
        if pattern.search(text):
            failures.append(f"forbidden: {message}")
    for pattern, message in REQUIRED:
        if not pattern.search(text):
            failures.append(f"missing: {message}")
    if failures:
        print("README public surface check FAILED:", file=sys.stderr)
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1
    print("README public surface check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
