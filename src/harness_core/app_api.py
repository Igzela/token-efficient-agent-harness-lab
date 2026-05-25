"""Pure API handlers for the local Harness app control plane."""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path
from typing import Any

from harness_core.app_registry import (
    AppRegistry,
    AppRegistryError,
    RepoRef,
    registry_to_dict,
)
from harness_core.app_diagnostics import build_app_diagnostics, build_app_status, derive_recent_errors
from harness_core.instance_audit import audit_instance
from harness_core.plan_store import PlanStoreError, get_plan, load_plans, save_plan
from harness_core.plan_triage import (
    DEFAULT_TRIAGE_LIMIT,
    MAX_TRIAGE_LIMIT,
    PlanTriageError,
    build_portfolio_triage,
)
from harness_core.plan_workbench import (
    PlanFilters,
    PlanWorkbenchError,
    compare_plans,
    list_plan_summaries,
    summarize_plans,
)
from harness_core.resource_planner import (
    DeterministicResourcePlanner,
    PlanningInputError,
    PlanningTask,
)
from harness_core.review_guidance import build_review_guidance


MAX_PLAN_LIST_LIMIT = 100


class AppApiInputError(ValueError):
    """Raised when a request body has the wrong shape."""


@dataclass(frozen=True)
class AppApiResponse:
    """HTTP-shaped response returned by pure API handlers."""

    status_code: int
    body_json: dict[str, Any]
    headers: dict[str, str] | None = None

    def body_bytes(self) -> bytes:
        return json.dumps(self.body_json, ensure_ascii=False, sort_keys=True).encode("utf-8")

    def response_headers(self) -> dict[str, str]:
        base = {"Content-Type": "application/json; charset=utf-8"}
        if self.headers:
            base.update(self.headers)
        return base


def handle_api_request(
    method: str,
    path: str,
    body: bytes | str | None,
    registry_path: str | Path,
    plans_path: str | Path | None = None,
) -> AppApiResponse:
    """Handle one API request without starting a server."""

    route, query_string = _split_path(path)
    method = method.upper()
    plan_store_path = Path(plans_path) if plans_path is not None else default_plan_store_path()

    try:
        if route == "/api/health" and method == "GET":
            return _json(200, {"status": "ok", "mode": "local_read_only_control_plane"})
        if route == "/api/app/status" and method == "GET":
            return _app_status(registry_path, plan_store_path)
        if route == "/api/app/diagnostics" and method == "GET":
            return _app_diagnostics(registry_path, plan_store_path)
        if route == "/api/app/recent-errors" and method == "GET":
            return _app_recent_errors(registry_path, plan_store_path)
        if route == "/api/repos" and method == "GET":
            return _list_repos(registry_path)
        if route == "/api/repos" and method == "POST":
            return _add_repo(body, registry_path)
        if route == "/api/audit" and method == "GET":
            query = _parse_query(query_string)
            repo_id = _single_query_value(query, "repo_id")
            return _audit_repo(repo_id, registry_path)
        if route == "/api/plans" and method == "GET":
            query = _parse_query(query_string)
            return _list_plans(plan_store_path, query)
        if route == "/api/plans" and method == "POST":
            return _create_plan(body, registry_path, plan_store_path)
        if route == "/api/plans/summary" and method == "GET":
            query = _parse_query(query_string)
            return _plans_summary(plan_store_path, query)
        if route == "/api/plans/compare" and method == "GET":
            query = _parse_query(query_string)
            return _compare_plans(plan_store_path, query)
        if route == "/api/plans/triage" and method == "GET":
            query = _parse_query(query_string)
            return _plans_triage(plan_store_path, query)
        if route == "/api/plans/review-guidance" and method == "GET":
            query = _parse_query(query_string)
            return _review_guidance(plan_store_path, query)
        if route.startswith("/api/plans/") and method == "GET":
            plan_id = route.removeprefix("/api/plans/")
            return _get_plan(plan_id, plan_store_path)
        if route.startswith("/api/"):
            return _error(404, "not_found", "API route not found")
        return _error(404, "not_found", "Route not found")
    except AppRegistryError as exc:
        return _error(400, "invalid_registry_request", str(exc))
    except AppApiInputError as exc:
        return _error(400, "invalid_json", str(exc))
    except PlanningInputError as exc:
        return _error(400, _planning_error_code(str(exc)), str(exc))
    except PlanStoreError as exc:
        return _error(500, "plan_store_error", str(exc))
    except PlanTriageError as exc:
        return _error(400, "invalid_plan_triage_request", str(exc))
    except PlanWorkbenchError as exc:
        return _error(400, "invalid_plan_workbench_request", str(exc))
    except json.JSONDecodeError:
        return _error(400, "invalid_json", "Request body must be valid JSON")
    except OSError:
        return _error(500, "app_state_io_error", "App state file could not be read or written")


def _list_repos(registry_path: str | Path) -> AppApiResponse:
    registry = AppRegistry.load(registry_path)
    return _json(200, registry_to_dict(registry))


def _app_status(registry_path: str | Path, plans_path: str | Path) -> AppApiResponse:
    return _json(200, {"ok": True, "status": build_app_status(registry_path, plans_path)})


def _app_diagnostics(registry_path: str | Path, plans_path: str | Path) -> AppApiResponse:
    return _json(200, {"ok": True, "diagnostics": build_app_diagnostics(registry_path, plans_path)})


def _app_recent_errors(registry_path: str | Path, plans_path: str | Path) -> AppApiResponse:
    status = build_app_status(registry_path, plans_path)
    return _json(200, {"ok": True, "recent_errors": derive_recent_errors(status)})


def _add_repo(body: bytes | str | None, registry_path: str | Path) -> AppApiResponse:
    data = _decode_json_object(body)
    registry = AppRegistry.load(registry_path)
    updated = registry.add_repo(RepoRef.from_dict(data))
    updated.save(registry_path)
    repo = updated.get_repo(data["id"])
    return _json(201, {"repo": repo.to_dict() if repo else data})


def _audit_repo(repo_id: str | None, registry_path: str | Path) -> AppApiResponse:
    if not repo_id:
        return _error(400, "missing_repo_id", "repo_id query parameter is required")

    registry = AppRegistry.load(registry_path)
    repo = registry.get_repo(repo_id)
    if repo is None:
        return _error(404, "invalid_repo_id", "Repo id not found")

    if repo.kind == "remote":
        return _json(
            409,
            {
                "error": {
                    "code": "remote_audit_unsupported",
                    "message": "Remote repositories are metadata-only until registered as a local path.",
                },
                "repo": repo.to_dict(),
            },
        )

    if not repo.path:
        return _error(400, "invalid_repo", "Local repo is missing path")
    report = audit_instance(repo.path)
    return _json(200, {"repo": repo.to_dict(), "audit": report.to_dict()})


def _list_plans(plans_path: str | Path, query: dict[str, list[str]]) -> AppApiResponse:
    data = load_plans(plans_path)
    filters = PlanFilters(
        repo_id=_single_query_value(query, "repo_id"),
        status=_single_query_value(query, "status"),
        risk_level=_single_query_value(query, "risk_level"),
        task_type=_single_query_value(query, "task_type"),
        limit=_optional_positive_int(_single_query_value(query, "limit"), "limit", MAX_PLAN_LIST_LIMIT),
    )
    return _json(
        200,
        {
            "ok": True,
            "schema_version": data["schema_version"],
            "plans": list_plan_summaries(data["plans"], filters),
        },
    )


def _create_plan(body: bytes | str | None, registry_path: str | Path, plans_path: str | Path) -> AppApiResponse:
    data = _decode_json_object(body)
    if "path" in data:
        return _error(400, "path_not_allowed", "Plan requests must reference repo_id, not a filesystem path")

    task = PlanningTask.from_dict(data)
    registry = AppRegistry.load(registry_path)
    repo = registry.get_repo(task.repo_id)
    if repo is None:
        return _error(404, "invalid_repo_id", "Repo id not found")

    audit_report = None
    if repo.kind == "local":
        if not repo.path:
            return _error(400, "invalid_repo", "Local repo is missing path")
        if _path_is_inside(plans_path, repo.path):
            return _error(
                400,
                "plan_store_inside_target_repo",
                "Plan store path must be outside the selected target repository",
            )
        audit_report = audit_instance(repo.path)

    plan = DeterministicResourcePlanner().plan(task, repo, audit_report)
    save_plan(plans_path, plan)
    status_code = 200 if plan.status == "blocked" else 201
    return _json(status_code, {"plan": plan.to_dict()})


def _get_plan(plan_id: str, plans_path: str | Path) -> AppApiResponse:
    if not plan_id:
        return _error(404, "invalid_plan_id", "Plan id not found")
    plan = get_plan(plans_path, plan_id)
    if plan is None:
        return _error(404, "invalid_plan_id", "Plan id not found")
    return _json(200, {"plan": plan})


def _plans_summary(plans_path: str | Path, query: dict[str, list[str]]) -> AppApiResponse:
    data = load_plans(plans_path)
    summary = summarize_plans(data["plans"], repo_id=_single_query_value(query, "repo_id"))
    return _json(200, {"ok": True, "summary": summary})


def _compare_plans(plans_path: str | Path, query: dict[str, list[str]]) -> AppApiResponse:
    data = load_plans(plans_path)
    plan_ids = query.get("plan_id", [])
    try:
        comparison = compare_plans(data["plans"], plan_ids)
    except KeyError as exc:
        return _error(404, "invalid_plan_id", f"Plan id not found: {exc.args[0]}")
    return _json(200, {"ok": True, "comparison": comparison})


def _plans_triage(plans_path: str | Path, query: dict[str, list[str]]) -> AppApiResponse:
    data = load_plans(plans_path)
    limit = _optional_triage_limit(_single_query_value(query, "limit"))
    triage = build_portfolio_triage(
        data["plans"],
        repo_id=_single_query_value(query, "repo_id"),
        limit=limit if limit is not None else DEFAULT_TRIAGE_LIMIT,
    )
    return _json(200, {"ok": True, "triage": triage})


def _review_guidance(plans_path: str | Path, query: dict[str, list[str]]) -> AppApiResponse:
    plan_id = _single_query_value(query, "plan_id")
    if not plan_id:
        return _error(400, "invalid_review_guidance_request", "plan_id query parameter is required")
    plan = get_plan(plans_path, plan_id)
    if plan is None:
        return _error(404, "plan_not_found", "Plan id not found")
    return _json(200, {"ok": True, "guidance": build_review_guidance(plan)})


def _decode_json_object(body: bytes | str | None) -> dict[str, Any]:
    if body is None:
        raise json.JSONDecodeError("missing body", "", 0)
    if isinstance(body, bytes):
        body_text = body.decode("utf-8")
    else:
        body_text = body

    data = json.loads(body_text)
    if not isinstance(data, dict):
        raise AppApiInputError("Request body must be an object")
    return data


def default_plan_store_path() -> Path:
    state_home = os.environ.get("XDG_STATE_HOME")
    if state_home:
        return Path(state_home).expanduser() / "harness-app" / "plans.json"
    return Path.home() / ".local" / "state" / "harness-app" / "plans.json"


def _path_is_inside(candidate_path: str | Path, root_path: str | Path) -> bool:
    candidate = Path(candidate_path).expanduser().resolve()
    root = Path(root_path).expanduser().resolve()
    return candidate == root or root in candidate.parents


def _planning_error_code(message: str) -> str:
    if "token" in message or "budget" in message:
        return "invalid_budget"
    return "invalid_plan_request"


def _optional_positive_int(value: str | None, field_name: str, max_value: int) -> int | None:
    if value is None:
        return None
    if not value.isdigit() or int(value) < 1:
        raise PlanWorkbenchError(f"{field_name} must be a positive integer")
    parsed = int(value)
    if parsed > max_value:
        raise PlanWorkbenchError(f"{field_name} must be less than or equal to {max_value}")
    return parsed


def _optional_triage_limit(value: str | None) -> int | None:
    if value is None:
        return None
    if not value.isdigit() or int(value) < 1:
        raise PlanTriageError("limit must be a positive integer")
    parsed = int(value)
    if parsed > MAX_TRIAGE_LIMIT:
        raise PlanTriageError(f"limit must be less than or equal to {MAX_TRIAGE_LIMIT}")
    return parsed


def _single_query_value(query: dict[str, list[str]], key: str) -> str | None:
    values = query.get(key)
    if not values:
        return None
    return values[0]


def _split_path(path: str) -> tuple[str, str]:
    if "?" not in path:
        return path, ""
    route, query_string = path.split("?", 1)
    return route, query_string


def _parse_query(query_string: str) -> dict[str, list[str]]:
    result: dict[str, list[str]] = {}
    if not query_string:
        return result
    for part in query_string.split("&"):
        if not part or "=" not in part:
            continue
        key, value = part.split("=", 1)
        if not key or not value:
            continue
        result.setdefault(key, []).append(value)
    return result


def _json(status_code: int, body: dict[str, Any]) -> AppApiResponse:
    return AppApiResponse(status_code=status_code, body_json=body)


def _error(status_code: int, code: str, message: str) -> AppApiResponse:
    return _json(status_code, {"error": {"code": code, "message": message}})
