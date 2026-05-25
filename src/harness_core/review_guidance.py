"""Non-persistent review guidance derived from stored plans.

MVP5 guidance is advisory only. It does not approve, execute, mutate plans,
write target repositories, or persist review outcome records.
"""

from __future__ import annotations

from copy import deepcopy
from typing import Any

from harness_core.plan_workbench import recommend_next_review_action


GUIDANCE_SCHEMA_VERSION = "review_guidance.v1"
BOUNDARY_NOTICE = "Review guidance is advisory only. It does not approve, execute, or mutate plans."
REVIEW_OPTION_NAMES = (
    "continue_review",
    "request_more_context",
    "reduce_budget",
    "register_local_repo",
    "revise_objective",
    "split_plan",
    "inspect_blockers",
    "inspect_audit_result",
    "inspect_gates",
    "compare_with_lower_budget_plan",
    "keep_remote_metadata_only",
)


def build_review_guidance(plan: dict[str, Any]) -> dict[str, Any]:
    """Return a deterministic non-executable review guidance preview."""

    snapshot = deepcopy(plan)
    next_action = recommend_next_review_action(snapshot)
    options = derive_review_options(snapshot)
    evidence = derive_evidence_requirements(snapshot)
    token_guidance = derive_token_efficiency_guidance(snapshot)
    recommended = options[0]["option"] if options else "continue_review"

    return {
        "schema_version": GUIDANCE_SCHEMA_VERSION,
        "plan_id": _string(snapshot.get("plan_id")),
        "status": _string(snapshot.get("status")),
        "executable": False,
        "preview_only": True,
        "next_review_action": next_action,
        "recommended_option": recommended,
        "options": options,
        "evidence_requirements": evidence,
        "token_efficiency_guidance": token_guidance,
        "boundary_notice": BOUNDARY_NOTICE,
    }


def derive_review_options(plan: dict[str, Any]) -> list[dict[str, str]]:
    """Derive human review options without creating or recording outcomes."""

    status = _string(plan.get("status"))
    blockers = set(_string_list(plan.get("blockers")))
    steps = _list_of_dicts(plan.get("steps"))
    notes = _string_list(plan.get("token_efficiency_notes"))

    if status == "blocked" and "remote_metadata_only" in blockers:
        return [
            _option("register_local_repo", "Remote repositories are metadata-only; local registration enables read-only audit.", "human_review_only"),
            _option("keep_remote_metadata_only", "Keep this plan as metadata-only if local registration is not appropriate.", "human_review_only"),
        ]
    if status == "blocked" and ("audit_blocked" in blockers or _audit_verdict(plan) == "BLOCKED"):
        return [_option("inspect_audit_result", "The read-only audit blocked planning and needs human inspection.", "human_review_only")]
    if status == "blocked":
        return [_option("inspect_blockers", "Plan blockers must be understood before further review.", "human_review_only")]
    if status == "needs_approval":
        return [
            _option("inspect_gates", "Review gates explain why this plan is held for human review.", "human_review_only"),
            _option("continue_review", "Continue reviewing the non-executable plan details and evidence.", "human_review_only"),
        ]
    if status == "ready_for_review" and _has_budget_pressure(notes):
        return [
            _option("reduce_budget", "Budget pressure was detected; inspect whether lower context is acceptable.", "human_review_only"),
            _option("compare_with_lower_budget_plan", "Compare this plan with a lower-budget variant before continuing.", "human_review_only"),
        ]
    if status == "ready_for_review" and len(steps) > 4:
        return [_option("split_plan", "The plan has many planned steps; consider reviewing smaller slices.", "human_review_only")]
    if status == "ready_for_review":
        return [_option("continue_review", "The plan is ready for human review, not execution.", "human_review_only")]
    return [_option("revise_objective", "The plan status is not recognized by the guidance preview.", "human_review_only")]


def derive_evidence_requirements(plan: dict[str, Any]) -> list[dict[str, Any]]:
    """Derive evidence that a human should inspect next."""

    requirements = [
        {
            "kind": "plan_boundary",
            "required": True,
            "reason": "Confirm executable=false and preview_only guidance before acting.",
        },
        {
            "kind": "audit_result",
            "required": True,
            "reason": "Planning depends on the read-only audit state.",
        },
    ]
    if _string(plan.get("status")) == "blocked":
        requirements.append(
            {
                "kind": "blocker_review",
                "required": True,
                "reason": "Blocked plans require blocker inspection before further review.",
            }
        )
    if _string(plan.get("status")) == "needs_approval":
        requirements.append(
            {
                "kind": "gate_review",
                "required": True,
                "reason": "Review gates without recording a gate outcome or changing plan status.",
            }
        )
    if _string_list(plan.get("token_efficiency_notes")):
        requirements.append(
            {
                "kind": "token_budget_review",
                "required": False,
                "reason": "Token-efficiency notes may explain context or budget tradeoffs.",
            }
        )
    return requirements


def derive_token_efficiency_guidance(plan: dict[str, Any]) -> list[str]:
    """Return token-efficiency suggestions derived from one plan."""

    guidance: list[str] = []
    notes = _string_list(plan.get("token_efficiency_notes"))
    context_budget = _int(plan.get("context_budget"))
    execution_budget = _int(plan.get("execution_budget"))
    total_budget = _int(plan.get("total_token_budget"))

    if _has_budget_pressure(notes):
        guidance.append("Inspect whether summary or excerpt context is sufficient before increasing context budget.")
    if context_budget == 0:
        guidance.append("Context was omitted; request more context only if review evidence is insufficient.")
    if execution_budget < 1000:
        guidance.append("Execution budget is tight in the plan; prioritize verifier and gate review before optional detail.")
    if total_budget > 0:
        guidance.append(f"Total planned token budget is {total_budget}; compare with lower-budget variants when available.")
    if not guidance:
        guidance.append("No budget pressure detected; review planned steps before requesting more context.")
    return guidance


def _option(option: str, reason: str, allowed_effect: str) -> dict[str, str]:
    if option not in REVIEW_OPTION_NAMES:
        raise ValueError(f"unknown review guidance option: {option}")
    return {"option": option, "reason": reason, "allowed_effect": allowed_effect}


def _audit_verdict(plan: dict[str, Any]) -> str:
    audit = plan.get("audit_summary") if isinstance(plan.get("audit_summary"), dict) else {}
    return _string(audit.get("verdict"))


def _has_budget_pressure(notes: list[str]) -> bool:
    return any("budget pressure" in note.lower() for note in notes)


def _string_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [item for item in value if isinstance(item, str)]


def _list_of_dicts(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        return []
    return [item for item in value if isinstance(item, dict)]


def _string(value: Any) -> str:
    return value if isinstance(value, str) else ""


def _int(value: Any) -> int:
    return value if isinstance(value, int) else 0
