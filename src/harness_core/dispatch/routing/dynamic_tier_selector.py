"""Dynamic tier selector: adaptive routing with cold-start fallback."""

from __future__ import annotations

from typing import Any

from ..dispatch_decision import MODEL_TIERS, RejectedCandidate, ShadowRoute
from ..model_selector import ModelSelector
from ..task_analyzer import TaskAnalysis
from .cost_of_pass_router import CostOfPassRouter
from .promotion_gate import PromotionGate


class DynamicTierSelector:
    """Select tier dynamically per task group, falling back to static rules."""

    def __init__(
        self,
        static_selector: ModelSelector,
        cost_of_pass_router: CostOfPassRouter,
        promotion_gate: PromotionGate,
    ) -> None:
        self._static = static_selector
        self._router = cost_of_pass_router
        self._gate = promotion_gate

    def select(
        self, analysis: TaskAnalysis
    ) -> tuple[
        str,            # selected_tier
        str | None,     # selected_profile_id
        str,            # fallback_tier
        str | None,     # fallback_profile_id
        list[ShadowRoute],
        list[RejectedCandidate],
        str,            # routing_reason
    ]:
        task_group = f"{analysis.task_domain}_{analysis.task_intent}"

        if self._router.can_route_adaptively(task_group):
            result = self._router.best_tier_for_task_group(task_group)
            if result is not None:
                best_tier, _cop = result
                verdict = self._gate.evaluate(task_group, best_tier)
                if verdict.verdict == "promote":
                    rejected: list[RejectedCandidate] = []
                    reasons: list[str] = [f"adaptive_routing:cost_of_pass"]

                    selected_tier, rejected, reasons = self._apply_hard_constraints(
                        analysis, best_tier, rejected, reasons,
                    )

                    fallback_tier = self._fallback_tier(selected_tier)
                    shadow_routes = self._build_shadow_routes(analysis, selected_tier, fallback_tier)

                    return (
                        selected_tier,
                        None,
                        fallback_tier,
                        None,
                        shadow_routes,
                        rejected,
                        "; ".join(reasons),
                    )

        return self._static.select(analysis)

    def _apply_hard_constraints(
        self,
        analysis: TaskAnalysis,
        selected_tier: str,
        rejected: list[RejectedCandidate],
        reasons: list[str],
    ) -> tuple[str, list[RejectedCandidate], list[str]]:
        if analysis.confidence_label == "low":
            reasons.append("low_confidence_escalation")
            rejected.append(RejectedCandidate(
                tier=selected_tier,
                profile_id=None,
                reason="low confidence",
                constraint_failed="confidence_threshold",
            ))
            selected_tier = "strong_planner"

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

        if analysis.context_budget_estimate < 500:
            rejected.append(RejectedCandidate(
                tier="strong_planner",
                profile_id=None,
                reason="budget too low for strong_planner",
                constraint_failed="budget_threshold",
            ))
            reasons.append("budget_constrained")

        return selected_tier, rejected, reasons

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
        if fallback != selected:
            routes.append(ShadowRoute(
                tier=fallback,
                profile_id=None,
                reason="fallback option",
                admission_scope="diagnostic",
                expected_tradeoff="lower cost, potentially lower quality",
            ))
        if selected != "cheap_executor":
            routes.append(ShadowRoute(
                tier="cheap_executor",
                profile_id=None,
                reason="cost-optimized alternative",
                admission_scope="diagnostic",
                expected_tradeoff="lowest cost, adequate for simple tasks",
            ))
        if not routes:
            routes.append(ShadowRoute(
                tier=selected,
                profile_id=None,
                reason="self-diagnostic (no cheaper alternative)",
                admission_scope="diagnostic",
                expected_tradeoff="same tier, diagnostic comparison",
            ))
        return routes
