"""Dynamic tier selector: adaptive routing with cold-start fallback."""

from __future__ import annotations

from typing import Any

from ..dispatch_decision import MODEL_TIERS, RejectedCandidate, ShadowRoute
from ..model_selector import ModelSelector
from ..task_analyzer import TaskAnalysis
from .cost_of_pass_router import CostOfPassRouter
from .promotion_gate import PromotionGate
from .schemas import RoutingSelection, make_task_group


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
    ) -> RoutingSelection:
        task_group = make_task_group(analysis.task_domain, analysis.task_intent)

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

                    return RoutingSelection(
                        selected_tier=selected_tier,
                        selected_profile_id=None,
                        fallback_tier=fallback_tier,
                        fallback_profile_id=None,
                        shadow_routes=shadow_routes,
                        rejected_candidates=rejected,
                        routing_reason="; ".join(reasons),
                        routing_mode="adaptive",
                    )

        # Cold-start fallback: delegate to static selector, wrap in RoutingSelection
        static_result = self._static.select(analysis)
        return RoutingSelection(
            selected_tier=static_result[0],
            selected_profile_id=static_result[1],
            fallback_tier=static_result[2],
            fallback_profile_id=static_result[3],
            shadow_routes=static_result[4],
            rejected_candidates=static_result[5],
            routing_reason=f"adaptive_cold_start_fallback; {static_result[6]}",
            routing_mode="static",
        )

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
