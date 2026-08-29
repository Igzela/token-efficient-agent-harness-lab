#!/usr/bin/env python3
"""One-shot local entrypoint for the repository-owned agent loop."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sys
from typing import Callable, Sequence

import local_loop
import local_run_once
import local_supervisor
import shadow_steward
import route_driver
import state_manager
import steward
import steward_github
import steward_workers
from steward_journal import StewardJournal


NORMAL_IDLE = {"control_stopped", "capacity_full", "no_eligible_task"}
FAIL_CLOSED = {"identity_rejected", "stale_checkout", "unavailable"}
ROUTE_TERMINALS = {
    "ROUTE_EXHAUSTED",
    "DECISION_REQUIRED",
    "T3_REQUIRED",
    "OUTCOME_UNKNOWN",
}


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


def _bounded_route_transitions(value: str) -> int:
    try:
        parsed = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError("must be an integer")
    if parsed < 1 or parsed > 256:
        raise argparse.ArgumentTypeError("must be between 1 and 256")
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
    run_subject.add_argument("--route-drive", metavar="PACKET_ID", help="drive the closeout/promotion PR lifecycle for one closed packet")
    run_once.add_argument("--attempt-id", required=True)
    mission_stage = subparsers.add_parser(
        "mission-stage",
        help="load and run one authenticated current Mission Stage",
    )
    mission_stage.add_argument("--repo", required=True, help="GitHub repository as owner/name")
    mission_stage.add_argument(
        "--repo-path", required=True, type=Path, help="exact local Git worktree root"
    )
    mission_stage.add_argument(
        "--approval-issue", required=True, type=int,
        help="Issue carrying the exact owner-approval marker",
    )
    mission_stage.add_argument(
        "--request", required=True,
        help="bounded request text; only its redacted proposal facts are retained",
    )
    mission_stage.add_argument(
        "--stage-pr", type=int,
        help="existing Stage PR number to re-read after Ready/CI/review",
    )
    mission_stage.add_argument(
        "--journal",
        default=os.environ.get("STEWARD_JOURNAL_PATH", "/var/lib/agent-steward/steward.sqlite3"),
    )
    mission_stage.add_argument(
        "--lock-dir",
        default=os.environ.get("STEWARD_LOCK_DIR", "/var/lib/agent-steward/locks"),
    )
    route_run = subparsers.add_parser(
        "route-run",
        help="continuously drive the accepted route without a caller-selected packet",
    )
    route_run.add_argument("--repo", required=True, help="GitHub repository as owner/name")
    route_run.add_argument("--repo-path", required=True, type=Path, help="exact local Git worktree root")
    route_run.add_argument("--max-transitions", type=_bounded_route_transitions, default=128)
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
    route_run_factory: Callable[..., route_driver.RepositoryRouteRunner] | None = None,
    steward_factory: Callable[..., steward.Steward] | None = None,
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
            elif args.route_drive:
                result = runner.run_route_once(args.route_drive, args.attempt_id)
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
    if args.command == "mission-stage":
        try:
            runner = (run_once_factory or local_run_once.LocalRunOnce)(
                repository=args.repo, repo_path=args.repo_path
            )
            proposal = shadow_steward.compile_proposal(args.request)
            worker = None
            reviewer = None
            if (
                args.approval_issue == steward_workers.PR4B_CANARY_APPROVAL_ISSUE
                and proposal.proposal_sha256 == steward_workers.PR4B_CANARY_PROPOSAL_SHA256
                and proposal.requested_paths == steward_workers.PR4B_CANARY_ALLOWED_PATHS
                and proposal.change_types == ("documentation",)
            ):
                worker = steward_workers.pr4b_canary_worker()
                reviewer = steward_workers.pr4b_canary_reviewer()
            stage_steward = (steward_factory or steward.Steward)(
                repository=args.repo,
                repo_path=args.repo_path,
                journal=StewardJournal(args.journal),
                github=steward_github.GhReadOnlyGitHub(),
                lock_dir=args.lock_dir,
                worker=worker,
                reviewer=reviewer,
            )
            result = runner.run_mission_stage(
                proposal,
                approval_issue=args.approval_issue,
                steward=stage_steward,
                stage_pr=args.stage_pr,
            )
            wire = {"kind": "repo-agent-mission-stage.v1", **result}
        except (
            OSError,
            ValueError,
            local_loop.LoopUnavailable,
            shadow_steward.ShadowStewardError,
            steward.StewardError,
            steward_github.GitHubFactsError,
        ) as exc:
            wire = {
                "kind": "repo-agent-mission-stage.v1",
                "status": "unavailable",
                "details": {"reason": str(exc)[:300]},
            }
        print(json.dumps(wire, ensure_ascii=False, sort_keys=True))
        return 0 if wire.get("status") in {
            "waiting_approval", "stage_pr_draft", "stage_pr_waiting", "waiting_for_merge", "complete"
        } else 2
    if args.command == "route-run":
        try:
            factory = route_run_factory or route_driver.RepositoryRouteRunner
            runner = factory(
                repository=args.repo,
                repo_path=args.repo_path,
                max_transitions=args.max_transitions,
            )
            result = runner.run()
            wire = result.to_wire() if hasattr(result, "to_wire") else result
        except (OSError, ValueError, local_loop.LoopUnavailable) as exc:
            wire = {
                "kind": "repo-agent-route-run.v1",
                "state": "UNRECOVERABLE_INFRASTRUCTURE_FAILURE",
                "reason": str(exc)[:300],
            }
        print(json.dumps(wire, ensure_ascii=False, sort_keys=True))
        return 0 if wire.get("state") in ROUTE_TERMINALS else 2
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
