"""Command-line wrapper for the Stage 1 harness core library."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Sequence

from .digest import generate_batch_digest
from .errors import ReplayPreflightError
from .event_store import ValidationIssue, replay_preflight, validate_jsonl_file
from .projection_store import replay_all, replay_project_state, replay_task_queue_state


def main(argv: Sequence[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)
    return args.func(args)


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="harness-core")
    subparsers = parser.add_subparsers(dest="command", required=True)

    validate = subparsers.add_parser("validate-events", help="validate an events.jsonl file")
    validate.add_argument("path")
    validate.add_argument(
        "--verbose",
        action="store_true",
        help="run replay preflight even when JSONL validation already failed",
    )
    validate.set_defaults(func=_cmd_validate_events)

    project_state = subparsers.add_parser("project-state", help="print project item statuses")
    project_state.add_argument("path")
    project_state.set_defaults(func=_cmd_project_state)

    task_queue = subparsers.add_parser("task-queue", help="print task queue handoff projection")
    task_queue.add_argument("path")
    task_queue.set_defaults(func=_cmd_task_queue)

    digest = subparsers.add_parser("digest", help="print digest stub summary")
    digest.add_argument("path")
    digest.set_defaults(func=_cmd_digest)

    return parser


def _cmd_validate_events(args: argparse.Namespace) -> int:
    path = Path(args.path)
    if not path.exists():
        _print_error(f"file not found: {path}")
        return 1

    jsonl_report = validate_jsonl_file(path)
    if jsonl_report.errors:
        print("JSONL validation: FAIL")
        _print_issues(jsonl_report.errors, stream=sys.stderr)
        if not args.verbose:
            return 1
    else:
        print("JSONL validation: OK")

    preflight_report = replay_preflight(path)
    if preflight_report.errors:
        print("Replay preflight: FAIL")
        _print_issues(preflight_report.errors, stream=sys.stderr)
        return 1

    print(f"Replay preflight: OK ({preflight_report.event_count} events)")
    if preflight_report.warnings:
        _print_issues(preflight_report.warnings, stream=sys.stderr)
    return 0 if not jsonl_report.errors else 1


def _cmd_project_state(args: argparse.Namespace) -> int:
    try:
        projection = replay_project_state(args.path)
    except (FileNotFoundError, ReplayPreflightError) as exc:
        _print_error(str(exc))
        return 1

    for item_id in sorted(projection.items):
        item = projection.items[item_id]
        print(f"{item.item_id} {item.status} {item.last_event_id}")
    _print_issues(projection.warnings, stream=sys.stderr)
    return 0


def _cmd_task_queue(args: argparse.Namespace) -> int:
    try:
        projection = replay_task_queue_state(args.path)
    except (FileNotFoundError, ReplayPreflightError) as exc:
        _print_error(str(exc))
        return 1

    for handoff in projection.handoffs:
        print(
            f"{handoff.handoff_id} {handoff.item_id} "
            f"{handoff.scheduling_policy} {handoff.event_id}"
        )
    _print_issues(projection.warnings, stream=sys.stderr)
    return 0


def _cmd_digest(args: argparse.Namespace) -> int:
    try:
        projections = replay_all(args.path)
    except (FileNotFoundError, ReplayPreflightError) as exc:
        _print_error(str(exc))
        return 1

    digest = generate_batch_digest(projections)
    print("completed_items " + ",".join(digest.completed_items))
    print("blocked_or_waiting_approval " + ",".join(digest.blocked_or_waiting_approval))
    print("failed_items " + ",".join(digest.failed_items))
    print(f"handoff_count {digest.handoff_count}")
    print(f"resolved_dependency_count {digest.resolved_dependency_count}")
    return 0


def _print_issues(issues: list[ValidationIssue], stream) -> None:
    for issue in issues:
        line = "-" if issue.line_number is None else str(issue.line_number)
        print(f"line {line}: {issue.error_type}: {issue.message}", file=stream)


def _print_error(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)


if __name__ == "__main__":
    raise SystemExit(main())
