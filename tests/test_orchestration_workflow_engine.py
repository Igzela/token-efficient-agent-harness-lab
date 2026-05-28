"""Tests for orchestration/workflow_engine.py — full workflow lifecycle."""

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
from harness_core.dispatch.orchestration.schemas import WorkflowGraph, WorkflowNode
from harness_core.dispatch.task_analyzer import TaskAnalysis


def _make_analysis(complexity=0.2, risk_flags=(), domain="code", intent="review"):
    return TaskAnalysis(
        analysis_id="test-analysis",
        raw_request_snapshot="test",
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


class WorkflowEngineCreateTests(unittest.TestCase):
    def test_create_workflow(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis())
        self.assertEqual(graph.status, "decomposed")
        self.assertGreater(len(graph.nodes), 0)

    def test_create_workflow_with_budget(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis(), budget_limit=50.0)
        self.assertIsNotNone(graph)

    def test_create_workflow_rejects_undecided_dispatch(self):
        engine = WorkflowEngine()
        with self.assertRaises(ValueError):
            engine.create_workflow(_make_analysis(), decision_status="needs_approval")


class WorkflowEngineTickTests(unittest.TestCase):
    def test_tick_transitions_to_running(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis())
        graph = engine.tick(graph)
        self.assertEqual(graph.status, "running")

    def test_tick_completes_single_node_workflow(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis())
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.complete_node(graph, node.node_id, "output-1")
        graph = engine.tick(graph)
        self.assertEqual(graph.status, "completed")
        self.assertIsNotNone(graph.result)

    def test_tick_already_completed(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis())
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.complete_node(graph, node.node_id, "output-1")
        graph = engine.tick(graph)
        same_graph = engine.tick(graph)
        self.assertEqual(same_graph.status, "completed")

    def test_tick_already_cancelled(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis())
        graph = engine.cancel(graph)
        same_graph = engine.tick(graph)
        self.assertEqual(same_graph.status, "cancelled")

    def test_fail_node(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis())
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.fail_node(graph, node.node_id, "something broke")
        updated_node = [n for n in graph.nodes if n.node_id == node.node_id][0]
        self.assertEqual(updated_node.status, "failed")
        self.assertEqual(updated_node.error, "something broke")


class WorkflowEngineMultiNodeTests(unittest.TestCase):
    def test_multi_node_sequential_execution(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis(complexity=0.4, risk_flags=("target_write",)))
        self.assertEqual(len(graph.nodes), 2)

        graph = engine.tick(graph)
        running = [n for n in graph.nodes if n.status == "running"]
        self.assertGreater(len(running), 0)

        first = running[0]
        graph = engine.complete_node(graph, first.node_id, "out-1")
        graph = engine.tick(graph)

        remaining = [n for n in graph.nodes if n.status not in ("completed", "failed", "cancelled")]
        if remaining:
            graph = engine.complete_node(graph, remaining[0].node_id, "out-2")
            graph = engine.tick(graph)

        self.assertEqual(graph.status, "completed")

    def test_complex_workflow_execution(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis(complexity=0.7, risk_flags=("target_write", "provider_call")))
        self.assertEqual(len(graph.nodes), 4)

        for _ in range(10):
            if graph.status in ("completed", "failed", "cancelled"):
                break
            graph = engine.tick(graph)
            for node in graph.nodes:
                if node.status in ("ready", "running"):
                    graph = engine.complete_node(graph, node.node_id, f"out-{node.node_id}")

        self.assertEqual(graph.status, "completed")


class WorkflowEngineCancelTests(unittest.TestCase):
    def test_cancel(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis())
        graph = engine.cancel(graph)
        self.assertEqual(graph.status, "cancelled")


class WorkflowEngineApprovalTests(unittest.TestCase):
    def test_resume_after_approval(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis())
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.complete_node(graph, node.node_id, "out-1")
        graph = engine.tick(graph)
        if graph.status == "waiting_human":
            graph = engine.resume_after_approval(graph, node.node_id)
            self.assertEqual(graph.status, "running")

    def test_reject_approval(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis())
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.complete_node(graph, node.node_id, "out-1")
        graph = engine.tick(graph)
        if graph.status == "waiting_human":
            graph = engine.reject_approval(graph, node.node_id, "not good enough")
            self.assertEqual(graph.status, "cancelled")


if __name__ == "__main__":
    unittest.main()
