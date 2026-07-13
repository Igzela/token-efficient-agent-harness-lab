#!/usr/bin/env python3
"""Run the PE-6 allowlisted drill registry and emit bounded evidence."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts.fault_drill_contract import ContractError, validate_report, write_canonical_json
from scripts.fault_drill_harness import report_for, run_selected
from scripts.fault_drill_registry import SUITE_NAMES, get_scenario, scenario_ids_for, validate_registry


def _source_head() -> str:
    completed = subprocess.run(
        ("git", "rev-parse", "HEAD"),
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
        timeout=5,
    )
    source_head = completed.stdout.strip()
    if len(source_head) != 40 or any(character not in "0123456789abcdef" for character in source_head):
        raise ContractError("git did not return a full source commit")
    return source_head


def _safe_output(path: Path) -> Path:
    resolved = path.expanduser().resolve()
    allowed_roots = [Path(tempfile.gettempdir()).resolve()]
    configured = os.environ.get("ACP_PE6_OUTPUT_ROOT")
    if configured:
        allowed_roots.append(Path(configured).expanduser().resolve())
    if not any(resolved == root or root in resolved.parents for root in allowed_roots):
        raise ContractError("report output must be inside a disposable temp/output root")
    return resolved


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument("--suite", choices=sorted(SUITE_NAMES))
    selection.add_argument("--scenario-id")
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--worker", type=int, default=0)
    parser.add_argument("--output")
    parser.add_argument("--format", choices=("human", "json"), default="human")
    parser.add_argument(
        "--require-supported",
        action="store_true",
        help="return failure when an otherwise valid environment is unavailable",
    )
    return parser


def _check_numeric(name: str, value: int) -> None:
    if isinstance(value, bool) or not 0 <= value <= 2**32 - 1:
        raise ContractError(f"{name} must be between 0 and 2^32-1")


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        validate_registry()
        _check_numeric("seed", args.seed)
        _check_numeric("worker", args.worker)
        scenario_ids = scenario_ids_for(suite=args.suite, scenario_id=args.scenario_id)
        source_head = _source_head()
        results = run_selected(
            scenario_ids,
            source_head=source_head,
            seed=args.seed,
            worker_id=args.worker,
        )
        report = report_for(
            suite=args.suite or "scenario",
            source_head=source_head,
            seed=args.seed,
            worker_id=args.worker,
            results=results,
        )
        validate_report(report)
        if args.output:
            write_canonical_json(_safe_output(Path(args.output)), report)

        if args.format == "json":
            print(json.dumps(report, sort_keys=True, separators=(",", ":")))
        else:
            for result in report["results"]:
                print(
                    f"{result['scenario_id']} status={result['status']} "
                    f"duration_ms={result['duration_ms']} "
                    f"cleanup={result['cleanup_evidence']['outcome']}"
                )
            print(f"report_sha256={report['report_sha256']}")

        statuses = {result["status"] for result in report["results"]}
        failed = statuses & {"failed_recovery", "failed_rollback", "cleanup_failed", "invalid_scenario", "aborted"}
        if failed:
            return 1
        if args.require_supported and "unsupported" in statuses:
            return 1
        return 0
    except (ContractError, OSError, subprocess.SubprocessError) as exc:
        print(f"fault drill refused: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
