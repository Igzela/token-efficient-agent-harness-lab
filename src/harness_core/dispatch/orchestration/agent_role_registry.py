"""Agent role registry: defines available agent roles and handles assignment."""

from __future__ import annotations

from .schemas import AgentRole


class AgentRoleRegistry:
    """In-memory registry of available agent roles."""

    def __init__(self) -> None:
        self._roles: dict[str, AgentRole] = {}
        self._active_count: dict[str, int] = {}
        self._assignments: dict[tuple[str, str], str] = {}  # (workflow_id, node_id) -> role_id

    def register_role(self, role: AgentRole) -> None:
        self._roles[role.role_id] = role
        self._active_count.setdefault(role.role_id, 0)

    def get_role(self, role_id: str) -> AgentRole | None:
        return self._roles.get(role_id)

    def roles_for_task_type(self, task_type: str) -> list[AgentRole]:
        return [r for r in self._roles.values() if task_type in r.capabilities]

    def assign_agent(self, workflow_id: str, node_id: str, task_type: str) -> str | None:
        candidates = self.roles_for_task_type(task_type)
        for role in candidates:
            active = self._active_count.get(role.role_id, 0)
            if active < role.max_concurrent_nodes:
                self._active_count[role.role_id] = active + 1
                self._assignments[(workflow_id, node_id)] = role.role_id
                return role.role_id
        return None

    def release_agent(self, role_id: str) -> None:
        current = self._active_count.get(role_id, 0)
        self._active_count[role_id] = max(0, current - 1)

    def release_node(self, workflow_id: str, node_id: str) -> str | None:
        key = (workflow_id, node_id)
        role_id = self._assignments.pop(key, None)
        if role_id:
            self.release_agent(role_id)
        return role_id

    def get_assignment(self, workflow_id: str, node_id: str) -> str | None:
        return self._assignments.get((workflow_id, node_id))

    def all_roles(self) -> list[AgentRole]:
        return list(self._roles.values())
