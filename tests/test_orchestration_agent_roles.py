"""Tests for orchestration/agent_role_registry.py — agent role registration and assignment."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.orchestration.agent_role_registry import AgentRoleRegistry
from harness_core.dispatch.orchestration.schemas import AgentRole


class AgentRoleRegistryTests(unittest.TestCase):
    def setUp(self):
        self.registry = AgentRoleRegistry()

    def test_register_and_get(self):
        role = AgentRole(role_id="r1", role_name="code_agent", capabilities=("code_review",))
        self.registry.register_role(role)
        self.assertEqual(self.registry.get_role("r1").role_name, "code_agent")

    def test_get_unknown_returns_none(self):
        self.assertIsNone(self.registry.get_role("nonexistent"))

    def test_roles_for_task_type(self):
        r1 = AgentRole(role_id="r1", role_name="code", capabilities=("code_review", "code_generate"))
        r2 = AgentRole(role_id="r2", role_name="docs", capabilities=("docs_review",))
        self.registry.register_role(r1)
        self.registry.register_role(r2)
        results = self.registry.roles_for_task_type("code_review")
        self.assertEqual(len(results), 1)
        self.assertEqual(results[0].role_id, "r1")

    def test_roles_for_task_type_empty(self):
        self.assertEqual(self.registry.roles_for_task_type("nonexistent"), [])

    def test_assign_agent(self):
        role = AgentRole(role_id="r1", role_name="code", capabilities=("code_review",), max_concurrent_nodes=2)
        self.registry.register_role(role)
        result = self.registry.assign_agent("w1", "n1", "code_review")
        self.assertEqual(result, "r1")

    def test_assign_agent_respects_concurrency(self):
        role = AgentRole(role_id="r1", role_name="code", capabilities=("code_review",), max_concurrent_nodes=1)
        self.registry.register_role(role)
        self.registry.assign_agent("w1", "n1", "code_review")
        result = self.registry.assign_agent("w1", "n2", "code_review")
        self.assertIsNone(result)

    def test_assign_agent_no_capable_role(self):
        result = self.registry.assign_agent("w1", "n1", "unknown_task")
        self.assertIsNone(result)

    def test_release_agent(self):
        role = AgentRole(role_id="r1", role_name="code", capabilities=("code_review",), max_concurrent_nodes=1)
        self.registry.register_role(role)
        self.registry.assign_agent("w1", "n1", "code_review")
        self.registry.release_agent("r1")
        result = self.registry.assign_agent("w1", "n2", "code_review")
        self.assertEqual(result, "r1")

    def test_all_roles(self):
        r1 = AgentRole(role_id="r1", role_name="code", capabilities=("code_review",))
        r2 = AgentRole(role_id="r2", role_name="docs", capabilities=("docs_review",))
        self.registry.register_role(r1)
        self.registry.register_role(r2)
        self.assertEqual(len(self.registry.all_roles()), 2)


if __name__ == "__main__":
    unittest.main()
