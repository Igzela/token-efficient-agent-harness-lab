"""Read-only portfolio triage derived from stored non-executable plans.

MVP6 triage ranks stored plans for human review. It does not approve, execute,
mutate, assign work, write target repositories, or persist triage output.
"""

from __future__ import annotations

from copy import deepcopy
from typing import Any

from harness_core.plan_workbench import recommend_next_review_action


PLAN_TRIAGE_SCHEMA_VERSION = "plan_triage.v1"
TRIAGE_BOUNDARY_NOTICE = (
    "Portfolio triage is advisory only. It does not approve, execute, mutate, assign, "
    "or write target repositories."
)
DEFAULT_TRIAGE_LIMIT = 50
MAX_TRIAGE_LIMIT = 100


class PlanTriageError(ValueError):
    """Raised when a plan triage request is invalid."""


def build_portfolio_triage(
    plans: list[dict[str, Any]],
    *,
    repo_id: str | None = None,
    limit: int = DEFAULT_TRIAGE_LIMIT,
) -> dict[str, Any]:
    """Return a deterministic read-only triage view over stored plans."""

    _validate_limit(limit)
    snapshots = [deepcopy(plan) for plan in plans if isinstance(plan, dict)]
    filtered = [plan for plan in snapshots if repo_id is None or _repo_id(plan) == repo_id]
    all_items = [triage_plan(plan, stored_index=index) for index, plan in enumerate(filtered)]
    all_items.sort(key=lambda item: (-item["review_priority"], -item["stored_index"], item["plan_id"]))
    items = all_items[:limit]

    return {
        "schema_version": PLAN_TRIAGE_SCHEMA_VERSION,
        "repo_id": repo_id,
        "total_plans": len(filtered),
        "returned_items": len(items),
        "generated_from_store_only": True,
        "persistent": False,
        "non_executable": True,
        "summary": _summary(all_items),
        "items": items,
        "boundary_notice": TRIAGE_BOUNDARY_NOTICE,
    }


def triage_plan(plan: dict[str, Any], *, stored_index: int = 0) -> dict[str, Any]:
    """Return one portfolio triage item for a stored plan."""

    bucket, bottleneck, priority, focus = _classification(plan)
    token_hotspots = derive_token_hotspots(plan)
    gates = _approval_gates(plan)
    return {
        "stored_index": stored_index,
        "plan_id": _string(plan.get("plan_id")),
        "repo_id": _repo_id(plan),
        "status": _string(plan.get("status")),
        "effective_risk": _string(plan.get("effective_risk")),
        "task_type": _task_type(plan),
        "executable": False,
        "review_priority": priority,
        "review_bucket": bucket,
        "bottleneck": bottleneck,
        "next_review_action": recommend_next_review_action(plan),
        "recommended_human_focus": focus,
        "token_budget": {
            "total": _int(plan.get("total_token_budget")),
            "context": _int(plan.get("context_budget")),
            "execution": _int(plan.get("execution_budget")),
        },
        "token_hotspots": token_hotspots,
        "blockers": _blockers(plan),
        "approval_gate_count": len(gates),
        "step_count": len(_steps(plan)),
    }


def classify_plan_bottleneck(plan: dict[str, Any]) -> str:
    """Return the main deterministic bottleneck label for one plan."""

    return _classification(plan)[1]


def derive_token_hotspots(plan: dict[str, Any]) -> list[str]:
    """Return deterministic token-efficiency hotspot labels for one plan."""

    hotspots: list[str] = []
    total_budget = _int(plan.get("total_token_budget"))
    context_budget = _int(plan.get("context_budget"))
    steps = _steps(plan)
    gates = _approval_gates(plan)

    if total_budget > 0 and context_budget / total_budget >= 0.75:
        hotspots.append("high_context_budget")
    if _string_list(plan.get("token_efficiency_notes")):
        hotspots.append("budget_pressure_notes_present")
    if any(_string(step.get("context_mode")) == "full" for step in steps) and _has_budget_pressure(plan):
        hotspots.append("full_context_under_pressure")
    if len(steps) >= 8:
        hotspots.append("high_step_count")
    if len(gates) >= 3:
        hotspots.append("gate_heavy_plan")
    return hotspots


def compute_review_priority(plan: dict[str, Any]) -> int:
    """Return a review priority score, not an execution priority."""

    return _classification(plan)[2]


def validate_triage_limit(limit: int) -> int:
    """Validate and return a portfolio triage limit."""

    _validate_limit(limit)
    return limit


def _classification(plan: dict[str, Any]) -> tuple[str, str, int, str]:
    status = _string(plan.get("status"))
    blockers = set(_blockers(plan))
    steps = _steps(plan)

    if status == "blocked" and "remote_metadata_only" in blockers:
        return (
            "remote_limited",
            "remote_metadata_only",
            70,
            "Register local repo or keep metadata-only before deeper review.",
        )
    if status == "blocked" and ("audit_blocked" in blockers or _audit_verdict(plan) == "BLOCKED"):
        return ("audit_blocked", "audit_failure", 90, "Inspect audit result before spending more context.")
    if status == "blocked":
        return ("blocked", "blockers", 85, "Inspect blockers before continuing review.")
    if status == "needs_approval":
        return ("review_gates", "approval_gates", 80, "Inspect review gates before spending more context.")
    if status == "ready_for_review" and _has_budget_pressure(plan):
        return ("token_budget_review", "token_hotspot", 60, "Review token hotspots before requesting more context.")
    if status == "ready_for_review" and len(steps) >= 8:
        return ("split_or_simplify", "plan_complexity", 50, "Review whether the plan should be split into smaller slices.")
    if status == "ready_for_review":
        return ("normal_review", "none", 40, "Continue human review of planned steps.")
    return ("unknown_review", "unknown_status", 10, "Inspect plan status before continuing review.")


def _summary(items: list[dict[str, Any]]) -> dict[str, int]:
    summary = {
        "blocked": 0,
        "needs_approval": 0,
        "ready_for_review": 0,
        "token_hotspot_count": 0,
        "remote_limited_count": 0,
        "audit_blocked_count": 0,
        "budget_pressure_count": 0,
    }
    for item in items:
        status = item["status"]
        if status in summary:
            summary[status] += 1
        if item["token_hotspots"]:
            summary["token_hotspot_count"] += 1
        if item["review_bucket"] == "remote_limited":
            summary["remote_limited_count"] += 1
        if item["review_bucket"] == "audit_blocked":
            summary["audit_blocked_count"] += 1
        if "budget_pressure_notes_present" in item["token_hotspots"]:
            summary["budget_pressure_count"] += 1
    return summary


def _validate_limit(limit: int) -> None:
    if not isinstance(limit, int) or limit < 1:
        raise PlanTriageError("limit must be a positive integer")
    if limit > MAX_TRIAGE_LIMIT:
        raise PlanTriageError(f"limit must be less than or equal to {MAX_TRIAGE_LIMIT}")


def _repo_id(plan: dict[str, Any]) -> str:
    task = plan.get("task") if isinstance(plan.get("task"), dict) else {}
    repo = plan.get("repo_snapshot") if isinstance(plan.get("repo_snapshot"), dict) else {}
    return _string(task.get("repo_id")) or _string(repo.get("id"))


def _task_type(plan: dict[str, Any]) -> str:
    task = plan.get("task") if isinstance(plan.get("task"), dict) else {}
    return _string(task.get("task_type"))


def _audit_verdict(plan: dict[str, Any]) -> str:
    audit = plan.get("audit_summary") if isinstance(plan.get("audit_summary"), dict) else {}
    return _string(audit.get("verdict"))


def _has_budget_pressure(plan: dict[str, Any]) -> bool:
    return any("budget pressure" in note.lower() for note in _string_list(plan.get("token_efficiency_notes")))


def _steps(plan: dict[str, Any]) -> list[dict[str, Any]]:
    steps = plan.get("steps")
    if not isinstance(steps, list):
        return []
    return [step for step in steps if isinstance(step, dict)]


def _approval_gates(plan: dict[str, Any]) -> list[str]:
    return _string_list(plan.get("approval_gates"))


def _blockers(plan: dict[str, Any]) -> list[str]:
    return _string_list(plan.get("blockers"))


def _string_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [item for item in value if isinstance(item, str)]


def _string(value: Any) -> str:
    return value if isinstance(value, str) else ""


def _int(value: Any) -> int:
    return value if isinstance(value, int) else 0
