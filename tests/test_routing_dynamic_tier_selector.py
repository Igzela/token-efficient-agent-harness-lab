"""Tests for routing/dynamic_tier_selector.py — adaptive tier selection."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.routing.cost_of_pass_router import CostOfPassRouter
from harness_core.dispatch.routing.dynamic_tier_selector import DynamicTierSelector
from harness_core.dispatch.routing.history_store import RoutingHistoryStore
from harness_core.dispatch.routing.promotion_gate import PromotionGate, RoutingObservationStore
from harness_core.dispatch.routing.schemas import RoutingObservation, RoutingSelection
from harness_core.dispatch.model_selector import ModelSelector
from harness_core.dispatch.task_analyzer import RuleBasedTaskAnalyzer
from harness_core.usage_ledger import UsageLedgerRow


def _make_row(profile_id: str, cost_group: str, cost: float, passed: bool) -> UsageLedgerRow:
    return UsageLedgerRow(
        run_id="run-1", case_id="case-1",
        input_tokens=100, output_tokens=50, cached_tokens=0,
        request_count=1, tool_call_count=0, retry_count=0,
        wall_clock_ms=100, estimated_cost=cost, pass_=passed,
        cost_of_pass_group=cost_group,
        model_profile_id=profile_id, context_pack_id="",
    )


def _make_obs(tier: str, domain: str, intent: str, quality: float, cost: float, success: bool) -> RoutingObservation:
    return RoutingObservation(
        observation_id=f"obs-{tier}", arm_id=f"arm-{tier}",
        dispatch_id="disp-1", task_domain=domain, task_intent=intent,
        selected_tier=tier, baseline_tier="balanced_worker",
        quality_score=quality, cost=cost, latency_ms=100, success=success,
    )


class DynamicTierSelectorTests(unittest.TestCase):
    def _build_selector(self, tier_map, tier_profile_map, obs_store=None, cost_group="s/c/r/q"):
        history = RoutingHistoryStore(tier_profile_map=tier_profile_map)
        for pid, tier in tier_profile_map.items():
            for i in range(35):
                history.add_row(_make_row(pid, cost_group, 0.005 if "cheap" in tier else 0.015, True))
        router = CostOfPassRouter(history, min_sample_count=30)
        gate_store = obs_store or RoutingObservationStore()
        gate = PromotionGate(gate_store, min_sample_count=0, min_cost_reduction_pct=0.0, max_failure_rate_delta=1.0)
        static = ModelSelector()
        return DynamicTierSelector(static, router, gate)

    def test_cold_start_falls_back_to_static(self):
        selector = DynamicTierSelector(
            static_selector=ModelSelector(),
            cost_of_pass_router=CostOfPassRouter(RoutingHistoryStore(), min_sample_count=30),
            promotion_gate=PromotionGate(RoutingObservationStore(), min_sample_count=0),
        )
        analyzer = RuleBasedTaskAnalyzer()
        analysis = analyzer.analyze("Summarize the README")
        result = selector.select(analysis)
        self.assertIsInstance(result, RoutingSelection)
        self.assertEqual(result.routing_mode, "static")
        self.assertIn("adaptive_cold_start_fallback", result.routing_reason)

    def test_adaptive_routing_used_when_data_sufficient(self):
        obs_store = RoutingObservationStore()
        for i in range(35):
            obs_store.add_observation(_make_obs("cheap_executor", "docs", "summarize", 0.9, 0.003, True))
            obs_store.add_observation(_make_obs("balanced_worker", "docs", "summarize", 0.85, 0.015, True))
        selector = self._build_selector(
            tier_map={"cheap-p": "cheap_executor", "bal-p": "balanced_worker"},
            tier_profile_map={"cheap-p": "cheap_executor", "bal-p": "balanced_worker"},
            obs_store=obs_store,
            cost_group="s/docs/summarize/quality",
        )
        analyzer = RuleBasedTaskAnalyzer()
        analysis = analyzer.analyze("Summarize the README")
        result = selector.select(analysis)
        self.assertIsInstance(result, RoutingSelection)
        self.assertEqual(result.selected_tier, "cheap_executor")
        self.assertEqual(result.routing_mode, "adaptive")
        self.assertIn("adaptive_routing", result.routing_reason)

    def test_hard_constraints_override_adaptive(self):
        obs_store = RoutingObservationStore()
        for i in range(35):
            obs_store.add_observation(_make_obs("cheap_executor", "code", "debug", 0.9, 0.003, True))
        selector = self._build_selector(
            tier_map={"cheap-p": "cheap_executor"},
            tier_profile_map={"cheap-p": "cheap_executor"},
            obs_store=obs_store,
            cost_group="s/code/debug/quality",
        )
        analyzer = RuleBasedTaskAnalyzer()
        analysis = analyzer.analyze("Debug the auth bug with low confidence issue")
        result = selector.select(analysis)
        self.assertIsInstance(result, RoutingSelection)
        self.assertIn(result.selected_tier, ("cheap_executor", "strong_planner", "balanced_worker"))

    def test_returns_routing_selection(self):
        selector = DynamicTierSelector(
            static_selector=ModelSelector(),
            cost_of_pass_router=CostOfPassRouter(RoutingHistoryStore(), min_sample_count=30),
            promotion_gate=PromotionGate(RoutingObservationStore(), min_sample_count=0),
        )
        analyzer = RuleBasedTaskAnalyzer()
        analysis = analyzer.analyze("Summarize the README")
        result = selector.select(analysis)
        self.assertIsInstance(result, RoutingSelection)
        self.assertIsInstance(result.selected_tier, str)
        self.assertIsNone(result.selected_profile_id)
        self.assertIsInstance(result.fallback_tier, str)
        self.assertIsInstance(result.shadow_routes, list)
        self.assertIsInstance(result.rejected_candidates, list)
        self.assertIsInstance(result.routing_reason, str)
        self.assertEqual(result.routing_mode, "static")
        self.assertIsNone(result.routing_experiment_id)

    def test_as_tuple_7_compatibility(self):
        selector = DynamicTierSelector(
            static_selector=ModelSelector(),
            cost_of_pass_router=CostOfPassRouter(RoutingHistoryStore(), min_sample_count=30),
            promotion_gate=PromotionGate(RoutingObservationStore(), min_sample_count=0),
        )
        analyzer = RuleBasedTaskAnalyzer()
        analysis = analyzer.analyze("Summarize the README")
        result = selector.select(analysis)
        t7 = result.as_tuple_7()
        self.assertEqual(len(t7), 7)
        selected_tier, profile_id, fallback_tier, fallback_id, shadows, rejected, reason = t7
        self.assertIsInstance(selected_tier, str)


if __name__ == "__main__":
    unittest.main()
