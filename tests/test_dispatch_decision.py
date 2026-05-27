"""Tests for dispatch_decision.py schemas."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_decision import (
    BudgetReservation,
    DispatchDecision,
    Evidence,
    ExecutionGate,
    RejectedCandidate,
    ShadowRoute,
    COMPLEXITY_WEIGHTS,
    EXECUTION_GATE_TYPES,
    DECISION_STATUSES,
)


def make_evidence(**overrides):
    defaults = dict(
        feature="test_feature",
        text="test text",
        span=(0, 10),
        polarity="positive",
        source="raw_request",
    )
    defaults.update(overrides)
    return Evidence(**defaults)


def make_shadow_route(**overrides):
    defaults = dict(
        tier="balanced_worker",
        profile_id=None,
        reason="test reason",
        admission_scope="diagnostic",
    )
    defaults.update(overrides)
    return ShadowRoute(**defaults)


def make_budget_reservation(**overrides):
    defaults = dict(
        reservation_id="res-001",
        decision_id="dec-001",
        currency="token",
        pre_budget=5000,
        reserved_input_tokens=3000,
        reserved_output_tokens=2000,
        reserved_total_tokens=5000,
        reserved_cost=0.05,
        status="reserved",
        created_at="2026-01-01T00:00:00Z",
        updated_at="2026-01-01T00:00:00Z",
    )
    defaults.update(overrides)
    return BudgetReservation(**defaults)


def make_execution_gate(**overrides):
    defaults = dict(
        gate_id="gate-001",
        gate_type="provider_disabled",
        severity="block",
        reason="provider disabled in Phase 1",
    )
    defaults.update(overrides)
    return ExecutionGate(**defaults)


def make_dispatch_decision(**overrides):
    defaults = dict(
        decision_id="dec-001",
        analysis_id="analysis-001",
        analysis_snapshot={"task_domain": "code", "task_intent": "review"},
        selected_tier="balanced_worker",
        fallback_tier="cheap_executor",
        routing_reason="default policy",
        quality_requirement="standard",
        expected_quality_band="medium",
        confidence=0.8,
        confidence_label="high",
        budget_reservation=make_budget_reservation(),
        execution_policy={"executor_type": "noop", "execution_allowed": True, "requires_human_review": False, "max_retries": 0},
        decision_status="decided",
        created_at="2026-01-01T00:00:00Z",
    )
    defaults.update(overrides)
    return DispatchDecision(**defaults)


class EvidenceTests(unittest.TestCase):
    def test_creation(self):
        e = make_evidence()
        self.assertEqual(e.feature, "test_feature")
        self.assertEqual(e.polarity, "positive")

    def test_to_dict(self):
        d = make_evidence().to_dict()
        self.assertIn("feature", d)
        self.assertIn("span", d)
        self.assertIsInstance(d["span"], list)

    def test_frozen(self):
        e = make_evidence()
        with self.assertRaises(AttributeError):
            e.feature = "changed"


class ShadowRouteTests(unittest.TestCase):
    def test_creation(self):
        sr = make_shadow_route()
        self.assertEqual(sr.admission_scope, "diagnostic")

    def test_to_dict(self):
        d = make_shadow_route().to_dict()
        self.assertIn("tier", d)
        self.assertIn("admission_scope", d)


class BudgetReservationTests(unittest.TestCase):
    def test_creation(self):
        br = make_budget_reservation()
        self.assertEqual(br.status, "reserved")
        self.assertEqual(br.reserved_total_tokens, 5000)

    def test_to_dict(self):
        d = make_budget_reservation().to_dict()
        self.assertIn("reservation_id", d)
        self.assertIn("budget_violation", d)


class ExecutionGateTests(unittest.TestCase):
    def test_creation(self):
        g = make_execution_gate()
        self.assertEqual(g.gate_type, "provider_disabled")
        self.assertFalse(g.cleared)

    def test_to_dict(self):
        d = make_execution_gate().to_dict()
        self.assertIn("gate_id", d)
        self.assertIn("clearance_required", d)


class RejectedCandidateTests(unittest.TestCase):
    def test_creation(self):
        rc = RejectedCandidate(tier="cheap_executor", profile_id=None, reason="too weak")
        self.assertEqual(rc.tier, "cheap_executor")

    def test_to_dict(self):
        d = RejectedCandidate(tier="t", profile_id=None, reason="r").to_dict()
        self.assertIn("tier", d)


class DispatchDecisionTests(unittest.TestCase):
    def test_creation(self):
        dd = make_dispatch_decision()
        self.assertEqual(dd.selected_tier, "balanced_worker")
        self.assertEqual(dd.decision_status, "decided")

    def test_to_dict(self):
        d = make_dispatch_decision().to_dict()
        self.assertIn("decision_id", d)
        self.assertIn("budget_reservation", d)
        self.assertIsInstance(d["budget_reservation"], dict)

    def test_with_gates(self):
        gates = (make_execution_gate(), make_execution_gate(gate_id="g2", gate_type="risk"))
        dd = make_dispatch_decision(execution_gates=gates)
        self.assertEqual(len(dd.execution_gates), 2)

    def test_with_shadow_routes(self):
        routes = (make_shadow_route(), make_shadow_route(tier="cheap_executor"))
        dd = make_dispatch_decision(shadow_routes=routes)
        self.assertEqual(len(dd.shadow_routes), 2)


class ConstantsTests(unittest.TestCase):
    def test_complexity_weights_sum_to_one(self):
        total = sum(COMPLEXITY_WEIGHTS.values())
        self.assertAlmostEqual(total, 1.0, places=6)

    def test_execution_gate_types(self):
        self.assertIn("provider_disabled", EXECUTION_GATE_TYPES)
        self.assertIn("sandbox_disabled", EXECUTION_GATE_TYPES)
        self.assertIn("target_write", EXECUTION_GATE_TYPES)

    def test_decision_statuses(self):
        self.assertIn("decided", DECISION_STATUSES)
        self.assertIn("needs_approval", DECISION_STATUSES)


if __name__ == "__main__":
    unittest.main()
