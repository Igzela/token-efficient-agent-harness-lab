"""Model selector with routing policy and shadow dual-track."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .dispatch_decision import MODEL_TIERS, RejectedCandidate, ShadowRoute
from .task_analyzer import TaskAnalysis

# ---------------------------------------------------------------------------
# Routing policy
# ---------------------------------------------------------------------------

_DEFAULT_TIER_MAP: dict[str, str] = {
    "code_generate": "balanced_worker",
    "code_review": "balanced_worker",
    "code_debug": "strong_planner",
    "code_refactor": "balanced_worker",
    "docs_summarize": "cheap_executor",
    "docs_generate": "cheap_executor",
    "docs_review": "cheap_executor",
    "docs_explain": "cheap_executor",
    "config_review": "cheap_executor",
    "config_generate": "balanced_worker",
    "infra_review": "balanced_worker",
    "infra_plan": "strong_planner",
    "math_generate": "strong_planner",
    "math_explain": "balanced_worker",
    "architecture_plan": "strong_planner",
    "architecture_design": "strong_planner",
    "repo_ops_review": "cheap_executor",
    "repo_ops_generate": "balanced_worker",
    "governance_audit": "verifier",
    "governance_review": "verifier",
    "other_classify": "cheap_executor",
}

_HIGH_RISK_OVERRIDES: dict[str, str] = {
    "cheap_executor": "balanced_worker",
    "balanced_worker": "strong_planner",
}


@dataclass(frozen=True)
class DispatchRoutingPolicy:
    policy_id: str
    tier_map: dict[str, str]  # "{domain}_{intent}" -> tier
    description: str = ""

    def select_tier(self, analysis: TaskAnalysis) -> str:
        key = f"{analysis.task_domain}_{analysis.task_intent}"
        tier = self.tier_map.get(key, "balanced_worker")
        if analysis.risk_level in ("critical", "high"):
            tier = _HIGH_RISK_OVERRIDES.get(tier, tier)
        return tier


_DEFAULT_POLICY = DispatchRoutingPolicy(
    policy_id="default_v1",
    tier_map=_DEFAULT_TIER_MAP,
    description="Default routing policy for Phase 1",
)


# ---------------------------------------------------------------------------
# Selector
# ---------------------------------------------------------------------------


class ModelSelector:
    """Select model tier based on analysis, with shadow dual-track."""

    def __init__(self, routing_policy: DispatchRoutingPolicy | None = None):
        self._policy = routing_policy or _DEFAULT_POLICY

    def select(
        self, analysis: TaskAnalysis
    ) -> tuple[
        str,            # selected_tier
        str | None,     # selected_profile_id
        str,            # fallback_tier
        str | None,     # fallback_profile_id
        list[ShadowRoute],      # shadow_routes
        list[RejectedCandidate], # rejected_candidates
        str,            # routing_reason
    ]:
        selected_tier = self._policy.select_tier(analysis)
        rejected: list[RejectedCandidate] = []
        reasons: list[str] = [f"policy_map:{analysis.task_domain}_{analysis.task_intent}"]

        # Hard constraint: low confidence -> escalate
        if analysis.confidence_label == "low":
            reasons.append("low_confidence_escalation")
            rejected.append(RejectedCandidate(
                tier=selected_tier,
                profile_id=None,
                reason="low confidence",
                constraint_failed="confidence_threshold",
            ))
            selected_tier = "strong_planner"

        # Hard constraint: critical risk -> use strongest available
        if analysis.risk_level == "critical":
            if selected_tier not in ("strong_planner", "advisor"):
                rejected.append(RejectedCandidate(
                    tier=selected_tier,
                    profile_id=None,
                    reason="critical risk requires stronger tier",
                    constraint_failed="risk_level",
                ))
                selected_tier = "strong_planner"
                reasons.append("critical_risk_override")

        # Budget feasibility check
        if analysis.context_budget_estimate < 500:
            rejected.append(RejectedCandidate(
                tier="strong_planner",
                profile_id=None,
                reason="budget too low for strong_planner",
                constraint_failed="budget_threshold",
            ))
            reasons.append("budget_constrained")

        # Fallback: one tier down
        fallback_tier = self._fallback_tier(selected_tier)

        # Shadow routes: always at least one diagnostic alternative
        shadow_routes = self._build_shadow_routes(analysis, selected_tier, fallback_tier)

        routing_reason = "; ".join(reasons)
        return (
            selected_tier,
            None,  # selected_profile_id (Phase 3+)
            fallback_tier,
            None,  # fallback_profile_id (Phase 3+)
            shadow_routes,
            rejected,
            routing_reason,
        )

    def _fallback_tier(self, selected: str) -> str:
        tier_order = list(MODEL_TIERS)
        idx = tier_order.index(selected) if selected in tier_order else 1
        if idx < len(tier_order) - 1:
            return tier_order[idx + 1]
        return tier_order[-1]

    def _build_shadow_routes(
        self, analysis: TaskAnalysis, selected: str, fallback: str
    ) -> list[ShadowRoute]:
        routes: list[ShadowRoute] = []

        # Always add fallback as shadow
        if fallback != selected:
            routes.append(ShadowRoute(
                tier=fallback,
                profile_id=None,
                reason="fallback option",
                admission_scope="diagnostic",
                expected_tradeoff="lower cost, potentially lower quality",
            ))

        # Add a cheaper alternative if not already cheapest
        if selected != "cheap_executor":
            routes.append(ShadowRoute(
                tier="cheap_executor",
                profile_id=None,
                reason="cost-optimized alternative",
                admission_scope="diagnostic",
                expected_tradeoff="lowest cost, adequate for simple tasks",
            ))

        # Ensure at least one shadow route
        if not routes:
            routes.append(ShadowRoute(
                tier=selected,
                profile_id=None,
                reason="self-diagnostic (no cheaper alternative)",
                admission_scope="diagnostic",
                expected_tradeoff="same tier, diagnostic comparison",
            ))

        return routes
