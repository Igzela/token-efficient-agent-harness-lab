"""Command-line wrapper for the Stage 1 harness core library."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Sequence

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

    validate_alias = subparsers.add_parser("validate", help="validate an events.jsonl file (alias)")
    validate_alias.add_argument("path")
    validate_alias.add_argument(
        "--verbose",
        action="store_true",
        help="run replay preflight even when JSONL validation already failed",
    )
    validate_alias.set_defaults(func=_cmd_validate_events)

    project_state = subparsers.add_parser("project-state", help="print project item statuses")
    project_state.add_argument("path")
    project_state.set_defaults(func=_cmd_project_state)

    task_queue = subparsers.add_parser("task-queue", help="print task queue handoff projection")
    task_queue.add_argument("path")
    task_queue.set_defaults(func=_cmd_task_queue)

    digest = subparsers.add_parser("digest", help="print digest stub summary")
    digest.add_argument("path")
    digest.set_defaults(func=_cmd_digest)

    dispatch = subparsers.add_parser("dispatch", help="create a dispatch decision from a request JSON file")
    dispatch.add_argument("request_file", help="path to JSON file with request text")
    dispatch.add_argument("--store", default=None, help="path to DurableStore database")
    dispatch.set_defaults(func=_cmd_dispatch)

    plans = subparsers.add_parser("plans", help="manage stored plans")
    plans.add_argument("--store", default=None, help="path to DurableStore database")
    plans_sub = plans.add_subparsers(dest="plans_command", required=True)
    plans_list = plans_sub.add_parser("list", help="list stored plans")
    plans_list.set_defaults(func=_cmd_plans_list)
    plans_show = plans_sub.add_parser("show", help="show plan details")
    plans_show.add_argument("plan_id")
    plans_show.set_defaults(func=_cmd_plans_show)

    repos = subparsers.add_parser("repos", help="manage registered repos")
    repos.add_argument("--store", default=None, help="path to DurableStore database")
    repos_sub = repos.add_subparsers(dest="repos_command", required=True)
    repos_list = repos_sub.add_parser("list", help="list registered repos")
    repos_list.set_defaults(func=_cmd_repos_list)
    repos_add = repos_sub.add_parser("add", help="register a repo")
    repos_add.add_argument("repo_path")
    repos_add.set_defaults(func=_cmd_repos_add)

    health = subparsers.add_parser("health", help="run health checks and print status")
    health.add_argument("--store", default=None, help="path to DurableStore database")
    health.set_defaults(func=_cmd_health)

    status = subparsers.add_parser("status", help="show overall system status")
    status.add_argument("--store", default=None, help="path to DurableStore database")
    status.set_defaults(func=_cmd_status)

    return parser


# ---------------------------------------------------------------------------
# Existing commands
# ---------------------------------------------------------------------------


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


# ---------------------------------------------------------------------------
# New commands
# ---------------------------------------------------------------------------


def _get_store(args: argparse.Namespace) -> Any:
    from .dispatch.durable_store import DurableStore
    store_path = getattr(args, "store", None)
    return DurableStore(db_path=store_path or ":memory:")


def _cmd_dispatch(args: argparse.Namespace) -> int:
    from .dispatch.dispatch_engine import DispatchEngine

    path = Path(args.request_file)
    if not path.exists():
        _print_error(f"file not found: {path}")
        return 1

    try:
        request_data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        _print_error(f"failed to read request file: {exc}")
        return 1

    if not isinstance(request_data, dict):
        _print_error("request file must contain a JSON object")
        return 1

    raw_request = request_data.get("text", "")
    if not raw_request:
        _print_error("request file must contain a 'text' field")
        return 1

    request_source = request_data.get("source", "cli")
    engine = DispatchEngine()
    bundle = engine.dispatch(raw_request, request_source=request_source)

    store_path = getattr(args, "store", None)
    if store_path:
        from .dispatch.durable_store import DurableStore
        store = DurableStore(db_path=store_path)
        try:
            store.save_plan(
                bundle.record.dispatch_id,
                bundle.decision.to_dict(),
            )
        finally:
            store.close()
    else:
        print("Note: dispatch result not persisted (use --store to save)", file=sys.stderr)

    decision_dict = bundle.decision.to_dict()
    output = {
        "dispatch_id": bundle.record.dispatch_id,
        "decision": decision_dict,
    }
    json.dump(output, sys.stdout, indent=2, default=str)
    print()
    return 0


def _cmd_plans_list(args: argparse.Namespace) -> int:
    store = _get_store(args)
    try:
        plans = store.list_plans()
        if not plans:
            print("No plans stored.")
            return 0
        for plan in plans:
            print(f"{plan.record_id}  created={plan.created_at}")
        return 0
    except Exception as exc:
        _print_error(str(exc))
        return 1
    finally:
        store.close()


def _cmd_plans_show(args: argparse.Namespace) -> int:
    store = _get_store(args)
    try:
        plan = store.get_plan(args.plan_id)
        if plan is None:
            _print_error(f"plan not found: {args.plan_id}")
            return 1
        json.dump(plan.data, sys.stdout, indent=2, default=str)
        print()
        return 0
    except Exception as exc:
        _print_error(str(exc))
        return 1
    finally:
        store.close()


def _cmd_repos_list(args: argparse.Namespace) -> int:
    store = _get_store(args)
    try:
        repos = store.list_repos()
        if not repos:
            print("No repos registered.")
            return 0
        for repo in repos:
            print(f"{repo.record_id}  created={repo.created_at}")
        return 0
    except Exception as exc:
        _print_error(str(exc))
        return 1
    finally:
        store.close()


def _cmd_repos_add(args: argparse.Namespace) -> int:
    repo_path = Path(args.repo_path)
    if not repo_path.exists():
        _print_error(f"path not found: {repo_path}")
        return 1

    store = _get_store(args)
    try:
        repo_id = repo_path.name
        data = {"path": str(repo_path.resolve()), "name": repo_id}
        store.save_repo(repo_id, data)
        print(f"Registered repo: {repo_id} ({data['path']})")
        return 0
    except Exception as exc:
        _print_error(str(exc))
        return 1
    finally:
        store.close()


def _cmd_health(args: argparse.Namespace) -> int:
    from .dispatch.health_checker import HealthChecker

    store = _get_store(args)
    try:
        checker = HealthChecker(store=store)
        report = checker.health()

        print(f"Overall status: {report.status}")
        for check in report.checks:
            latency_str = f" ({check.latency_ms:.1f}ms)" if check.latency_ms > 0 else ""
            msg_str = f" - {check.message}" if check.message else ""
            print(f"  {check.name}: {check.status}{msg_str}{latency_str}")

        return 0 if report.status == "healthy" else 1
    except Exception as exc:
        _print_error(str(exc))
        return 1
    finally:
        store.close()


def _cmd_status(args: argparse.Namespace) -> int:
    from .dispatch.health_checker import HealthChecker

    store = _get_store(args)
    try:
        stats = store.stats()
        checker = HealthChecker(store=store)
        health_report = checker.health()

        print(f"System status: {health_report.status}")
        print(f"Plans:         {stats['plans']}")
        print(f"Repos:         {stats['repos']}")
        print(f"Events:        {stats['events']}")
        print(f"Migrations:    {stats['migrations']}")

        return 0 if health_report.status == "healthy" else 1
    except Exception as exc:
        _print_error(str(exc))
        return 1
    finally:
        store.close()


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _print_issues(issues: list[ValidationIssue], stream) -> None:
    for issue in issues:
        line = "-" if issue.line_number is None else str(issue.line_number)
        print(f"line {line}: {issue.error_type}: {issue.message}", file=stream)


def _print_error(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)


if __name__ == "__main__":
    raise SystemExit(main())
