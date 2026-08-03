#!/usr/bin/env python3
"""One-shot local entrypoint for the repository-owned agent loop."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
from typing import Callable, Sequence

import local_loop
import local_run_once
import local_supervisor
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
    run_once = subparsers.add_parser(
        "run-once", help="claim and execute one Issue through the local control plane"
    )
    run_once.add_argument("--repo", required=True, help="GitHub repository as owner/name")
    run_once.add_argument("--repo-path", required=True, type=Path, help="exact local Git worktree root")
    run_subject = run_once.add_mutually_exclusive_group(required=True)
    run_subject.add_argument("--issue", type=int)
    run_subject.add_argument("--plan-id")
    run_once.add_argument("--attempt-id", required=True)
    batch = subparsers.add_parser(
        "run-batch", help="poll and launch up to the repository K local workers"
    )
    batch.add_argument("--repo", required=True, help="GitHub repository as owner/name")
    batch.add_argument("--repo-path", required=True, type=Path, help="exact local Git worktree root")
    batch.add_argument("--max-active", type=_bounded_max_active, default=state_manager.MAX_ACTIVE)
    batch.add_argument("--task-timeout", type=int, default=3600)
    return parser


def main(
    argv: Sequence[str] | None = None,
    *,
    controller_factory: Callable[..., local_loop.LoopController] | None = None,
    run_once_factory: Callable[..., local_run_once.LocalRunOnce] | None = None,
    supervisor_factory: Callable[..., local_supervisor.LocalSupervisor] | None = None,
) -> int:
    args = build_parser().parse_args(argv)
    if args.command == "run-batch":
        try:
            poller = (controller_factory or local_loop.LoopController)(
                local_loop.GitHubAdapter(args.repo),
                local_loop.GitAdapter(),
                repository=args.repo,
                repo_path=args.repo_path,
                max_active=args.max_active,
            )
            factory = supervisor_factory or local_supervisor.LocalSupervisor
            result = factory(
                poller,
                repository=args.repo,
                repo_path=args.repo_path,
                max_active=args.max_active,
                task_timeout_seconds=args.task_timeout,
            ).run_batch()
        except (OSError, ValueError, local_loop.LoopUnavailable) as exc:
            result = {
                "kind": "repo-agent-supervisor.v1",
                "status": "unavailable",
                "reason": str(exc)[:300],
                "results": [],
            }
        print(json.dumps(result, ensure_ascii=False, sort_keys=True))
        return 0 if result.get("status") in {"completed", "ready"} else 2
    if args.command == "run-once":
        try:
            factory = run_once_factory or local_run_once.LocalRunOnce
            runner = factory(repository=args.repo, repo_path=args.repo_path)
            if args.plan_id is not None:
                result = runner.run_plan_once(args.plan_id, args.attempt_id)
            else:
                result = runner.run_once(args.issue, args.attempt_id)
            wire = result.to_wire() if hasattr(result, "to_wire") else result
        except (OSError, ValueError, local_loop.LoopUnavailable) as exc:
            wire = {
                "kind": "repo-agent-local-run-once.v1",
                "status": "unavailable",
                "issue_number": args.issue,
                "attempt_id": args.attempt_id,
                "details": {"reason": str(exc)[:300]},
            }
        print(json.dumps(wire, ensure_ascii=False, sort_keys=True))
        return 0 if wire.get("status") == "handed_off" else 2
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
    if len(sys.argv) > 1 and sys.argv[1] == "run-once":
        local_run_once.ensure_task_process_group()
    sys.exit(main())
