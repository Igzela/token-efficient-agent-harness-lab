"""Hardening tests for Phase 5 orchestration — addresses GPT review P0/P1 items."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.orchestration.workflow_engine import WorkflowEngine
from harness_core.dispatch.orchestration.task_decomposer import TaskDecomposer
from harness_core.dispatch.orchestration.agent_role_registry import AgentRoleRegistry
from harness_core.dispatch.orchestration.multi_agent_budget import MultiAgentBudgetManager
from harness_core.dispatch.orchestration.human_approval_gate import HumanApprovalGate
from harness_core.dispatch.orchestration.conflict_resolver import ConflictResolver
from harness_core.dispatch.orchestration.result_aggregator import ResultAggregator
from harness_core.dispatch.orchestration.work_queue import WorkQueue
from harness_core.dispatch.orchestration.schemas import (
    AgentRole, WorkflowGraph, WorkflowNode, WorkflowEdge,
)
from harness_core.dispatch.task_analyzer import TaskAnalysis


def _make_analysis(complexity=0.2, risk_flags=(), domain="code", intent="review"):
    return TaskAnalysis(
        analysis_id="harden-analysis",
        raw_request_snapshot="hardening test",
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


# P0#1: DispatchDecision gating
class DispatchGatingTests(unittest.TestCase):
    def test_create_workflow_rejects_needs_approval(self):
        engine = WorkflowEngine()
        with self.assertRaises(ValueError) as ctx:
            engine.create_workflow(_make_analysis(), dispatch_id="disp-1", decision_status="needs_approval")
        self.assertIn("needs_approval", str(ctx.exception))

    def test_create_workflow_rejects_blocked(self):
        engine = WorkflowEngine()
        with self.assertRaises(ValueError):
            engine.create_workflow(_make_analysis(), dispatch_id="disp-1", decision_status="blocked")

    def test_create_workflow_accepts_decided(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1", decision_status="decided")
        self.assertEqual(graph.status, "decomposed")

    def test_create_workflow_with_explicit_dispatch_id(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis(), dispatch_id="dispatch-123")
        self.assertEqual(graph.dispatch_id, "dispatch-123")

    def test_create_workflow_requires_dispatch_id(self):
        engine = WorkflowEngine()
        with self.assertRaises(TypeError):
            engine.create_workflow(_make_analysis())


# P0#2: Terminal semantics — failed node never silently completes
class TerminalSemanticsTests(unittest.TestCase):
    def test_failed_node_workflow_never_completes(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.fail_node(graph, node.node_id, "breakage")
        graph = engine.tick(graph)
        self.assertIn(graph.status, ("failed", "waiting_human"))
        self.assertNotEqual(graph.status, "completed")

    def test_multi_node_with_failure_not_completed(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(
            _make_analysis(complexity=0.7, risk_flags=("target_write", "provider_call")),
            dispatch_id="disp-1",
        )
        graph = engine.tick(graph)
        for node in graph.nodes:
            if node.status == "running":
                graph = engine.fail_node(graph, node.node_id, "breakage")
                break
        graph = engine.tick(graph)
        self.assertNotEqual(graph.status, "completed")


# P0#3: Approval reachability for failed nodes
class ApprovalReachabilityTests(unittest.TestCase):
    def test_failed_node_can_trigger_approval(self):
        gate = HumanApprovalGate(risk_threshold=0.0)
        engine = WorkflowEngine(approval_gate=gate)
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.fail_node(graph, node.node_id, "failure")
        graph = engine.tick(graph)
        self.assertEqual(graph.status, "waiting_human")

    def test_completed_node_can_trigger_approval(self):
        gate = HumanApprovalGate(risk_threshold=0.0)
        engine = WorkflowEngine(approval_gate=gate)
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.complete_node(graph, node.node_id, "out", cost=1.0)
        graph = engine.tick(graph)
        self.assertEqual(graph.status, "waiting_human")


# P0#4: Budget enforcement
class BudgetEnforcementTests(unittest.TestCase):
    def test_budget_overrun_triggers_fail(self):
        budget = MultiAgentBudgetManager(overrun_strategy="cancel")
        engine = WorkflowEngine(budget_manager=budget)
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1", budget_limit=1.0)
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.complete_node(graph, node.node_id, "out", cost=5.0)
        self.assertEqual(graph.status, "failed")

    def test_budget_within_limit_succeeds(self):
        budget = MultiAgentBudgetManager(overrun_strategy="cancel")
        engine = WorkflowEngine(budget_manager=budget)
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1", budget_limit=100.0)
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.complete_node(graph, node.node_id, "out", cost=0.5)
        graph = engine.tick(graph)
        self.assertEqual(graph.status, "completed")

    def test_budget_recorded_with_agent_id(self):
        budget = MultiAgentBudgetManager()
        registry = AgentRoleRegistry()
        registry.register_role(AgentRole(
            role_id="r1", role_name="code", capabilities=("code",),
            max_concurrent_nodes=10,
        ))
        decomposer = TaskDecomposer(role_registry=registry)
        engine = WorkflowEngine(
            decomposer=decomposer, budget_manager=budget, role_registry=registry,
        )
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1", budget_limit=100.0)
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        self.assertIsNotNone(node.assigned_agent_id)
        graph = engine.complete_node(graph, node.node_id, "out", cost=1.0)
        agent_cost = budget.get_agent_cost(graph.workflow_id, node.assigned_agent_id)
        self.assertAlmostEqual(agent_cost, 1.0)


# P0#5: Agent registry lifecycle
class AgentReleaseTests(unittest.TestCase):
    def test_agent_released_on_complete(self):
        registry = AgentRoleRegistry()
        registry.register_role(AgentRole(
            role_id="r1", role_name="code", capabilities=("code",),
            max_concurrent_nodes=1,
        ))
        decomposer = TaskDecomposer(role_registry=registry)
        engine = WorkflowEngine(decomposer=decomposer, role_registry=registry)
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.complete_node(graph, node.node_id, "out")
        assignment = registry.get_assignment(graph.workflow_id, node.node_id)
        self.assertIsNone(assignment)

    def test_agent_released_on_fail(self):
        registry = AgentRoleRegistry()
        registry.register_role(AgentRole(
            role_id="r1", role_name="code", capabilities=("code",),
            max_concurrent_nodes=1,
        ))
        decomposer = TaskDecomposer(role_registry=registry)
        engine = WorkflowEngine(decomposer=decomposer, role_registry=registry)
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.fail_node(graph, node.node_id, "err")
        assignment = registry.get_assignment(graph.workflow_id, node.node_id)
        self.assertIsNone(assignment)

    def test_agent_released_on_cancel(self):
        registry = AgentRoleRegistry()
        registry.register_role(AgentRole(
            role_id="r1", role_name="code", capabilities=("code",),
            max_concurrent_nodes=1,
        ))
        decomposer = TaskDecomposer(role_registry=registry)
        engine = WorkflowEngine(decomposer=decomposer, role_registry=registry)
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.cancel(graph)
        for node in graph.nodes:
            assignment = registry.get_assignment(graph.workflow_id, node.node_id)
            self.assertIsNone(assignment)

    def test_concurrency_freed_after_complete(self):
        registry = AgentRoleRegistry()
        registry.register_role(AgentRole(
            role_id="r1", role_name="code", capabilities=("code",),
            max_concurrent_nodes=1,
        ))
        decomposer = TaskDecomposer(role_registry=registry)
        engine = WorkflowEngine(decomposer=decomposer, role_registry=registry)
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.complete_node(graph, node.node_id, "out")
        result = registry.assign_agent("other-wf", "other-node", "code")
        self.assertEqual(result, "r1")


# P0#6: Graph/queue state consistency
class StateConsistencyTests(unittest.TestCase):
    def test_graph_status_consistent_after_tick(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.tick(graph)
        for node in graph.nodes:
            self.assertIn(node.status, ("running", "pending", "ready", "completed", "failed", "cancelled"))

    def test_graph_status_consistent_after_complete(self):
        engine = WorkflowEngine()
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.complete_node(graph, node.node_id, "out")
        completed = [n for n in graph.nodes if n.node_id == node.node_id][0]
        self.assertEqual(completed.status, "completed")


# P1: Schema alignment
class SchemaAlignmentTests(unittest.TestCase):
    def test_workflow_graph_has_updated_at(self):
        graph = WorkflowGraph(workflow_id="w1", dispatch_id="d1", updated_at="2026-01-01T00:00:00Z")
        self.assertEqual(graph.updated_at, "2026-01-01T00:00:00Z")

    def test_workflow_graph_to_dict_includes_updated_at(self):
        graph = WorkflowGraph(workflow_id="w1", dispatch_id="d1", updated_at="2026-01-01T00:00:00Z")
        d = graph.to_dict()
        self.assertIn("updated_at", d)
        self.assertEqual(d["updated_at"], "2026-01-01T00:00:00Z")

    def test_decompose_sets_updated_at(self):
        decomposer = TaskDecomposer()
        graph = decomposer.decompose(_make_analysis(), dispatch_id="disp-1")
        self.assertTrue(len(graph.updated_at) > 0)


# P1: DependencyResolver.execution_order validates first
class DependencyResolverValidationTests(unittest.TestCase):
    def test_execution_order_returns_empty_for_invalid_graph(self):
        from harness_core.dispatch.orchestration.dependency_resolver import DependencyResolver
        resolver = DependencyResolver()
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="t", assigned_agent_id=None)
        edge = WorkflowEdge(edge_id="e1", from_node_id="missing", to_node_id="n1")
        graph = WorkflowGraph(workflow_id="w1", dispatch_id="d1", nodes=(n1,), edges=(edge,))
        order = resolver.execution_order(graph)
        self.assertEqual(order, [])


# P1: Conflict resolution affects workflow status
class ConflictResolutionPersistenceTests(unittest.TestCase):
    def test_budget_overrun_cancels_workflow(self):
        engine = WorkflowEngine()
        n1 = WorkflowNode(
            node_id="n1", workflow_id="w1", task_type="t",
            assigned_agent_id=None, status="completed",
            budget=1.0, cost_incurred=5.0,
        )
        graph = WorkflowGraph(
            workflow_id="w1", dispatch_id="d1", nodes=(n1,), status="running",
        )
        conflicts = engine._conflict_resolver.detect_conflicts(graph)
        self.assertTrue(any(c.conflict_type == "budget_overrun" for c in conflicts))
        graph = engine._resolve_conflicts(graph, [c for c in conflicts if c.resolution_strategy is None])
        self.assertEqual(graph.status, "cancelled")


# P0: Terminal path cleanup — cancel and budget-overrun must release agents
class TerminalCleanupTests(unittest.TestCase):
    def test_cancel_running_node_releases_registry(self):
        registry = AgentRoleRegistry()
        registry.register_role(AgentRole(
            role_id="r1", role_name="code", capabilities=("code",),
            max_concurrent_nodes=1,
        ))
        decomposer = TaskDecomposer(role_registry=registry)
        engine = WorkflowEngine(decomposer=decomposer, role_registry=registry)
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.cancel(graph)
        assignment = registry.get_assignment(graph.workflow_id, node.node_id)
        self.assertIsNone(assignment)

    def test_cancel_running_node_changes_status(self):
        registry = AgentRoleRegistry()
        registry.register_role(AgentRole(
            role_id="r1", role_name="code", capabilities=("code",),
            max_concurrent_nodes=1,
        ))
        decomposer = TaskDecomposer(role_registry=registry)
        engine = WorkflowEngine(decomposer=decomposer, role_registry=registry)
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.cancel(graph)
        updated = [n for n in graph.nodes if n.node_id == node.node_id][0]
        self.assertEqual(updated.status, "cancelled")

    def test_budget_overrun_releases_assigned_agent(self):
        registry = AgentRoleRegistry()
        registry.register_role(AgentRole(
            role_id="r1", role_name="code", capabilities=("code",),
            max_concurrent_nodes=10,
        ))
        decomposer = TaskDecomposer(role_registry=registry)
        budget = MultiAgentBudgetManager(overrun_strategy="cancel")
        engine = WorkflowEngine(
            decomposer=decomposer, budget_manager=budget, role_registry=registry,
        )
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1", budget_limit=1.0)
        graph = engine.tick(graph)
        node = [n for n in graph.nodes if n.status == "running"][0]
        graph = engine.complete_node(graph, node.node_id, "out", cost=5.0)
        self.assertEqual(graph.status, "failed")
        assignment = registry.get_assignment(graph.workflow_id, node.node_id)
        self.assertIsNone(assignment)

    def test_cancelled_workflow_has_no_running_nodes(self):
        registry = AgentRoleRegistry()
        registry.register_role(AgentRole(
            role_id="r1", role_name="code", capabilities=("code",),
            max_concurrent_nodes=1,
        ))
        decomposer = TaskDecomposer(role_registry=registry)
        engine = WorkflowEngine(decomposer=decomposer, role_registry=registry)
        graph = engine.create_workflow(_make_analysis(), dispatch_id="disp-1")
        graph = engine.tick(graph)
        graph = engine.cancel(graph)
        self.assertEqual(graph.status, "cancelled")
        for node in graph.nodes:
            self.assertNotEqual(node.status, "running")


if __name__ == "__main__":
    unittest.main()
