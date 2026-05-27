"""Tests for routing/promotion_gate.py — evidence thresholds for promotion."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.routing.promotion_gate import PromotionGate, RoutingObservationStore
from harness_core.dispatch.routing.schemas import RoutingObservation


def _make_obs(tier: str, domain: str, intent: str, quality: float, cost: float, success: bool) -> RoutingObservation:
    return RoutingObservation(
        observation_id=f"obs-{tier}-{quality}", arm_id=f"arm-{tier}",
        dispatch_id="disp-1", task_domain=domain, task_intent=intent,
        selected_tier=tier, baseline_tier="balanced_worker",
        quality_score=quality, cost=cost, latency_ms=100, success=success,
    )


class PromotionGateTests(unittest.TestCase):
    def test_insufficient_data(self):
        store = RoutingObservationStore()
        gate = PromotionGate(store, min_sample_count=30)
        verdict = gate.evaluate("code_review", "cheap_executor")
        self.assertEqual(verdict.verdict, "insufficient_data")
        self.assertEqual(verdict.sample_count, 0)

    def test_promotes_when_all_conditions_met(self):
        store = RoutingObservationStore()
        for i in range(35):
            store.add_observation(_make_obs("cheap_executor", "code", "review", 0.9, 0.005, True))
            store.add_observation(_make_obs("balanced_worker", "code", "review", 0.85, 0.015, True))
        gate = PromotionGate(store, min_sample_count=30, min_cost_reduction_pct=5.0, max_failure_rate_delta=0.05)
        verdict = gate.evaluate("code_review", "cheap_executor", "balanced_worker")
        self.assertEqual(verdict.verdict, "promote")
        self.assertGreater(verdict.sample_count, 0)

    def test_holds_on_quality_regression(self):
        store = RoutingObservationStore()
        for i in range(35):
            store.add_observation(_make_obs("cheap_executor", "code", "review", 0.5, 0.005, True))
            store.add_observation(_make_obs("balanced_worker", "code", "review", 0.9, 0.015, True))
        gate = PromotionGate(store, min_sample_count=30, min_cost_reduction_pct=0.0, max_failure_rate_delta=1.0)
        verdict = gate.evaluate("code_review", "cheap_executor", "balanced_worker")
        self.assertEqual(verdict.verdict, "hold")
        self.assertIn("quality_regression", verdict.reasons[0])

    def test_holds_on_insufficient_cost_reduction(self):
        store = RoutingObservationStore()
        for i in range(35):
            store.add_observation(_make_obs("cheap_executor", "code", "review", 0.9, 0.014, True))
            store.add_observation(_make_obs("balanced_worker", "code", "review", 0.85, 0.015, True))
        gate = PromotionGate(store, min_sample_count=30, min_cost_reduction_pct=10.0, max_failure_rate_delta=1.0)
        verdict = gate.evaluate("code_review", "cheap_executor", "balanced_worker")
        self.assertEqual(verdict.verdict, "hold")

    def test_holds_on_worse_failure_rate(self):
        store = RoutingObservationStore()
        for i in range(35):
            store.add_observation(_make_obs("cheap_executor", "code", "review", 0.9, 0.005, i < 25))
            store.add_observation(_make_obs("balanced_worker", "code", "review", 0.85, 0.015, True))
        gate = PromotionGate(store, min_sample_count=30, min_cost_reduction_pct=0.0, max_failure_rate_delta=0.05)
        verdict = gate.evaluate("code_review", "cheap_executor", "balanced_worker")
        self.assertEqual(verdict.verdict, "hold")

    def test_human_review_required(self):
        store = RoutingObservationStore()
        for i in range(35):
            store.add_observation(_make_obs("cheap_executor", "code", "review", 0.9, 0.005, True))
            store.add_observation(_make_obs("balanced_worker", "code", "review", 0.85, 0.015, True))
        gate = PromotionGate(store, min_sample_count=30, min_cost_reduction_pct=0.0, max_failure_rate_delta=1.0, require_human_review=True)
        verdict = gate.evaluate("code_review", "cheap_executor", "balanced_worker")
        self.assertEqual(verdict.verdict, "hold")
        self.assertTrue(verdict.requires_human_review)

    def test_promotion_verdict_to_dict(self):
        store = RoutingObservationStore()
        gate = PromotionGate(store)
        verdict = gate.evaluate("code_review", "cheap_executor")
        d = verdict.to_dict()
        self.assertEqual(d["verdict"], "insufficient_data")
        self.assertEqual(d["task_group"], "code_review")
        self.assertIn("reasons", d)

    def test_check_sample_count(self):
        store = RoutingObservationStore()
        for i in range(10):
            store.add_observation(_make_obs("cheap_executor", "code", "review", 0.9, 0.005, True))
        gate = PromotionGate(store, min_sample_count=30)
        sufficient, count = gate.check_sample_count("code_review", "cheap_executor")
        self.assertFalse(sufficient)
        self.assertEqual(count, 10)


class RoutingObservationStoreTests(unittest.TestCase):
    def test_add_and_query(self):
        store = RoutingObservationStore()
        obs = _make_obs("cheap_executor", "code", "review", 0.9, 0.005, True)
        store.add_observation(obs)
        self.assertEqual(store.total_count(), 1)
        self.assertEqual(len(store.observations_for_arm("arm-cheap_executor")), 1)

    def test_count_for_tier_and_group(self):
        store = RoutingObservationStore()
        store.add_observation(_make_obs("cheap_executor", "code", "review", 0.9, 0.005, True))
        store.add_observation(_make_obs("balanced_worker", "code", "review", 0.85, 0.015, True))
        self.assertEqual(store.count_for_tier_and_group("cheap_executor", "code", "review"), 1)

    def test_all_observations(self):
        store = RoutingObservationStore()
        store.add_observation(_make_obs("cheap_executor", "c", "r", 0.9, 0.01, True))
        store.add_observation(_make_obs("balanced_worker", "c", "r", 0.8, 0.02, True))
        self.assertEqual(len(store.all_observations()), 2)


if __name__ == "__main__":
    unittest.main()
