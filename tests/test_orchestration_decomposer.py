"""Tests for orchestration/task_decomposer.py — TaskAnalysis to WorkflowGraph decomposition."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.orchestration.agent_role_registry import AgentRoleRegistry
from harness_core.dispatch.orchestration.schemas import AgentRole
from harness_core.dispatch.orchestration.task_decomposer import TaskDecomposer
from harness_core.dispatch.task_analyzer import TaskAnalysis, RuleBasedTaskAnalyzer


def _make_analysis(
    domain: str = "code", intent: str = "review", complexity: float = 0.2,
    risk_flags: tuple[str, ...] = (), features: dict | None = None,
) -> TaskAnalysis:
    return TaskAnalysis(
        analysis_id="test-analysis",
        raw_request_snapshot="test request",
        request_source="test_fixture",
        primary_task_type=f"{domain}_{intent}",
        task_domain=domain,
        task_intent=intent,
        risk_flags=risk_flags,
        complexity_score=complexity,
        cognitive_complexity=0.3,
        context_complexity=0.2,
        execution_risk=0.1,
        ambiguity_score=0.1,
        required_capabilities=("code_analysis",),
        context_budget_estimate=3000,
        execution_budget_estimate=2000,
        quality_requirement="standard",
        risk_level="low",
        confidence=0.8,
        confidence_label="high",
        uncertainty_reason=(),
        safe_default="proceed_with_caution",
        escalation_trigger=None,
        positive_evidence=(),
        negative_evidence=(),
        features_detected=features or {"domain": domain, "intent": intent},
        analysis_method="rule_only",
        created_at="2026-01-01T00:00:00Z",
    )


class TaskDecomposerSimpleTests(unittest.TestCase):
    def test_simple_task_produces_single_node(self):
        decomposer = TaskDecomposer()
        analysis = _make_analysis(complexity=0.2, risk_flags=())
        graph = decomposer.decompose(analysis)
        self.assertEqual(len(graph.nodes), 1)
        self.assertEqual(len(graph.edges), 0)
        self.assertEqual(graph.status, "decomposed")

    def test_simple_task_has_correct_workflow_id(self):
        decomposer = TaskDecomposer()
        graph = decomposer.decompose(_make_analysis())
        self.assertTrue(graph.workflow_id.startswith("wf-"))
        self.assertEqual(graph.dispatch_id, "test-analysis")

    def test_medium_task_produces_two_nodes(self):
        decomposer = TaskDecomposer()
        analysis = _make_analysis(complexity=0.4, risk_flags=("target_write",))
        graph = decomposer.decompose(analysis)
        self.assertEqual(len(graph.nodes), 2)
        self.assertEqual(len(graph.edges), 1)

    def test_medium_task_edge_connects_nodes(self):
        decomposer = TaskDecomposer()
        graph = decomposer.decompose(_make_analysis(complexity=0.4, risk_flags=("target_write",)))
        edge = graph.edges[0]
        node_ids = {n.node_id for n in graph.nodes}
        self.assertIn(edge.from_node_id, node_ids)
        self.assertIn(edge.to_node_id, node_ids)

    def test_complex_task_produces_four_nodes(self):
        decomposer = TaskDecomposer()
        analysis = _make_analysis(complexity=0.7, risk_flags=("target_write", "provider_call"))
        graph = decomposer.decompose(analysis)
        self.assertEqual(len(graph.nodes), 4)
        self.assertEqual(len(graph.edges), 3)

    def test_complex_task_chain_structure(self):
        decomposer = TaskDecomposer()
        graph = decomposer.decompose(_make_analysis(complexity=0.7, risk_flags=("target_write", "provider_call")))
        task_types = [n.task_type for n in graph.nodes]
        self.assertTrue(any("analyze" in t for t in task_types))
        self.assertTrue(any("plan" in t for t in task_types))
        self.assertTrue(any("execute" in t for t in task_types))
        self.assertTrue(any("review" in t for t in task_types))


class TaskDecomposerRegistryTests(unittest.TestCase):
    def test_decomposer_with_registry_assigns_agents(self):
        registry = AgentRoleRegistry()
        registry.register_role(AgentRole(
            role_id="r1", role_name="code", capabilities=("code",), max_concurrent_nodes=10,
        ))
        decomposer = TaskDecomposer(role_registry=registry)
        graph = decomposer.decompose(_make_analysis(domain="code", intent="review"))
        self.assertIsNotNone(graph.nodes[0].assigned_agent_id)


class TaskDecomposerBudgetTests(unittest.TestCase):
    def test_nodes_have_budget_from_analysis(self):
        decomposer = TaskDecomposer()
        analysis = _make_analysis(complexity=0.7, risk_flags=("target_write", "provider_call"))
        graph = decomposer.decompose(analysis)
        for node in graph.nodes:
            self.assertGreater(node.budget, 0)


if __name__ == "__main__":
    unittest.main()
