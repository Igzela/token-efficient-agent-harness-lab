#!/usr/bin/env python3
"""CLI for the Harness App MVP0 read-only instance auditor."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC_ROOT = REPO_ROOT / "src"
if str(SRC_ROOT) not in sys.path:
    sys.path.insert(0, str(SRC_ROOT))

from harness_core.instance_audit import audit_instance, format_report  # noqa: E402


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Read-only audit of a target repository's harness instance controls."
    )
    parser.add_argument(
        "--target-repo",
        required=True,
        help="Path to the target repository to audit.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit machine-readable JSON instead of a human-readable report.",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    report = audit_instance(args.target_repo)
    if args.json:
        print(report.to_json())
    else:
        print(format_report(report))
    return 2 if report.verdict == "BLOCKED" else 0


if __name__ == "__main__":
    raise SystemExit(main())
