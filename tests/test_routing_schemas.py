"""Tests for routing/schemas.py — Phase 4 routing dataclasses."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.routing.schemas import (
    RoutingExperiment,
    RoutingArm,
    RoutingObservation,
    PROMOTION_GATE_DEFAULTS,
    EXPERIMENT_STATUSES,
    ROUTING_MODES,
    PROMOTION_VERDICTS,
    ROUTING_EXPERIMENT_SCHEMA_VERSION,
    ROUTING_ARM_SCHEMA_VERSION,
    ROUTING_OBSERVATION_SCHEMA_VERSION,
)


class RoutingObservationTests(unittest.TestCase):
    def test_construction(self):
        obs = RoutingObservation(
            observation_id="obs-1",
            arm_id="arm-1",
            dispatch_id="disp-1",
            task_domain="code",
            task_intent="review",
            selected_tier="balanced_worker",
            baseline_tier="cheap_executor",
            quality_score=0.85,
            cost=0.012,
            latency_ms=150,
            success=True,
        )
        self.assertEqual(obs.observation_id, "obs-1")
        self.assertEqual(obs.selected_tier, "balanced_worker")
        self.assertTrue(obs.success)
        self.assertFalse(obs.budget_violation)
        self.assertEqual(obs.schema_version, ROUTING_OBSERVATION_SCHEMA_VERSION)

    def test_to_dict(self):
        obs = RoutingObservation(
            observation_id="obs-2",
            arm_id="arm-2",
            dispatch_id="disp-2",
            task_domain="docs",
            task_intent="summarize",
            selected_tier="cheap_executor",
            baseline_tier="cheap_executor",
            quality_score=0.9,
            cost=0.001,
            latency_ms=50,
            success=True,
        )
        d = obs.to_dict()
        self.assertEqual(d["observation_id"], "obs-2")
        self.assertEqual(d["selected_tier"], "cheap_executor")
        self.assertTrue(d["success"])
        self.assertFalse(d["budget_violation"])

    def test_frozen(self):
        obs = RoutingObservation(
            observation_id="x", arm_id="a", dispatch_id="d",
            task_domain="c", task_intent="r", selected_tier="t",
            baseline_tier="b", quality_score=0.5, cost=0.01,
            latency_ms=100, success=True,
        )
        with self.assertRaises(AttributeError):
            obs.observation_id = "y"


class RoutingArmTests(unittest.TestCase):
    def test_construction(self):
        arm = RoutingArm(
            arm_id="arm-1",
            experiment_id="exp-1",
            tier="balanced_worker",
        )
        self.assertEqual(arm.tier, "balanced_worker")
        self.assertEqual(arm.traffic_weight, 1.0)
        self.assertEqual(arm.observations, ())

    def test_to_dict(self):
        obs = RoutingObservation(
            observation_id="o1", arm_id="a1", dispatch_id="d1",
            task_domain="code", task_intent="review",
            selected_tier="balanced_worker", baseline_tier="cheap_executor",
            quality_score=0.8, cost=0.01, latency_ms=100, success=True,
        )
        arm = RoutingArm(
            arm_id="a1", experiment_id="e1",
            tier="balanced_worker", observations=(obs,),
        )
        d = arm.to_dict()
        self.assertEqual(len(d["observations"]), 1)
        self.assertEqual(d["observations"][0]["observation_id"], "o1")

    def test_frozen(self):
        arm = RoutingArm(arm_id="a", experiment_id="e", tier="t")
        with self.assertRaises(AttributeError):
            arm.tier = "x"


class RoutingExperimentTests(unittest.TestCase):
    def test_construction(self):
        exp = RoutingExperiment(
            experiment_id="exp-1",
            name="Test Experiment",
            task_group="code_review",
        )
        self.assertEqual(exp.status, "created")
        self.assertIsNone(exp.start_time)
        self.assertIsNone(exp.end_time)
        self.assertIsNone(exp.conclusion)
        self.assertEqual(exp.schema_version, ROUTING_EXPERIMENT_SCHEMA_VERSION)

    def test_to_dict(self):
        arm = RoutingArm(arm_id="a1", experiment_id="exp-1", tier="t")
        exp = RoutingExperiment(
            experiment_id="exp-1", name="Test", task_group="g",
            arms=(arm,), status="running",
        )
        d = exp.to_dict()
        self.assertEqual(d["status"], "running")
        self.assertEqual(len(d["arms"]), 1)

    def test_frozen(self):
        exp = RoutingExperiment(experiment_id="e", name="n", task_group="g")
        with self.assertRaises(AttributeError):
            exp.name = "x"


class ConstantsTests(unittest.TestCase):
    def test_experiment_statuses(self):
        self.assertIn("created", EXPERIMENT_STATUSES)
        self.assertIn("running", EXPERIMENT_STATUSES)
        self.assertIn("concluded", EXPERIMENT_STATUSES)
        self.assertIn("rolled_back", EXPERIMENT_STATUSES)

    def test_routing_modes(self):
        self.assertIn("static", ROUTING_MODES)
        self.assertIn("adaptive", ROUTING_MODES)
        self.assertIn("shadow", ROUTING_MODES)

    def test_promotion_verdicts(self):
        self.assertIn("promote", PROMOTION_VERDICTS)
        self.assertIn("hold", PROMOTION_VERDICTS)
        self.assertIn("reject", PROMOTION_VERDICTS)
        self.assertIn("insufficient_data", PROMOTION_VERDICTS)

    def test_promotion_gate_defaults(self):
        self.assertEqual(PROMOTION_GATE_DEFAULTS["min_sample_count"], 30)
        self.assertEqual(PROMOTION_GATE_DEFAULTS["max_failure_rate_delta"], 0.05)
        self.assertEqual(PROMOTION_GATE_DEFAULTS["min_cost_reduction_pct"], 5.0)


if __name__ == "__main__":
    unittest.main()
