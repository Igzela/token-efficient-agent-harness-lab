"""Read-only derived views for MVP4 plan review.

The workbench reads app-owned planning state and derives summaries,
comparisons, and review actions. It does not mutate plans, target repos, or
approval state.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


PLAN_REVIEW_ACTIONS = (
    "review_blockers",
    "review_approval_gates",
    "review_token_budget",
    "review_steps",
    "review_remote_limit",
    "review_audit_failure",
    "ready_for_human_decision",
)


class PlanWorkbenchError(ValueError):
    """Raised when a derived plan-workbench request is invalid."""


@dataclass(frozen=True)
class PlanFilters:
    repo_id: str | None = None
    status: str | None = None
    risk_level: str | None = None
    task_type: str | None = None
    limit: int | None = None


def list_plan_summaries(plans: list[dict[str, Any]], filters: PlanFilters | None = None) -> list[dict[str, Any]]:
    """Return lightweight deterministic summaries for plan history tables."""

    plan_filters = filters or PlanFilters()
    summaries = [_plan_list_item(plan, index) for index, plan in enumerate(plans) if isinstance(plan, dict)]
    filtered = [item for item in summaries if _matches_filters(item, plan_filters)]
    if plan_filters.limit is not None:
        filtered = filtered[: plan_filters.limit]
    return filtered


def summarize_plans(plans: list[dict[str, Any]], repo_id: str | None = None) -> dict[str, Any]:
    """Return aggregate plan-review metrics."""

    summaries = list_plan_summaries(plans, PlanFilters(repo_id=repo_id))
    by_status = {"ready_for_review": 0, "needs_approval": 0, "blocked": 0}
    by_repo_kind = {"local": 0, "remote": 0}
    by_action: dict[str, int] = {}
    total_budget = 0
    plans_with_blockers = 0
    plans_with_approval_gates = 0

    for item in summaries:
        status = item["status"]
        by_status[status] = by_status.get(status, 0) + 1
        repo_kind = item["repo_kind"]
        by_repo_kind[repo_kind] = by_repo_kind.get(repo_kind, 0) + 1
        total_budget += item["total_token_budget"]
        if item["blocker_count"] > 0:
            plans_with_blockers += 1
        if item["approval_gate_count"] > 0:
            plans_with_approval_gates += 1
        action = item["next_review_action"]
        by_action[action] = by_action.get(action, 0) + 1

    total_plans = len(summaries)
    average_budget = total_budget // total_plans if total_plans else 0
    return {
        "total_plans": total_plans,
        "by_status": by_status,
        "by_repo_kind": by_repo_kind,
        "total_token_budget": total_budget,
        "average_token_budget": average_budget,
        "plans_with_blockers": plans_with_blockers,
        "plans_with_approval_gates": plans_with_approval_gates,
        "most_common_next_review_action": _most_common_action(by_action),
    }


def compare_plans(plans: list[dict[str, Any]], plan_ids: list[str]) -> dict[str, Any]:
    """Compare exactly two stored plans without mutating plan state."""

    if len(plan_ids) != 2:
        raise PlanWorkbenchError("exactly two plan_id query parameters are required")
    if plan_ids[0] == plan_ids[1]:
        raise PlanWorkbenchError("duplicate plan_id values cannot be compared")
    by_id = _plans_by_id(plans)
    missing = [plan_id for plan_id in plan_ids if plan_id not in by_id]
    if missing:
        raise KeyError(missing[0])

    first = by_id[plan_ids[0]]
    second = by_id[plan_ids[1]]
    first_steps = _steps(first)
    second_steps = _steps(second)
    token_delta = _int(second.get("total_token_budget")) - _int(first.get("total_token_budget"))
    context_delta = _int(second.get("context_budget")) - _int(first.get("context_budget"))
    execution_delta = _int(second.get("execution_budget")) - _int(first.get("execution_budget"))
    approval_delta = len(_approval_gates(second)) - len(_approval_gates(first))
    blocker_delta = len(_blockers(second)) - len(_blockers(first))

    return {
        "plan_ids": list(plan_ids),
        "same_repo": _repo_id(first) == _repo_id(second),
        "status_delta": _delta_label(_string(first.get("status")), _string(second.get("status"))),
        "next_review_action_delta": _delta_label(recommend_next_review_action(first), recommend_next_review_action(second)),
        "token_budget_delta": token_delta,
        "context_budget_delta": context_delta,
        "execution_budget_delta": execution_delta,
        "step_count_delta": len(second_steps) - len(first_steps),
        "approval_gate_delta": approval_delta,
        "blocker_delta": blocker_delta,
        "context_mode_changes": _context_mode_changes(first_steps, second_steps),
        "efficiency_note": _efficiency_note(token_delta, context_delta, execution_delta, approval_delta, blocker_delta),
    }


def recommend_next_review_action(plan: dict[str, Any]) -> str:
    """Derive the next non-executable review action for one plan."""

    status = _string(plan.get("status"))
    blockers = set(_blockers(plan))
    if status == "blocked" and "remote_metadata_only" in blockers:
        return "review_remote_limit"
    if status == "blocked" and ("audit_blocked" in blockers or _audit_verdict(plan) == "BLOCKED"):
        return "review_audit_failure"
    if status == "blocked":
        return "review_blockers"
    if status == "needs_approval":
        return "review_approval_gates"
    if status == "ready_for_review" and _has_budget_pressure(plan):
        return "review_token_budget"
    if status == "ready_for_review":
        return "review_steps"
    return "ready_for_human_decision"


def _plan_list_item(plan: dict[str, Any], stored_index: int) -> dict[str, Any]:
    task = plan.get("task") if isinstance(plan.get("task"), dict) else {}
    repo = plan.get("repo_snapshot") if isinstance(plan.get("repo_snapshot"), dict) else {}
    steps = _steps(plan)
    gates = _approval_gates(plan)
    blockers = _blockers(plan)
    return {
        "stored_index": stored_index,
        "plan_id": _string(plan.get("plan_id")),
        "repo_id": _repo_id(plan),
        "repo_kind": _string(repo.get("kind")),
        "task_id": _string(task.get("task_id")),
        "task_type": _string(task.get("task_type")),
        "status": _string(plan.get("status")),
        "effective_risk": _string(plan.get("effective_risk")),
        "executable": bool(plan.get("executable")),
        "total_token_budget": _int(plan.get("total_token_budget")),
        "context_budget": _int(plan.get("context_budget")),
        "execution_budget": _int(plan.get("execution_budget")),
        "step_count": len(steps),
        "approval_gate_count": len(gates),
        "blocker_count": len(blockers),
        "next_review_action": recommend_next_review_action(plan),
    }


def _matches_filters(item: dict[str, Any], filters: PlanFilters) -> bool:
    if filters.repo_id and item["repo_id"] != filters.repo_id:
        return False
    if filters.status and item["status"] != filters.status:
        return False
    if filters.risk_level and item["effective_risk"] != filters.risk_level:
        return False
    if filters.task_type and item["task_type"] != filters.task_type:
        return False
    return True


def _plans_by_id(plans: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    for plan in plans:
        if isinstance(plan, dict):
            plan_id = _string(plan.get("plan_id"))
            if plan_id and plan_id not in result:
                result[plan_id] = plan
    return result


def _context_mode_changes(first_steps: list[dict[str, Any]], second_steps: list[dict[str, Any]]) -> list[dict[str, Any]]:
    changes: list[dict[str, Any]] = []
    max_len = max(len(first_steps), len(second_steps))
    for index in range(max_len):
        first_mode = _step_context_mode(first_steps, index)
        second_mode = _step_context_mode(second_steps, index)
        if first_mode != second_mode:
            changes.append({"step_index": index, "a": first_mode, "b": second_mode})
    return changes


def _step_context_mode(steps: list[dict[str, Any]], index: int) -> str | None:
    if index >= len(steps):
        return None
    return _string(steps[index].get("context_mode"))


def _efficiency_note(
    token_delta: int,
    context_delta: int,
    execution_delta: int,
    approval_delta: int,
    blocker_delta: int,
) -> str:
    if token_delta < 0:
        direction = "Plan b uses a lower total token budget."
    elif token_delta > 0:
        direction = "Plan b uses a higher total token budget."
    else:
        direction = "Both plans use the same total token budget."
    return (
        f"{direction} Context delta {context_delta}; execution delta {execution_delta}; "
        f"approval gate delta {approval_delta}; blocker delta {blocker_delta}."
    )


def _delta_label(first: str, second: str) -> str:
    if first == second:
        return "same"
    return f"{first}->{second}"


def _most_common_action(by_action: dict[str, int]) -> str | None:
    if not by_action:
        return None
    return sorted(by_action.items(), key=lambda item: (-item[1], item[0]))[0][0]


def _repo_id(plan: dict[str, Any]) -> str:
    task = plan.get("task") if isinstance(plan.get("task"), dict) else {}
    repo = plan.get("repo_snapshot") if isinstance(plan.get("repo_snapshot"), dict) else {}
    return _string(task.get("repo_id")) or _string(repo.get("id"))


def _audit_verdict(plan: dict[str, Any]) -> str:
    audit = plan.get("audit_summary") if isinstance(plan.get("audit_summary"), dict) else {}
    return _string(audit.get("verdict"))


def _steps(plan: dict[str, Any]) -> list[dict[str, Any]]:
    steps = plan.get("steps")
    if not isinstance(steps, list):
        return []
    return [step for step in steps if isinstance(step, dict)]


def _approval_gates(plan: dict[str, Any]) -> list[str]:
    return _string_list(plan.get("approval_gates"))


def _blockers(plan: dict[str, Any]) -> list[str]:
    return _string_list(plan.get("blockers"))


def _has_budget_pressure(plan: dict[str, Any]) -> bool:
    if any("budget pressure" in note.lower() for note in _string_list(plan.get("token_efficiency_notes"))):
        return True
    total_budget = _int(plan.get("total_token_budget"))
    context_budget = _int(plan.get("context_budget"))
    return total_budget >= 6000 or context_budget >= 5000


def _string_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [item for item in value if isinstance(item, str)]


def _string(value: Any) -> str:
    return value if isinstance(value, str) else ""


def _int(value: Any) -> int:
    return value if isinstance(value, int) else 0
