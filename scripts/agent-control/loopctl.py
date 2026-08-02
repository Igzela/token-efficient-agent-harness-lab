#!/usr/bin/env python3
"""One-shot local entrypoint for the repository-owned agent loop."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
from typing import Callable, Sequence

import local_loop
import state_manager


NORMAL_IDLE = {"control_stopped", "capacity_full", "no_eligible_task"}
FAIL_CLOSED = {"identity_rejected", "stale_checkout", "unavailable"}


def _bounded_max_active(value: str) -> int:
    try:
        parsed = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError("must be an integer")
    if parsed < 1 or parsed > state_manager.MAX_ACTIVE:
        raise argparse.ArgumentTypeError(
            f"must be between 1 and {state_manager.MAX_ACTIVE}"
        )
    return parsed


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    poll = subparsers.add_parser("poll", help="perform one read-only GitHub control-plane poll")
    poll.add_argument("--repo", required=True, help="GitHub repository as owner/name")
    poll.add_argument("--repo-path", required=True, type=Path, help="exact local Git worktree root")
    poll.add_argument("--max-active", type=_bounded_max_active, default=state_manager.MAX_ACTIVE)
    poll.add_argument(
        "--require-ready",
        action="store_true",
        help="return exit 3 when the poll is healthy but no task is ready",
    )
    return parser


def main(
    argv: Sequence[str] | None = None,
    *,
    controller_factory: Callable[..., local_loop.LoopController] | None = None,
) -> int:
    args = build_parser().parse_args(argv)
    factory = controller_factory or local_loop.LoopController
    try:
        controller = factory(
            local_loop.GitHubAdapter(args.repo),
            local_loop.GitAdapter(),
            repository=args.repo,
            repo_path=args.repo_path,
            max_active=args.max_active,
        )
        decision = controller.poll()
    except (OSError, ValueError, local_loop.LoopUnavailable) as exc:
        decision = {
            "kind": local_loop.POLL_KIND,
            "status": "unavailable",
            "action": "none",
            "selected": None,
            "rejected": [],
            "reason": str(exc)[:300],
        }
    print(json.dumps(decision, ensure_ascii=False, sort_keys=True))
    status = decision.get("status")
    if status in FAIL_CLOSED:
        return 2
    if args.require_ready and status in NORMAL_IDLE:
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
