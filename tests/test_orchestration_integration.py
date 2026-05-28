"""Integration tests for orchestration — end-to-end multi-agent workflows."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.orchestration.workflow_engine import WorkflowEngine
from harness_core.dispatch.orchestration.task_decomposer import TaskDecomposer
from harness_core.dispatch.orchestration.dependency_resolver import DependencyResolver
from harness_core.dispatch.orchestration.work_queue import WorkQueue
from harness_core.dispatch.orchestration.conflict_resolver import ConflictResolver
from harness_core.dispatch.orchestration.result_aggregator import ResultAggregator
from harness_core.dispatch.orchestration.human_approval_gate import HumanApprovalGate
from harness_core.dispatch.orchestration.multi_agent_budget import MultiAgentBudgetManager
from harness_core.dispatch.orchestration.agent_role_registry import AgentRoleRegistry
from harness_core.dispatch.orchestration.schemas import AgentRole
from harness_core.dispatch.task_analyzer import TaskAnalysis


def _make_analysis(complexity=0.2, risk_flags=(), domain="code", intent="review"):
    return TaskAnalysis(
        analysis_id="integ-analysis",
        raw_request_snapshot="integration test",
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
        required_capabilities=(),
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
        features_detected={"domain": domain, "intent": intent},
        analysis_method="rule_only",
        created_at="2026-01-01T00:00:00Z",
    )


class SimpleWorkflowIntegrationTests(unittest.TestCase):
    def test_single_node_workflow_completes(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis())
        graph = engine.tick(graph)
        node = graph.nodes[0]
        graph = engine.complete_node(graph, node.node_id, "output-1", cost=0.5)
        graph = engine.tick(graph)
        self.assertEqual(graph.status, "completed")
        self.assertIsNotNone(graph.result)
        self.assertEqual(graph.result["completed_nodes"], 1)

    def test_single_node_workflow_with_budget(self):
        engine = WorkflowEngine(budget_manager=MultiAgentBudgetManager())
        graph = engine.create_workflow(_make_analysis(), budget_limit=10.0)
        graph = engine.tick(graph)
        node = graph.nodes[0]
        graph = engine.complete_node(graph, node.node_id, "output-1", cost=1.0)
        graph = engine.tick(graph)
        self.assertEqual(graph.status, "completed")


class MediumWorkflowIntegrationTests(unittest.TestCase):
    def test_two_node_sequential_workflow(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis(complexity=0.4, risk_flags=("target_write",)))
        self.assertEqual(len(graph.nodes), 2)

        for _ in range(5):
            if graph.status in ("completed", "failed", "cancelled"):
                break
            graph = engine.tick(graph)
            for node in graph.nodes:
                if node.status == "ready":
                    graph = engine.complete_node(graph, node.node_id, f"out-{node.node_id}", cost=0.3)

        self.assertEqual(graph.status, "completed")
        self.assertEqual(graph.result["completed_nodes"], 2)


class ComplexWorkflowIntegrationTests(unittest.TestCase):
    def test_four_node_workflow_completes(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(
            _make_analysis(complexity=0.7, risk_flags=("target_write", "provider_call")),
            budget_limit=50.0,
        )
        self.assertEqual(len(graph.nodes), 4)

        for _ in range(10):
            if graph.status in ("completed", "failed", "cancelled"):
                break
            graph = engine.tick(graph)
            for node in graph.nodes:
                if node.status == "ready":
                    graph = engine.complete_node(graph, node.node_id, f"out-{node.node_id}", cost=0.5)

        self.assertEqual(graph.status, "completed")
        self.assertEqual(graph.result["completed_nodes"], 4)
        self.assertEqual(graph.result["total_nodes"], 4)


class CancelWorkflowIntegrationTests(unittest.TestCase):
    def test_cancel_mid_execution(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(
            _make_analysis(complexity=0.7, risk_flags=("target_write", "provider_call")),
        )
        graph = engine.tick(graph)
        graph = engine.cancel(graph)
        self.assertEqual(graph.status, "cancelled")


class AgentRoleWorkflowIntegrationTests(unittest.TestCase):
    def test_workflow_with_agent_registry(self):
        registry = AgentRoleRegistry()
        registry.register_role(AgentRole(
            role_id="code-agent", role_name="Code Agent",
            capabilities=("code_review", "code_analyze", "code_execute", "code_plan"),
            max_concurrent_nodes=10, budget_limit=50.0,
        ))
        decomposer = TaskDecomposer(role_registry=registry)
        engine = WorkflowEngine(decomposer=decomposer)
        graph = engine.create_workflow(_make_analysis(complexity=0.7, risk_flags=("target_write", "provider_call")))
        agents = [n.assigned_agent_id for n in graph.nodes]
        self.assertTrue(all(a is not None for a in agents))


class BudgetWorkflowIntegrationTests(unittest.TestCase):
    def test_budget_enforcement_workflow(self):
        budget_mgr = MultiAgentBudgetManager()
        engine = WorkflowEngine(budget_manager=budget_mgr)
        graph = engine.create_workflow(_make_analysis(), budget_limit=1.0)
        graph = engine.tick(graph)
        node = graph.nodes[0]
        graph = engine.complete_node(graph, node.node_id, "out", cost=0.5)
        graph = engine.tick(graph)
        self.assertEqual(graph.status, "completed")
        self.assertAlmostEqual(budget_mgr.get_workflow_cost(graph.workflow_id), 0.5)


class ResultAggregationIntegrationTests(unittest.TestCase):
    def test_result_contains_all_node_results(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(
            _make_analysis(complexity=0.4, risk_flags=("target_write",)),
        )
        for _ in range(5):
            if graph.status in ("completed", "failed", "cancelled"):
                break
            graph = engine.tick(graph)
            for node in graph.nodes:
                if node.status == "ready":
                    graph = engine.complete_node(graph, node.node_id, f"out-{node.node_id}")

        self.assertEqual(graph.status, "completed")
        self.assertIn("node_results", graph.result)
        self.assertEqual(len(graph.result["node_results"]), 2)


if __name__ == "__main__":
    unittest.main()
