"""Tests for routing adaptive engine integration — DispatchEngine with DynamicTierSelector."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.routing.cost_of_pass_router import CostOfPassRouter
from harness_core.dispatch.routing.dynamic_tier_selector import DynamicTierSelector
from harness_core.dispatch.routing.history_store import RoutingHistoryStore
from harness_core.dispatch.routing.promotion_gate import PromotionGate, RoutingObservationStore
from harness_core.dispatch.routing.schemas import RoutingObservation
from harness_core.dispatch.model_selector import ModelSelector
from harness_core.usage_ledger import UsageLedgerRow


def _make_row(profile_id: str, cost_group: str, cost: float, passed: bool) -> UsageLedgerRow:
    return UsageLedgerRow(
        run_id="r", case_id="c",
        input_tokens=100, output_tokens=50, cached_tokens=0,
        request_count=1, tool_call_count=0, retry_count=0,
        wall_clock_ms=100, estimated_cost=cost, pass_=passed,
        cost_of_pass_group=cost_group,
        model_profile_id=profile_id, context_pack_id="",
    )


def _make_obs(tier: str, domain: str, intent: str, quality: float, cost: float, success: bool) -> RoutingObservation:
    return RoutingObservation(
        observation_id=f"obs-{tier}", arm_id=f"arm-{tier}",
        dispatch_id="d", task_domain=domain, task_intent=intent,
        selected_tier=tier, baseline_tier="balanced_worker",
        quality_score=quality, cost=cost, latency_ms=100, success=success,
    )


class AdaptiveEngineIntegrationTests(unittest.TestCase):
    def test_engine_without_adaptive_is_backward_compatible(self):
        engine = DispatchEngine()
        bundle = engine.dispatch("Summarize the README")
        self.assertEqual(bundle.decision.routing_mode, "static")
        self.assertEqual(bundle.decision.selected_tier, "cheap_executor")

    def test_engine_with_adaptive_static_fallback(self):
        engine = DispatchEngine()
        bundle = engine.dispatch("Summarize the README")
        self.assertEqual(bundle.decision.routing_mode, "static")

    def test_engine_with_adaptive_selector(self):
        history = RoutingHistoryStore(tier_profile_map={"cheap-p": "cheap_executor", "bal-p": "balanced_worker"})
        for i in range(35):
            history.add_row(_make_row("cheap-p", "s/docs/summarize/quality", 0.005, True))
            history.add_row(_make_row("bal-p", "s/docs/summarize/quality", 0.015, True))
        obs_store = RoutingObservationStore()
        for i in range(35):
            obs_store.add_observation(_make_obs("cheap_executor", "docs", "summarize", 0.9, 0.003, True))
            obs_store.add_observation(_make_obs("balanced_worker", "docs", "summarize", 0.85, 0.015, True))
        router = CostOfPassRouter(history, min_sample_count=30)
        gate = PromotionGate(obs_store, min_sample_count=0, min_cost_reduction_pct=0.0, max_failure_rate_delta=1.0)
        selector = DynamicTierSelector(ModelSelector(), router, gate)
        engine = DispatchEngine(adaptive_selector=selector)
        bundle = engine.dispatch("Summarize the README")
        self.assertEqual(bundle.decision.routing_mode, "adaptive")
        self.assertEqual(bundle.decision.selected_tier, "cheap_executor")

    def test_cold_start_engine_falls_back(self):
        engine = DispatchEngine()
        bundle = engine.dispatch("Summarize the README")
        self.assertEqual(bundle.decision.routing_mode, "static")

    def test_routing_mode_in_to_dict(self):
        engine = DispatchEngine()
        bundle = engine.dispatch("Test request")
        d = bundle.decision.to_dict()
        self.assertEqual(d["routing_mode"], "static")
        self.assertIsNone(d["routing_experiment_id"])


if __name__ == "__main__":
    unittest.main()
