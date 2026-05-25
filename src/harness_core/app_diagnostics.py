"""Read-only diagnostics for the local Harness app.

MVP7 diagnostics describe app-owned state and component health. They do not
call providers, execute plans, launch workers or sandboxes, mutate plans, or
write target repositories.
"""

from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from harness_core.app_registry import AppRegistry, AppRegistryError
from harness_core.plan_store import PlanStoreError, load_plans


APP_DIAGNOSTICS_SCHEMA_VERSION = "app_diagnostics.v1"
APP_STATUS_SCHEMA_VERSION = "app_status.v1"
APP_RECENT_ERRORS_SCHEMA_VERSION = "app_recent_errors.v1"
DIAGNOSTICS_BOUNDARY_NOTICE = (
    "Operations diagnostics are read-only. They do not approve, execute, mutate, assign, "
    "call providers, launch workers or sandboxes, or write target repositories."
)


def build_app_status(registry_path: str | Path, plans_path: str | Path) -> dict[str, Any]:
    """Return component status for app-owned local state."""

    now = _now()
    registry_state = _inspect_registry(registry_path)
    plan_state = _inspect_plan_store(plans_path)
    components = _components(now, registry_state, plan_state)
    data_flow = _data_flow(registry_state, plan_state)
    storage = _storage_health(registry_state, plan_state)
    overall = _overall_status(components)

    return {
        "schema_version": APP_STATUS_SCHEMA_VERSION,
        "status": overall,
        "mode": "local_read_only_control_plane",
        "last_checked": now,
        "component_count": len(components),
        "components": components,
        "data_flow": data_flow,
        "storage": storage,
        "boundary_notice": DIAGNOSTICS_BOUNDARY_NOTICE,
    }


def build_app_diagnostics(registry_path: str | Path, plans_path: str | Path) -> dict[str, Any]:
    """Return a full read-only diagnostics report for the local app."""

    status = build_app_status(registry_path, plans_path)
    recent_errors = derive_recent_errors(status)
    return {
        "schema_version": APP_DIAGNOSTICS_SCHEMA_VERSION,
        "status": status["status"],
        "last_checked": status["last_checked"],
        "system_overview": {
            "mode": status["mode"],
            "component_count": status["component_count"],
            "blocked_components": _count_components(status["components"], "blocked"),
            "warning_components": _count_components(status["components"], "warning"),
            "unavailable_components": _count_components(status["components"], "unavailable"),
        },
        "components": status["components"],
        "data_flow": status["data_flow"],
        "storage": status["storage"],
        "recent_errors": recent_errors["errors"],
        "recommended_debug_actions": _recommended_actions(status, recent_errors["errors"]),
        "boundary_notice": DIAGNOSTICS_BOUNDARY_NOTICE,
    }


def derive_recent_errors(status: dict[str, Any]) -> dict[str, Any]:
    """Return recent derived app errors without persisting an event log."""

    errors: list[dict[str, Any]] = []
    checked_at = _string(status.get("last_checked"))
    for component in status.get("components", []):
        if not isinstance(component, dict):
            continue
        if component.get("status") in {"blocked", "unavailable"}:
            errors.append(
                {
                    "component": _string(component.get("component")),
                    "status": _string(component.get("status")),
                    "message": _string(component.get("message")),
                    "last_seen": checked_at,
                    "source": "derived_component_status",
                }
            )
    return {
        "schema_version": APP_RECENT_ERRORS_SCHEMA_VERSION,
        "persistent": False,
        "errors": errors,
        "boundary_notice": DIAGNOSTICS_BOUNDARY_NOTICE,
    }


def _components(now: str, registry_state: dict[str, Any], plan_state: dict[str, Any]) -> list[dict[str, Any]]:
    repo_count = int(registry_state.get("repo_count", 0))
    local_repo_count = int(registry_state.get("local_repo_count", 0))
    plan_count = int(plan_state.get("plan_count", 0))
    registry_ok = registry_state["status"] == "ok"
    plans_ok = plan_state["status"] == "ok"
    plans_blocked = plan_state["status"] == "blocked"
    return [
        _component(
            "app_server",
            "ok",
            "Local API handler is responding in read-only control-plane mode.",
            now,
            ["handle_api_request", "mode=local_read_only_control_plane"],
            "Use diagnostics panels before changing app state.",
        ),
        _component(
            "app_registry",
            registry_state["status"],
            registry_state["message"],
            now,
            registry_state["evidence"],
            registry_state["recommended_action"],
        ),
        _component(
            "instance_audit",
            "ok" if registry_ok and local_repo_count > 0 else "warning",
            "Read-only audit is available for registered local repositories."
            if registry_ok and local_repo_count > 0
            else "Register a readable local repository before expecting audit output.",
            now,
            [f"local_repo_count={local_repo_count}", f"repo_count={repo_count}"],
            "Register a local repo, then run a read-only audit.",
        ),
        _component(
            "plan_store",
            plan_state["status"],
            plan_state["message"],
            now,
            plan_state["evidence"],
            plan_state["recommended_action"],
        ),
        _component(
            "resource_planner",
            "ok" if registry_ok else "warning",
            "Deterministic planner can create non-executable plans from registered repositories."
            if registry_ok
            else "Planner needs a readable app registry before planning.",
            now,
            [f"repo_count={repo_count}", "executable=false"],
            "Create plans only from registered repo ids.",
        ),
        _component(
            "plan_workbench",
            "ok" if plans_ok else ("blocked" if plans_blocked else "warning"),
            "Plan history, summaries, and comparisons can be derived from stored plans."
            if plans_ok
            else "Plan workbench needs a readable plan store with app-owned plans.",
            now,
            [f"plan_count={plan_count}", f"plan_store_status={plan_state['status']}"],
            "Inspect plan store JSON if blocked.",
        ),
        _component(
            "review_guidance",
            "ok" if plans_ok and plan_count > 0 else "warning",
            "Review guidance is available for stored plans."
            if plans_ok and plan_count > 0
            else "Create or load a stored plan before generating review guidance.",
            now,
            [f"plan_count={plan_count}", "persistent=false"],
            "Select a stored plan before generating guidance.",
        ),
        _component(
            "plan_triage",
            "ok" if plans_ok else ("blocked" if plans_blocked else "warning"),
            "Portfolio triage can be derived from stored non-executable plans."
            if plans_ok
            else "Portfolio triage needs a readable plan store with app-owned plans.",
            now,
            [f"plan_count={plan_count}", "non_executable=true"],
            "Inspect plan store JSON if blocked.",
        ),
        _component(
            "dashboard_frontend",
            "ok",
            "Dashboard static assets expose planning, review, triage, and diagnostics panels.",
            now,
            ["web/dashboard/index.html", "web/dashboard/app.js"],
            "Use browser smoke checks after frontend changes.",
        ),
        _component(
            "security_boundary",
            "ok",
            "Provider calls, worker launch, sandbox execution, and target repo writes remain out of scope.",
            now,
            ["provider=no_calls", "sandbox=no_execution", "target_repo=read_only"],
            "Keep new diagnostics read-only and advisory.",
        ),
    ]


def _inspect_registry(path: str | Path) -> dict[str, Any]:
    registry_path = Path(path)
    evidence = [f"path={registry_path}", f"exists={registry_path.exists()}"]
    try:
        registry = AppRegistry.load(registry_path)
    except AppRegistryError as exc:
        return {
            "status": "blocked",
            "message": str(exc),
            "repo_count": 0,
            "local_repo_count": 0,
            "remote_repo_count": 0,
            "evidence": evidence,
            "recommended_action": "Inspect or recreate the app registry JSON file.",
        }
    repos = registry.list_repos()
    local_count = len([repo for repo in repos if repo.kind == "local"])
    remote_count = len([repo for repo in repos if repo.kind == "remote"])
    status = "ok" if registry_path.exists() else "warning"
    message = "Registry is readable." if registry_path.exists() else "Registry file is absent; app will start with zero repos."
    return {
        "status": status,
        "message": message,
        "repo_count": len(repos),
        "local_repo_count": local_count,
        "remote_repo_count": remote_count,
        "evidence": [*evidence, f"repo_count={len(repos)}", f"local_repo_count={local_count}"],
        "recommended_action": "Register a local repo if audit or planning panels need data.",
    }


def _inspect_plan_store(path: str | Path) -> dict[str, Any]:
    plans_path = Path(path)
    evidence = [f"path={plans_path}", f"exists={plans_path.exists()}"]
    try:
        data = load_plans(plans_path)
    except PlanStoreError as exc:
        return {
            "status": "blocked",
            "message": str(exc),
            "plan_count": 0,
            "evidence": evidence,
            "recommended_action": "Inspect or recreate the app plan store JSON file.",
        }
    plans = [plan for plan in data["plans"] if isinstance(plan, dict)]
    status = "ok" if plans_path.exists() else "warning"
    message = "Plan store is readable." if plans_path.exists() else "Plan store file is absent; app will start with zero plans."
    return {
        "status": status,
        "message": message,
        "plan_count": len(plans),
        "evidence": [*evidence, f"plan_count={len(plans)}", f"schema_version={data['schema_version']}"],
        "recommended_action": "Generate a non-executable plan if review panels need data.",
    }


def _data_flow(registry_state: dict[str, Any], plan_state: dict[str, Any]) -> list[dict[str, str]]:
    registry_ok = registry_state["status"] == "ok"
    plans_ok = plan_state["status"] == "ok"
    return [
        _flow_step("repo_registry", registry_state["status"], registry_state["message"]),
        _flow_step(
            "repo_to_audit",
            "ok" if registry_ok and registry_state.get("local_repo_count", 0) else "warning",
            "Audit needs at least one registered local repository.",
        ),
        _flow_step(
            "audit_to_plan",
            "ok" if registry_ok else "blocked",
            "Planning reads registry entries and audit summaries without executing work.",
        ),
        _flow_step("plan_store", plan_state["status"], plan_state["message"]),
        _flow_step(
            "plan_to_review",
            "ok" if plans_ok and plan_state.get("plan_count", 0) else "warning",
            "Review guidance and triage need stored non-executable plans.",
        ),
    ]


def _storage_health(registry_state: dict[str, Any], plan_state: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        {
            "name": "registry",
            "status": registry_state["status"],
            "evidence": registry_state["evidence"],
            "record_count": registry_state["repo_count"],
            "recommended_action": registry_state["recommended_action"],
        },
        {
            "name": "plan_store",
            "status": plan_state["status"],
            "evidence": plan_state["evidence"],
            "record_count": plan_state["plan_count"],
            "recommended_action": plan_state["recommended_action"],
        },
    ]


def _recommended_actions(status: dict[str, Any], errors: list[dict[str, Any]]) -> list[str]:
    if errors:
        return [f"Inspect {error['component']}: {error['message']}" for error in errors]
    components = status["components"]
    if any(component["status"] == "warning" for component in components):
        return ["Register a local repo or create a stored plan to populate all debug panels."]
    return ["All app diagnostics are readable; continue review through non-executable panels."]


def _component(
    name: str,
    status: str,
    message: str,
    last_checked: str,
    evidence: list[str],
    recommended_action: str,
) -> dict[str, Any]:
    return {
        "component": name,
        "status": status,
        "message": message,
        "last_checked": last_checked,
        "evidence": evidence,
        "recommended_action": recommended_action,
    }


def _flow_step(name: str, status: str, message: str) -> dict[str, str]:
    return {"step": name, "status": status, "message": message}


def _overall_status(components: list[dict[str, Any]]) -> str:
    statuses = {component["status"] for component in components}
    if "blocked" in statuses:
        return "blocked"
    if "unavailable" in statuses:
        return "unavailable"
    if "warning" in statuses:
        return "warning"
    return "ok"


def _count_components(components: list[dict[str, Any]], status: str) -> int:
    return len([component for component in components if component.get("status") == status])


def _now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def _string(value: Any) -> str:
    return value if isinstance(value, str) else ""
