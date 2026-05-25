"""Deterministic planning kernel for the local Harness app.

MVP3 plans work, budgets, and approval gates. It does not execute work, call
providers, start sandboxes, dispatch workers, or write target repositories.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import hashlib
import json
from typing import Any

from harness_core.app_registry import RepoRef
from harness_core.instance_audit import InstanceAuditReport


PLAN_STATUSES = ("ready_for_review", "needs_approval", "blocked")
PLAN_SCHEMA_VERSION = "resource_plan.v1"
TASK_SCHEMA_VERSION = "planning_task.v1"

_RISK_ORDER = {"low": 0, "medium": 1, "high": 2, "critical": 3}
_RISK_NAMES = {value: key for key, value in _RISK_ORDER.items()}
_HIGH_RISK_KEYWORDS = (
    "write",
    "modify",
    "delete",
    "remove",
    "deploy",
    "release",
    "push",
    "merge",
    "provider",
    "api key",
    "credential",
    "sandbox",
    "container",
    "worker",
    "concurrent",
    "mcp",
    "autonomous",
    "execute",
    "run command",
)
_READ_ONLY_KEYWORDS = ("read-only", "readonly", "audit", "inspect", "review", "docs", "document")
_HIGH_RISK_TASK_TYPES = {"write", "code_change", "deploy", "provider", "sandbox", "worker", "mcp"}


class PlanningInputError(ValueError):
    """Raised when planning input is malformed."""


@dataclass(frozen=True)
class PlanningTask:
    task_id: str
    repo_id: str
    objective: str
    task_type: str = "general"
    risk_level: str = "medium"
    constraints: tuple[str, ...] = ()
    max_context_tokens: int = 4000
    max_execution_tokens: int = 3000
    schema_version: str = TASK_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "task_id": self.task_id,
            "repo_id": self.repo_id,
            "objective": self.objective,
            "task_type": self.task_type,
            "risk_level": self.risk_level,
            "constraints": list(self.constraints),
            "max_context_tokens": self.max_context_tokens,
            "max_execution_tokens": self.max_execution_tokens,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "PlanningTask":
        constraints = data.get("constraints", ())
        if constraints is None:
            constraints = ()
        if not isinstance(constraints, (list, tuple)) or not all(isinstance(item, str) for item in constraints):
            raise PlanningInputError("constraints must be a list of strings")
        return cls(
            task_id=_string_or_default(data.get("task_id"), "task"),
            repo_id=_required_string(data, "repo_id"),
            objective=_required_string(data, "objective"),
            task_type=_string_or_default(data.get("task_type"), "general"),
            risk_level=_string_or_default(data.get("risk_level"), "medium"),
            constraints=tuple(constraints),
            max_context_tokens=_non_negative_int(data.get("max_context_tokens", 4000), "max_context_tokens"),
            max_execution_tokens=_non_negative_int(data.get("max_execution_tokens", 3000), "max_execution_tokens"),
        )


@dataclass(frozen=True)
class PlannedStep:
    step_id: str
    role: str
    action: str
    token_budget: int
    context_mode: str
    approval_required: bool
    reason: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "step_id": self.step_id,
            "role": self.role,
            "action": self.action,
            "token_budget": self.token_budget,
            "context_mode": self.context_mode,
            "approval_required": self.approval_required,
            "reason": self.reason,
        }


@dataclass(frozen=True)
class ResourcePlan:
    plan_id: str
    task: PlanningTask
    repo_snapshot: dict[str, Any]
    audit_summary: dict[str, Any]
    status: str
    executable: bool
    effective_risk: str
    total_token_budget: int
    context_budget: int
    execution_budget: int
    steps: tuple[PlannedStep, ...] = ()
    approval_gates: tuple[str, ...] = ()
    token_efficiency_notes: tuple[str, ...] = ()
    blockers: tuple[str, ...] = ()
    schema_version: str = PLAN_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "plan_id": self.plan_id,
            "task": self.task.to_dict(),
            "repo_snapshot": dict(self.repo_snapshot),
            "audit_summary": dict(self.audit_summary),
            "status": self.status,
            "executable": self.executable,
            "effective_risk": self.effective_risk,
            "total_token_budget": self.total_token_budget,
            "context_budget": self.context_budget,
            "execution_budget": self.execution_budget,
            "steps": [step.to_dict() for step in self.steps],
            "approval_gates": list(self.approval_gates),
            "token_efficiency_notes": list(self.token_efficiency_notes),
            "blockers": list(self.blockers),
        }


class DeterministicResourcePlanner:
    """Convert a task plus repo/audit state into a non-executable resource plan."""

    def plan(
        self,
        task: PlanningTask,
        repo_ref: RepoRef,
        audit_report: InstanceAuditReport | None,
    ) -> ResourcePlan:
        _validate_task(task)
        repo_snapshot = _repo_snapshot(repo_ref)
        audit_summary = _audit_summary(audit_report)
        blockers: list[str] = []

        if repo_ref.kind == "remote":
            blockers.append("remote_metadata_only")
        elif audit_report is None:
            blockers.append("audit_required")
        elif audit_report.verdict == "BLOCKED":
            blockers.append("audit_blocked")

        context_mode, context_budget, notes = _select_context_budget(task.max_context_tokens)
        execution_budget = task.max_execution_tokens
        total_budget = context_budget + execution_budget
        effective_risk = _effective_risk(task, audit_report)
        approval_gates = _approval_gates(task, effective_risk)

        if blockers:
            status = "blocked"
            steps: tuple[PlannedStep, ...] = ()
        else:
            status = "needs_approval" if approval_gates else "ready_for_review"
            steps = _make_steps(task, context_mode, execution_budget, bool(approval_gates))

        if task.max_execution_tokens < 1000:
            notes.append("Execution budget is tight; planner keeps verifier budget before optional advisor budget.")
        if status == "needs_approval":
            notes.append("Plan is held for human approval because inferred risk exceeds read-only review.")
        if status == "ready_for_review":
            notes.append("Plan is reviewable without execution authority; executable remains false.")

        plan_dict_seed = {
            "task": task.to_dict(),
            "repo": repo_snapshot,
            "audit": audit_summary,
            "status": status,
            "risk": effective_risk,
            "budgets": [total_budget, context_budget, execution_budget],
            "approval_gates": approval_gates,
            "blockers": blockers,
        }
        plan_id = "plan-" + hashlib.sha256(json.dumps(plan_dict_seed, sort_keys=True).encode("utf-8")).hexdigest()[:16]

        return ResourcePlan(
            plan_id=plan_id,
            task=task,
            repo_snapshot=repo_snapshot,
            audit_summary=audit_summary,
            status=status,
            executable=False,
            effective_risk=effective_risk,
            total_token_budget=total_budget,
            context_budget=context_budget,
            execution_budget=execution_budget,
            steps=steps,
            approval_gates=tuple(approval_gates),
            token_efficiency_notes=tuple(notes),
            blockers=tuple(blockers),
        )


def _validate_task(task: PlanningTask) -> None:
    if not task.objective.strip():
        raise PlanningInputError("objective is required")
    if task.risk_level not in _RISK_ORDER:
        raise PlanningInputError("risk_level must be low, medium, high, or critical")
    if task.max_context_tokens < 0 or task.max_execution_tokens < 0:
        raise PlanningInputError("token budgets must be non-negative")


def _repo_snapshot(repo_ref: RepoRef) -> dict[str, Any]:
    snapshot = {
        "id": repo_ref.id,
        "name": repo_ref.name,
        "kind": repo_ref.kind,
    }
    if repo_ref.kind == "local":
        snapshot["canonical_path"] = repo_ref.path
    else:
        snapshot["url"] = repo_ref.url
    if repo_ref.branch:
        snapshot["branch"] = repo_ref.branch
    return snapshot


def _audit_summary(audit_report: InstanceAuditReport | None) -> dict[str, Any]:
    if audit_report is None:
        return {"verdict": "MISSING", "blocker_count": 0, "warning_count": 0}
    return {
        "verdict": audit_report.verdict,
        "blocker_count": len(audit_report.blockers),
        "warning_count": len(audit_report.warnings),
    }


def _effective_risk(task: PlanningTask, audit_report: InstanceAuditReport | None) -> str:
    risk = _RISK_ORDER[task.risk_level]
    text = f"{task.objective} {task.task_type} {' '.join(task.constraints)}".lower()
    if any(keyword in text for keyword in _HIGH_RISK_KEYWORDS):
        risk = max(risk, _RISK_ORDER["high"])
    if task.task_type.lower() in _HIGH_RISK_TASK_TYPES:
        risk = max(risk, _RISK_ORDER["high"])
    if audit_report is not None and audit_report.verdict == "PASS_WITH_NOTES":
        risk = max(risk, _RISK_ORDER["medium"])
    if audit_report is not None and audit_report.verdict == "BLOCKED":
        risk = max(risk, _RISK_ORDER["critical"])
    if any(keyword in text for keyword in _READ_ONLY_KEYWORDS) and risk < _RISK_ORDER["high"]:
        risk = max(risk, _RISK_ORDER["low"])
    return _RISK_NAMES[risk]


def _approval_gates(task: PlanningTask, effective_risk: str) -> list[str]:
    gates: list[str] = []
    text = f"{task.objective} {task.task_type} {' '.join(task.constraints)}".lower()
    if _RISK_ORDER[effective_risk] >= _RISK_ORDER["high"]:
        gates.append("human_approval_required")
    if any(keyword in text for keyword in ("write", "modify", "delete", "remove", "push", "merge")):
        gates.append("target_repo_mutation_gate")
    if any(keyword in text for keyword in ("deploy", "release")):
        gates.append("deployment_gate")
    if any(keyword in text for keyword in ("provider", "api key", "credential")):
        gates.append("provider_integration_gate")
    if any(keyword in text for keyword in ("sandbox", "container", "worker", "concurrent", "mcp", "autonomous", "execute")):
        gates.append("execution_boundary_gate")
    return sorted(set(gates))


def _select_context_budget(max_context_tokens: int) -> tuple[str, int, list[str]]:
    if max_context_tokens >= 6000:
        return "full", 6000, ["Full context is available within the requested budget."]
    if max_context_tokens >= 2500:
        return "excerpt", 2500, ["Context budget pressure: full context reduced to excerpts."]
    if max_context_tokens >= 800:
        return "summary", 800, ["Context budget pressure: excerpts reduced to summary context."]
    return "none", 0, ["Context budget pressure: context omitted until budget is increased."]


def _make_steps(
    task: PlanningTask,
    context_mode: str,
    execution_budget: int,
    approval_required: bool,
) -> tuple[PlannedStep, ...]:
    roles = ("planner", "executor", "verifier")
    if approval_required:
        roles = ("planner", "advisor", "executor", "verifier")
    weights = {"planner": 30, "advisor": 10, "executor": 40, "verifier": 20}
    total_weight = sum(weights[role] for role in roles)
    remaining = execution_budget
    steps: list[PlannedStep] = []
    for index, role in enumerate(roles):
        if index == len(roles) - 1:
            budget = remaining
        else:
            budget = execution_budget * weights[role] // total_weight if total_weight else 0
            remaining -= budget
        action = _action_for_role(role, task)
        steps.append(
            PlannedStep(
                step_id=f"{index + 1:02d}-{role}",
                role=role,
                action=action,
                token_budget=budget,
                context_mode=context_mode if role in {"planner", "advisor", "executor"} else _verifier_context_mode(context_mode),
                approval_required=approval_required,
                reason=_reason_for_role(role, approval_required),
            )
        )
    return tuple(steps)


def _action_for_role(role: str, task: PlanningTask) -> str:
    if role == "planner":
        return f"Decompose task '{task.task_id}' into reviewable work slices."
    if role == "advisor":
        return "Review risk, approval gates, and token budget tradeoffs."
    if role == "executor":
        return "Prepare an implementation outline for the selected slice without running commands."
    return "Verify plan invariants, blockers, and evidence requirements."


def _reason_for_role(role: str, approval_required: bool) -> str:
    if approval_required:
        return f"{role} step is planned only; human approval is required before any future action."
    return f"{role} step is planned only; MVP3 records plans without performing actions."


def _verifier_context_mode(context_mode: str) -> str:
    if context_mode == "none":
        return "none"
    return "summary"


def _required_string(data: dict[str, Any], key: str) -> str:
    value = data.get(key)
    if not isinstance(value, str) or not value.strip():
        raise PlanningInputError(f"{key} is required")
    return value.strip()


def _string_or_default(value: Any, default: str) -> str:
    if value is None:
        return default
    if not isinstance(value, str):
        raise PlanningInputError("string field must be a string")
    stripped = value.strip()
    return stripped or default


def _non_negative_int(value: Any, field_name: str) -> int:
    if not isinstance(value, int) or value < 0:
        raise PlanningInputError(f"{field_name} must be a non-negative integer")
    return value
