"""Provider-free Mission, Stage, and WorkCard contract.

This module is deliberately a pure wire-contract boundary.  It does not read
the repository, call GitHub or a Provider, start a process, persist state, or
grant authority.  The existing packet controller remains the only lifecycle
writer while this contract is introduced as a read-only compatibility layer.
"""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import PurePosixPath
from typing import Any


SCHEMA_VERSION = "maintenance_mission.v1"
STAGE_SCHEMA_VERSION = "maintenance_stage.v1"
WORKCARD_SCHEMA_VERSION = "maintenance_workcard.v1"
PROJECTION_SCHEMA_VERSION = "legacy_mission_projection.v1"

MAX_TEXT_CHARS = 8 * 1024
MAX_PATH_CHARS = 512
MAX_LIST_ITEMS = 100
MAX_SCOPE_PATHS = 50
MAX_BUDGET_ATTEMPTS = 10_000
MAX_BUDGET_SECONDS = 31 * 24 * 60 * 60
MAX_BUDGET_CALLS = 100_000
MAX_BUDGET_COST_MICROS = 10**12
MAX_GRANT_USES = 10_000

SHA40 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
REPOSITORY = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
IDENTIFIER = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")
PACKET_ID = re.compile(r"^[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+$")
BRANCH = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._/-]{0,199}$")
TIMESTAMP = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?Z$"
)

MISSION_STATES = frozenset(
    {
        "IDLE",
        "PROPOSING",
        "WAITING_APPROVAL",
        "RUNNING",
        "VERIFYING",
        "INTEGRATING",
        "REPLAN",
        "PAUSED_FOR_OWNER",
        "COMPLETE",
    }
)
MODEL_TIERS = frozenset({"T0", "T1", "T2", "T3"})
WORKCARD_RESULTS = frozenset(
    {"PENDING", "COMPLETE", "FAILED", "REPLAN_REQUIRED", "OUTCOME_UNKNOWN"}
)

SAFE_GRANT_TYPES = frozenset({"read_only", "repository_maintenance"})
SAFE_OPERATIONS = frozenset(
    {"read", "write", "test", "branch", "draft_pr", "review", "ci_repair"}
)
SAFE_CHANGE_TYPES = frozenset(
    {"documentation", "source", "tests", "configuration", "workflow"}
)
SENSITIVE_PATH_COMPONENTS = frozenset(
    {".git", ".codex", "secrets", "credentials", "private", "ssh"}
)
FORBIDDEN_GRANT_WORDS = frozenset(
    {
        "approval",
        "credential",
        "deploy",
        "destructive",
        "effect",
        "merge",
        "production",
        "provider",
        "release",
        "target",
    }
)

STOP_CATEGORIES = {
    "WORKER_FAILED": "ROUTINE_RECOVERY",
    "WORKER_TIMEOUT": "ROUTINE_RECOVERY",
    "TEST_FAILED": "ROUTINE_RECOVERY",
    "CI_FAILED": "ROUTINE_RECOVERY",
    "REVIEW_CHANGES_REQUESTED": "ROUTINE_RECOVERY",
    "MAIN_DRIFT": "ROUTINE_RECOVERY",
    "NO_CHANGE": "ROUTINE_RECOVERY",
    "SCOPE_EXCEEDED": "PAUSED_FOR_OWNER",
    "AUTHORITY_REQUIRED": "PAUSED_FOR_OWNER",
    "REQUIREMENT_CONFLICT": "PAUSED_FOR_OWNER",
    "EXTERNAL_OUTCOME_UNKNOWN": "PAUSED_FOR_OWNER",
    "SAFETY_CONFLICT": "PAUSED_FOR_OWNER",
}

LEGACY_LIFECYCLE_WRITER = "scripts/agent-control/local_loop.py"
CAMPAIGN_MISSION_ID = "AUTONOMOUS-STEWARD-MIGRATION-2026-08-27"
CAMPAIGN_REPOSITORY = "Igzela/token-efficient-agent-harness-lab"
CAMPAIGN_SOURCE_REF = "autonomous-steward-migration-plan-2026-08-27"
CAMPAIGN_SOURCE_SHA256 = "4b6eacaa4ff58337a02a6a73f458ffb0e4d3cb4e71f256c1024b3dd6205e1d39"
CAMPAIGN_BASE_SHA = "4dba4a9ccb4948775fd4ed7452ee6e419327aa46"


class MissionContractError(ValueError):
    """Raised when a Mission contract cannot be proved safe and current."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


def _canonical_json(value: object) -> str:
    try:
        return json.dumps(
            value,
            sort_keys=True,
            separators=(",", ":"),
            ensure_ascii=True,
            allow_nan=False,
        )
    except (TypeError, ValueError) as exc:
        raise MissionContractError("canonical_json_invalid") from exc


def json_sha256(value: object) -> str:
    """Hash one canonical JSON value without accepting NaN or infinity."""

    return hashlib.sha256(_canonical_json(value).encode("utf-8")).hexdigest()


def _mapping(value: object, fields: set[str], required: set[str], reason: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != fields or not required <= set(value):
        raise MissionContractError(reason)
    return value


def _text(value: object, field: str, *, max_chars: int = MAX_TEXT_CHARS) -> str:
    if not isinstance(value, str) or not value.strip() or len(value) > max_chars:
        raise MissionContractError(f"{field}_invalid")
    if any(ord(char) < 32 and char not in "\t\n" for char in value):
        raise MissionContractError(f"{field}_invalid")
    return value.strip()


def _identifier(value: object, field: str) -> str:
    result = _text(value, field, max_chars=128)
    if IDENTIFIER.fullmatch(result) is None:
        raise MissionContractError(f"{field}_invalid")
    return result


def _sha(value: object, field: str, pattern: re.Pattern[str] = SHA256) -> str:
    if not isinstance(value, str) or pattern.fullmatch(value) is None:
        raise MissionContractError(f"{field}_invalid")
    return value


def _strings(
    value: object,
    field: str,
    *,
    allow_empty: bool = False,
    max_items: int = MAX_LIST_ITEMS,
) -> tuple[str, ...]:
    if not isinstance(value, list) or len(value) > max_items:
        raise MissionContractError(f"{field}_invalid")
    result = tuple(_text(item, field) for item in value)
    if not allow_empty and not result:
        raise MissionContractError(f"{field}_invalid")
    if len(result) != len(set(result)):
        raise MissionContractError(f"{field}_duplicated")
    return result


def _path(value: object, field: str, *, allow_directory: bool = True) -> str:
    if not isinstance(value, str) or not value or len(value) > MAX_PATH_CHARS:
        raise MissionContractError(f"{field}_invalid")
    if "\\" in value or "\x00" in value or any(char.isspace() for char in value):
        raise MissionContractError(f"{field}_invalid")
    directory = value.endswith("/")
    candidate = value[:-1] if directory else value
    parsed = PurePosixPath(candidate)
    if (
        not candidate
        or parsed.is_absolute()
        or any(part in {"", ".", ".."} for part in parsed.parts)
        or str(parsed) != candidate
        or (directory and not allow_directory)
    ):
        raise MissionContractError(f"{field}_invalid")
    return candidate + ("/" if directory else "")


def _paths(value: object, field: str, *, allow_empty: bool = False) -> tuple[str, ...]:
    if not isinstance(value, list) or len(value) > MAX_SCOPE_PATHS:
        raise MissionContractError(f"{field}_invalid")
    result = tuple(sorted({_path(item, field) for item in value}))
    if not allow_empty and not result:
        raise MissionContractError(f"{field}_invalid")
    if len(result) != len(value):
        raise MissionContractError(f"{field}_duplicated")
    for item in result:
        components = item.rstrip("/").split("/")
        if any(
            component in SENSITIVE_PATH_COMPONENTS
            or component.startswith(".env")
            or component in {"id_rsa", "id_ed25519"}
            for component in components
        ):
            raise MissionContractError(f"{field}_sensitive")
    return result


def _bounded_int(value: object, field: str, *, minimum: int, maximum: int) -> int:
    if type(value) is not int or not minimum <= value <= maximum:
        raise MissionContractError(f"{field}_invalid")
    return value


def _contains(scope: str, candidate: str) -> bool:
    return scope == candidate or (
        scope.endswith("/")
        and (candidate == scope[:-1] or candidate.startswith(scope))
    )


def path_in_scope(scope: tuple[str, ...], candidate: str) -> bool:
    """Return whether a normalized repository path is covered by a scope."""

    normalized = _path(candidate, "candidate_path", allow_directory=False)
    return any(_contains(item, normalized) for item in scope)


@dataclass(frozen=True)
class RepositoryIdentity:
    repository: str
    base_sha: str
    branch: str
    source_ref: str
    source_sha256: str

    def to_wire(self) -> dict[str, str]:
        return {
            "repository": self.repository,
            "base_sha": self.base_sha,
            "branch": self.branch,
            "source_ref": self.source_ref,
            "source_sha256": self.source_sha256,
        }

    @classmethod
    def from_wire(cls, value: object) -> RepositoryIdentity:
        wire = _mapping(
            value,
            {"repository", "base_sha", "branch", "source_ref", "source_sha256"},
            {"repository", "base_sha", "branch", "source_ref", "source_sha256"},
            "repository_identity_fields_invalid",
        )
        repository = _text(wire["repository"], "repository", max_chars=255)
        if REPOSITORY.fullmatch(repository) is None:
            raise MissionContractError("repository_invalid")
        base_sha = _sha(wire["base_sha"], "base_sha", SHA40)
        branch = _text(wire["branch"], "branch", max_chars=200)
        if BRANCH.fullmatch(branch) is None or ".." in branch or branch.endswith("/"):
            raise MissionContractError("branch_invalid")
        source_ref = _text(wire["source_ref"], "source_ref", max_chars=255)
        if any(char in source_ref for char in "\r\n"):
            raise MissionContractError("source_ref_invalid")
        source_sha256 = _sha(wire["source_sha256"], "source_sha256")
        return cls(repository, base_sha, branch, source_ref, source_sha256)


@dataclass(frozen=True)
class OwnerApproval:
    owner_identity: str
    proposal_sha256: str
    approval_id: str
    approved_at: str

    def to_wire(self) -> dict[str, str]:
        return {
            "owner_identity": self.owner_identity,
            "proposal_sha256": self.proposal_sha256,
            "approval_id": self.approval_id,
            "approved_at": self.approved_at,
        }

    @classmethod
    def from_wire(cls, value: object) -> OwnerApproval:
        wire = _mapping(
            value,
            {"owner_identity", "proposal_sha256", "approval_id", "approved_at"},
            {"owner_identity", "proposal_sha256", "approval_id", "approved_at"},
            "owner_approval_fields_invalid",
        )
        owner = _text(wire["owner_identity"], "owner_identity", max_chars=255)
        if any(char in owner for char in "\r\n"):
            raise MissionContractError("owner_identity_invalid")
        proposal = _sha(wire["proposal_sha256"], "approval_proposal_sha256")
        approval_id = _identifier(wire["approval_id"], "approval_id")
        approved_at = _text(wire["approved_at"], "approved_at", max_chars=32)
        if TIMESTAMP.fullmatch(approved_at) is None:
            raise MissionContractError("approved_at_invalid")
        return cls(owner, proposal, approval_id, approved_at)


@dataclass(frozen=True)
class Budget:
    max_attempts: int
    max_retries: int
    max_runtime_seconds: int
    max_calls: int
    max_cost_micros: int
    max_external_effects: int

    def to_wire(self) -> dict[str, int]:
        return {
            "max_attempts": self.max_attempts,
            "max_retries": self.max_retries,
            "max_runtime_seconds": self.max_runtime_seconds,
            "max_calls": self.max_calls,
            "max_cost_micros": self.max_cost_micros,
            "max_external_effects": self.max_external_effects,
        }

    @classmethod
    def from_wire(cls, value: object) -> Budget:
        fields = {
            "max_attempts",
            "max_retries",
            "max_runtime_seconds",
            "max_calls",
            "max_cost_micros",
            "max_external_effects",
        }
        wire = _mapping(value, fields, fields, "budget_fields_invalid")
        attempts = _bounded_int(
            wire["max_attempts"], "max_attempts", minimum=1, maximum=MAX_BUDGET_ATTEMPTS
        )
        retries = _bounded_int(
            wire["max_retries"], "max_retries", minimum=0, maximum=attempts - 1
        )
        runtime = _bounded_int(
            wire["max_runtime_seconds"],
            "max_runtime_seconds",
            minimum=1,
            maximum=MAX_BUDGET_SECONDS,
        )
        calls = _bounded_int(
            wire["max_calls"], "max_calls", minimum=1, maximum=MAX_BUDGET_CALLS
        )
        cost = _bounded_int(
            wire["max_cost_micros"],
            "max_cost_micros",
            minimum=0,
            maximum=MAX_BUDGET_COST_MICROS,
        )
        effects = _bounded_int(
            wire["max_external_effects"],
            "max_external_effects",
            minimum=0,
            maximum=0,
        )
        return cls(attempts, retries, runtime, calls, cost, effects)


@dataclass(frozen=True)
class Grant:
    grant_id: str
    grant_type: str
    allowed_paths: tuple[str, ...]
    allowed_operations: tuple[str, ...]
    max_uses: int

    def to_wire(self) -> dict[str, Any]:
        return {
            "grant_id": self.grant_id,
            "grant_type": self.grant_type,
            "allowed_paths": list(self.allowed_paths),
            "allowed_operations": list(self.allowed_operations),
            "max_uses": self.max_uses,
        }

    @classmethod
    def from_wire(cls, value: object) -> Grant:
        fields = {"grant_id", "grant_type", "allowed_paths", "allowed_operations", "max_uses"}
        wire = _mapping(value, fields, fields, "grant_fields_invalid")
        grant_id = _identifier(wire["grant_id"], "grant_id")
        grant_type = _text(wire["grant_type"], "grant_type", max_chars=64)
        if grant_type not in SAFE_GRANT_TYPES:
            raise MissionContractError("grant_type_forbidden")
        paths = _paths(wire["allowed_paths"], "grant_allowed_paths")
        operations = _strings(wire["allowed_operations"], "grant_allowed_operations")
        if any(operation not in SAFE_OPERATIONS for operation in operations):
            raise MissionContractError("grant_operation_forbidden")
        if grant_type == "read_only" and set(operations) != {"read"}:
            raise MissionContractError("read_only_grant_writes")
        if any(word in grant_type.lower() for word in FORBIDDEN_GRANT_WORDS):
            raise MissionContractError("grant_type_forbidden")
        uses = _bounded_int(wire["max_uses"], "grant_max_uses", minimum=1, maximum=MAX_GRANT_USES)
        return cls(grant_id, grant_type, paths, operations, uses)


@dataclass(frozen=True)
class RollbackBoundary:
    strategy: str
    reference: str
    verification: tuple[str, ...]

    def to_wire(self) -> dict[str, Any]:
        return {
            "strategy": self.strategy,
            "reference": self.reference,
            "verification": list(self.verification),
        }

    @classmethod
    def from_wire(cls, value: object) -> RollbackBoundary:
        fields = {"strategy", "reference", "verification"}
        wire = _mapping(value, fields, fields, "rollback_fields_invalid")
        strategy = _text(wire["strategy"], "rollback_strategy", max_chars=64)
        if strategy not in {"git_revert", "restore_accepted_main", "document_restore"}:
            raise MissionContractError("rollback_strategy_invalid")
        reference = _text(wire["reference"], "rollback_reference", max_chars=512)
        if any(char in reference for char in "\r\n"):
            raise MissionContractError("rollback_reference_invalid")
        verification = _strings(wire["verification"], "rollback_verification")
        return cls(strategy, reference, verification)


@dataclass(frozen=True)
class StopRule:
    code: str
    category: str
    description: str

    def to_wire(self) -> dict[str, str]:
        return {"code": self.code, "category": self.category, "description": self.description}

    @classmethod
    def from_wire(cls, value: object) -> StopRule:
        fields = {"code", "category", "description"}
        wire = _mapping(value, fields, fields, "stop_rule_fields_invalid")
        code = _text(wire["code"], "stop_code", max_chars=64)
        category = _text(wire["category"], "stop_category", max_chars=64)
        expected = STOP_CATEGORIES.get(code)
        if expected is None or category != expected:
            raise MissionContractError("stop_taxonomy_invalid")
        return cls(code, category, _text(wire["description"], "stop_description"))


@dataclass(frozen=True)
class MaintenanceMission:
    mission_id: str
    state: str
    objective: str
    completion_conditions: tuple[str, ...]
    repository_identity: RepositoryIdentity
    allowed_paths: tuple[str, ...]
    allowed_change_types: tuple[str, ...]
    forbidden_changes: tuple[str, ...]
    standing_grants: tuple[Grant, ...]
    budget: Budget
    quality_checks: tuple[str, ...]
    stop_rules: tuple[StopRule, ...]
    rollback: RollbackBoundary
    proposal_sha256: str
    owner_approval: OwnerApproval

    def proposal_wire(self) -> dict[str, Any]:
        """Return the approved semantic payload, excluding approval metadata."""

        return {
            "schema_version": SCHEMA_VERSION,
            "mission_id": self.mission_id,
            "state": self.state,
            "objective": self.objective,
            "completion_conditions": list(self.completion_conditions),
            "repository_identity": self.repository_identity.to_wire(),
            "allowed_paths": list(self.allowed_paths),
            "allowed_change_types": list(self.allowed_change_types),
            "forbidden_changes": list(self.forbidden_changes),
            "standing_grants": [grant.to_wire() for grant in self.standing_grants],
            "budget": self.budget.to_wire(),
            "quality_checks": list(self.quality_checks),
            "stop_rules": [rule.to_wire() for rule in self.stop_rules],
            "rollback": self.rollback.to_wire(),
        }

    @property
    def computed_proposal_sha256(self) -> str:
        return json_sha256(self.proposal_wire())

    def to_wire(self) -> dict[str, Any]:
        return {
            **self.proposal_wire(),
            "proposal_sha256": self.proposal_sha256,
            "owner_approval": self.owner_approval.to_wire(),
        }

    @classmethod
    def from_wire(cls, value: object) -> MaintenanceMission:
        fields = {
            "schema_version",
            "mission_id",
            "state",
            "objective",
            "completion_conditions",
            "repository_identity",
            "allowed_paths",
            "allowed_change_types",
            "forbidden_changes",
            "standing_grants",
            "budget",
            "quality_checks",
            "stop_rules",
            "rollback",
            "proposal_sha256",
            "owner_approval",
        }
        wire = _mapping(value, fields, fields, "mission_fields_invalid")
        if wire["schema_version"] != SCHEMA_VERSION:
            raise MissionContractError("mission_schema_unsupported")
        mission_id = _identifier(wire["mission_id"], "mission_id")
        state = _text(wire["state"], "mission_state", max_chars=64)
        if state not in MISSION_STATES:
            raise MissionContractError("mission_state_invalid")
        objective = _text(wire["objective"], "mission_objective")
        completion = _strings(wire["completion_conditions"], "completion_conditions")
        identity = RepositoryIdentity.from_wire(wire["repository_identity"])
        allowed_paths = _paths(wire["allowed_paths"], "mission_allowed_paths")
        change_types = _strings(wire["allowed_change_types"], "allowed_change_types")
        if any(change_type not in SAFE_CHANGE_TYPES for change_type in change_types):
            raise MissionContractError("change_type_forbidden")
        forbidden = _strings(wire["forbidden_changes"], "forbidden_changes")
        raw_grants = wire["standing_grants"]
        if not isinstance(raw_grants, list) or not raw_grants or len(raw_grants) > MAX_LIST_ITEMS:
            raise MissionContractError("standing_grants_invalid")
        grants = tuple(Grant.from_wire(item) for item in raw_grants)
        if len({grant.grant_id for grant in grants}) != len(grants):
            raise MissionContractError("standing_grants_duplicated")
        budget = Budget.from_wire(wire["budget"])
        quality = _strings(wire["quality_checks"], "quality_checks")
        raw_stops = wire["stop_rules"]
        if not isinstance(raw_stops, list) or not raw_stops or len(raw_stops) > MAX_LIST_ITEMS:
            raise MissionContractError("stop_rules_invalid")
        stops = tuple(StopRule.from_wire(item) for item in raw_stops)
        if len({rule.code for rule in stops}) != len(stops):
            raise MissionContractError("stop_rules_duplicated")
        categories = {rule.category for rule in stops}
        if not {"ROUTINE_RECOVERY", "PAUSED_FOR_OWNER"} <= categories:
            raise MissionContractError("stop_taxonomy_incomplete")
        rollback = RollbackBoundary.from_wire(wire["rollback"])
        proposal_sha = _sha(wire["proposal_sha256"], "mission_proposal_sha256")
        approval = OwnerApproval.from_wire(wire["owner_approval"])
        model = cls(
            mission_id,
            state,
            objective,
            completion,
            identity,
            allowed_paths,
            change_types,
            forbidden,
            grants,
            budget,
            quality,
            stops,
            rollback,
            proposal_sha,
            approval,
        )
        if model.computed_proposal_sha256 != proposal_sha:
            raise MissionContractError("mission_proposal_digest_mismatch")
        if approval.proposal_sha256 != proposal_sha:
            raise MissionContractError("owner_approval_digest_mismatch")
        for grant in grants:
            if not all(path_in_scope(allowed_paths, path.rstrip("/")) for path in grant.allowed_paths):
                raise MissionContractError("grant_scope_widens_mission")
        return model


@dataclass(frozen=True)
class Stage:
    stage_id: str
    mission_id: str
    objective: str
    repository_identity: RepositoryIdentity
    acceptance_checks: tuple[str, ...]
    compatibility_checks: tuple[str, ...]
    workcard_ids: tuple[str, ...]
    rollback: RollbackBoundary
    integration_pr: int | None
    exact_head: str | None

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": STAGE_SCHEMA_VERSION,
            "stage_id": self.stage_id,
            "mission_id": self.mission_id,
            "objective": self.objective,
            "repository_identity": self.repository_identity.to_wire(),
            "acceptance_checks": list(self.acceptance_checks),
            "compatibility_checks": list(self.compatibility_checks),
            "workcard_ids": list(self.workcard_ids),
            "rollback": self.rollback.to_wire(),
            "integration_pr": self.integration_pr,
            "exact_head": self.exact_head,
        }

    @classmethod
    def from_wire(cls, value: object) -> Stage:
        fields = {
            "schema_version",
            "stage_id",
            "mission_id",
            "objective",
            "repository_identity",
            "acceptance_checks",
            "compatibility_checks",
            "workcard_ids",
            "rollback",
            "integration_pr",
            "exact_head",
        }
        wire = _mapping(value, fields, fields, "stage_fields_invalid")
        if wire["schema_version"] != STAGE_SCHEMA_VERSION:
            raise MissionContractError("stage_schema_unsupported")
        stage_id = _identifier(wire["stage_id"], "stage_id")
        mission_id = _identifier(wire["mission_id"], "mission_id")
        objective = _text(wire["objective"], "stage_objective")
        identity = RepositoryIdentity.from_wire(wire["repository_identity"])
        acceptance = _strings(wire["acceptance_checks"], "acceptance_checks")
        compatibility = _strings(wire["compatibility_checks"], "compatibility_checks")
        workcards = _strings(wire["workcard_ids"], "workcard_ids")
        rollback = RollbackBoundary.from_wire(wire["rollback"])
        pr = wire["integration_pr"]
        if pr is not None:
            pr = _bounded_int(pr, "integration_pr", minimum=1, maximum=10**9)
        head = wire["exact_head"]
        if head is not None:
            head = _sha(head, "exact_head", SHA40)
        if (pr is None) != (head is None):
            raise MissionContractError("stage_integration_binding_invalid")
        return cls(
            stage_id,
            mission_id,
            objective,
            identity,
            acceptance,
            compatibility,
            workcards,
            rollback,
            pr,
            head,
        )


@dataclass(frozen=True)
class WorkCard:
    card_id: str
    stage_id: str
    allowed_paths: tuple[str, ...]
    forbidden_paths: tuple[str, ...]
    steps: tuple[str, ...]
    focused_tests: tuple[str, ...]
    negative_checks: tuple[str, ...]
    expected_evidence: tuple[str, ...]
    dependencies: tuple[str, ...]
    path_locks: tuple[str, ...]
    max_attempts: int
    model_tier: str
    rollback: RollbackBoundary
    result_state: str

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": WORKCARD_SCHEMA_VERSION,
            "card_id": self.card_id,
            "stage_id": self.stage_id,
            "allowed_paths": list(self.allowed_paths),
            "forbidden_paths": list(self.forbidden_paths),
            "steps": list(self.steps),
            "focused_tests": list(self.focused_tests),
            "negative_checks": list(self.negative_checks),
            "expected_evidence": list(self.expected_evidence),
            "dependencies": list(self.dependencies),
            "path_locks": list(self.path_locks),
            "max_attempts": self.max_attempts,
            "model_tier": self.model_tier,
            "rollback": self.rollback.to_wire(),
            "result_state": self.result_state,
        }

    @classmethod
    def from_wire(cls, value: object) -> WorkCard:
        fields = {
            "schema_version",
            "card_id",
            "stage_id",
            "allowed_paths",
            "forbidden_paths",
            "steps",
            "focused_tests",
            "negative_checks",
            "expected_evidence",
            "dependencies",
            "path_locks",
            "max_attempts",
            "model_tier",
            "rollback",
            "result_state",
        }
        wire = _mapping(value, fields, fields, "workcard_fields_invalid")
        if wire["schema_version"] != WORKCARD_SCHEMA_VERSION:
            raise MissionContractError("workcard_schema_unsupported")
        card_id = _identifier(wire["card_id"], "card_id")
        stage_id = _identifier(wire["stage_id"], "stage_id")
        allowed = _paths(wire["allowed_paths"], "workcard_allowed_paths")
        forbidden = _paths(wire["forbidden_paths"], "workcard_forbidden_paths", allow_empty=True)
        if any(
            _contains(first, second) or _contains(second, first)
            for first in allowed
            for second in forbidden
        ):
            raise MissionContractError("workcard_forbidden_path_overlaps_scope")
        steps = _strings(wire["steps"], "workcard_steps")
        focused = _strings(wire["focused_tests"], "focused_tests")
        negative = _strings(wire["negative_checks"], "negative_checks")
        evidence = _strings(wire["expected_evidence"], "expected_evidence")
        dependencies = _strings(wire["dependencies"], "workcard_dependencies", allow_empty=True)
        locks = _paths(wire["path_locks"], "path_locks", allow_empty=True)
        if any(not any(_contains(scope, lock) for scope in allowed) for lock in locks):
            raise MissionContractError("path_lock_outside_scope")
        attempts = _bounded_int(
            wire["max_attempts"], "workcard_max_attempts", minimum=1, maximum=MAX_BUDGET_ATTEMPTS
        )
        tier = _text(wire["model_tier"], "model_tier", max_chars=8)
        if tier not in MODEL_TIERS or tier == "T3":
            raise MissionContractError("model_tier_invalid")
        rollback = RollbackBoundary.from_wire(wire["rollback"])
        result = _text(wire["result_state"], "workcard_result_state", max_chars=32)
        if result not in WORKCARD_RESULTS:
            raise MissionContractError("workcard_result_state_invalid")
        return cls(
            card_id,
            stage_id,
            allowed,
            forbidden,
            steps,
            focused,
            negative,
            evidence,
            dependencies,
            locks,
            attempts,
            tier,
            rollback,
            result,
        )


@dataclass(frozen=True)
class LegacyMissionProjection:
    schema_version: str
    mission_id: str
    packet_id: str
    dispatch_lane: str
    allowed_paths: tuple[str, ...]
    writes_lifecycle: bool
    execution_authorized: bool
    authority_consumption_allowed: bool
    external_effect_limit: int
    lifecycle_writer: str

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "mission_id": self.mission_id,
            "packet_id": self.packet_id,
            "dispatch_lane": self.dispatch_lane,
            "allowed_paths": list(self.allowed_paths),
            "writes_lifecycle": self.writes_lifecycle,
            "execution_authorized": self.execution_authorized,
            "authority_consumption_allowed": self.authority_consumption_allowed,
            "external_effect_limit": self.external_effect_limit,
            "lifecycle_writer": self.lifecycle_writer,
        }


def validate_stage(stage: Stage, mission: MaintenanceMission, cards: tuple[WorkCard, ...] = ()) -> Stage:
    """Validate Stage identity, scope, card graph, and integration binding."""

    model = Stage.from_wire(stage.to_wire())
    if model.mission_id != mission.mission_id:
        raise MissionContractError("stage_mission_binding_invalid")
    if model.repository_identity != mission.repository_identity:
        raise MissionContractError("stage_repository_identity_invalid")
    if model.rollback != mission.rollback:
        raise MissionContractError("stage_rollback_widens_mission")
    if len(set(model.workcard_ids)) != len(model.workcard_ids):
        raise MissionContractError("stage_workcard_ids_duplicated")
    if cards:
        if {card.card_id for card in cards} != set(model.workcard_ids):
            raise MissionContractError("stage_workcard_graph_invalid")
        card_ids = set(model.workcard_ids)
        for card in cards:
            if any(dependency not in card_ids for dependency in card.dependencies):
                raise MissionContractError("workcard_dependency_unknown")
        graph = {card.card_id: set(card.dependencies) for card in cards}
        visiting: set[str] = set()
        visited: set[str] = set()

        def visit(card_id: str) -> None:
            if card_id in visiting:
                raise MissionContractError("workcard_dependency_cycle")
            if card_id in visited:
                return
            visiting.add(card_id)
            for dependency in graph[card_id]:
                visit(dependency)
            visiting.remove(card_id)
            visited.add(card_id)

        for card_id in graph:
            visit(card_id)
        for card in cards:
            validate_workcard(card, model, mission)
    return model


def validate_workcard(card: WorkCard, stage: Stage, mission: MaintenanceMission) -> WorkCard:
    model = WorkCard.from_wire(card.to_wire())
    validate_stage(stage, mission)
    if model.stage_id != stage.stage_id:
        raise MissionContractError("workcard_stage_binding_invalid")
    if model.card_id not in stage.workcard_ids:
        raise MissionContractError("workcard_not_in_stage")
    if model.rollback != stage.rollback:
        raise MissionContractError("workcard_rollback_widens_stage")
    if model.max_attempts > mission.budget.max_attempts:
        raise MissionContractError("workcard_budget_exceeded")
    if any(not path_in_scope(mission.allowed_paths, path.rstrip("/")) for path in model.allowed_paths):
        raise MissionContractError("workcard_scope_widens_mission")
    return model


def validate_owner_approval(
    mission: MaintenanceMission,
    *,
    authorized_owner_identities: tuple[str, ...],
) -> MaintenanceMission:
    """Require a trusted caller to bind the wire approval to an owner identity.

    The wire value identifies the claimed approver; the allowlist is supplied
    by the existing authority owner and is intentionally not part of the
    untrusted Mission payload.
    """

    model = MaintenanceMission.from_wire(mission.to_wire())
    if (
        not isinstance(authorized_owner_identities, tuple)
        or not authorized_owner_identities
        or any(not isinstance(identity, str) or not identity for identity in authorized_owner_identities)
        or model.owner_approval.owner_identity not in authorized_owner_identities
    ):
        raise MissionContractError("owner_approval_identity_untrusted")
    return model


def validate_current_mission(
    mission: MaintenanceMission,
    *,
    repository: str,
    base_sha: str,
    branch: str,
    source_ref: str,
    source_sha256: str,
    authorized_owner_identities: tuple[str, ...],
) -> MaintenanceMission:
    """Reject a valid-looking Mission bound to stale or unauthenticated identity."""

    model = validate_owner_approval(
        mission,
        authorized_owner_identities=authorized_owner_identities,
    )
    if model.repository_identity.repository != repository:
        raise MissionContractError("mission_repository_stale")
    if model.repository_identity.base_sha != base_sha:
        raise MissionContractError("mission_base_sha_stale")
    if model.repository_identity.branch != branch:
        raise MissionContractError("mission_branch_stale")
    if model.repository_identity.source_ref != source_ref:
        raise MissionContractError("mission_source_ref_stale")
    if model.repository_identity.source_sha256 != source_sha256:
        raise MissionContractError("mission_source_stale")
    return model


def validate_registered_campaign() -> MaintenanceMission:
    """Validate the one statically registered campaign against trusted inputs."""

    mission = campaign_mission()
    return validate_current_mission(
        mission,
        repository=CAMPAIGN_REPOSITORY,
        base_sha=CAMPAIGN_BASE_SHA,
        branch="main",
        source_ref=CAMPAIGN_SOURCE_REF,
        source_sha256=CAMPAIGN_SOURCE_SHA256,
        authorized_owner_identities=("repository-owner",),
    )


def stop_category(code: str) -> str:
    """Return the fixed routine-recovery or owner-pause category."""

    if not isinstance(code, str) or code not in STOP_CATEGORIES:
        raise MissionContractError("stop_code_unknown")
    return STOP_CATEGORIES[code]


def campaign_mission() -> MaintenanceMission:
    """Return the one registered, provider-free migration Mission contract."""

    identity = RepositoryIdentity(
        CAMPAIGN_REPOSITORY,
        CAMPAIGN_BASE_SHA,
        "main",
        CAMPAIGN_SOURCE_REF,
        CAMPAIGN_SOURCE_SHA256,
    )
    budget = Budget(
        max_attempts=32,
        max_retries=31,
        max_runtime_seconds=7 * 24 * 60 * 60,
        max_calls=10_000,
        max_cost_micros=0,
        max_external_effects=0,
    )
    grants = (
        Grant(
            "migration-contract-read-write",
            "repository_maintenance",
            ("docs/", "engine/", "scripts/", "tests/"),
            ("read", "write", "test", "branch", "draft_pr", "review", "ci_repair"),
            32,
        ),
    )
    stops = tuple(
        StopRule(code, STOP_CATEGORIES[code], description)
        for code, description in (
            ("WORKER_FAILED", "Repair ordinary bounded worker failures within the Mission budget."),
            ("WORKER_TIMEOUT", "Recover an ordinary bounded timeout without changing authority."),
            ("TEST_FAILED", "Repair focused or regression test failures within the current Stage."),
            ("CI_FAILED", "Repair a failed canonical check while the exact head remains bound."),
            ("REVIEW_CHANGES_REQUESTED", "Apply bounded independent-review repairs before acceptance."),
            ("MAIN_DRIFT", "Reconcile accepted main drift before continuing the current work."),
            ("SCOPE_EXCEEDED", "Pause for owner approval when requested work exceeds Mission scope."),
            ("AUTHORITY_REQUIRED", "Pause when a new authority, budget, or external target is required."),
            ("REQUIREMENT_CONFLICT", "Pause when acceptance criteria cannot resolve incompatible directions."),
            ("EXTERNAL_OUTCOME_UNKNOWN", "Pause and reconcile any unknown external-operation result."),
            ("SAFETY_CONFLICT", "Pause when evidence or safety boundaries contradict one another."),
        )
    )
    candidate = MaintenanceMission(
        CAMPAIGN_MISSION_ID,
        "IDLE",
        "Provider-free repository maintenance migration with exact evidence and rollback boundaries.",
        (
            "Each accepted migration Stage has focused and full verification.",
            "Every accepted change retains exact-head review, CI, and rollback evidence.",
            "No second lifecycle writer or external-effect owner is activated.",
        ),
        identity,
        ("docs/", "engine/", "scripts/", "tests/"),
        ("documentation", "source", "tests", "configuration"),
        (
            "provider calls and credentials",
            "product, target, release, or deployment effects",
            "destructive operations and a second lifecycle writer",
        ),
        grants,
        budget,
        (
            "exact-head independent review",
            "canonical CI and security baseline",
            "rollback and repository-scope verification",
        ),
        stops,
        RollbackBoundary(
            "restore_accepted_main",
            "accepted PR0-only main and retained PR0 receipts",
            ("Revert the bounded contract change.", "Re-run the accepted PR0 verification baseline."),
        ),
        "0" * 64,
        OwnerApproval("repository-owner", "0" * 64, "migration-owner-approval-2026-08-27", "2026-08-27T00:00:00Z"),
    )
    digest = candidate.computed_proposal_sha256
    return MaintenanceMission(
        **{
            **candidate.__dict__,
            "proposal_sha256": digest,
            "owner_approval": OwnerApproval(
                "repository-owner",
                digest,
                "migration-owner-approval-2026-08-27",
                "2026-08-27T00:00:00Z",
            ),
        }
    )


def validate_legacy_compatibility(packet: object, capsule: object) -> LegacyMissionProjection:
    """Read a legacy packet/capsule without granting or consuming authority."""

    if not isinstance(packet, dict) or not isinstance(capsule, dict):
        raise MissionContractError("legacy_projection_input_invalid")
    registered = validate_registered_campaign()
    packet_fields = {
        "packet_id",
        "state",
        "source_path",
        "packet_sha256",
        "allowed_paths",
        "forbidden_next_actions",
        "execution_authorized",
        "checkpoint_allowed",
        "dispatch_lane",
    }
    if not set(packet) <= packet_fields:
        raise MissionContractError("legacy_packet_fields_invalid")
    packet_id = _identifier(packet.get("packet_id"), "legacy_packet_id")
    if PACKET_ID.fullmatch(packet_id) is None:
        raise MissionContractError("legacy_packet_id_invalid")
    _text(packet.get("state"), "legacy_packet_state", max_chars=64)
    _path(packet.get("source_path"), "legacy_packet_source", allow_directory=False)
    _sha(packet.get("packet_sha256"), "legacy_packet_sha256")
    packet_paths = _paths(packet.get("allowed_paths"), "legacy_packet_allowed_paths", allow_empty=True)
    forbidden = _strings(packet.get("forbidden_next_actions", []), "legacy_forbidden_next_actions", allow_empty=True)
    if not isinstance(packet.get("execution_authorized", False), bool):
        raise MissionContractError("legacy_execution_authority_invalid")
    if packet.get("checkpoint_allowed") is not True and packet.get("checkpoint_allowed") is not False:
        raise MissionContractError("legacy_checkpoint_authority_invalid")
    dispatch_lane = _text(packet.get("dispatch_lane"), "legacy_dispatch_lane", max_chars=128)
    required_capsule_fields = {
        "schema_version",
        "packet_id",
        "dispatch_lane",
        "external_effect_limit",
        "authority_consumption_allowed",
        "secret_values_allowed",
        "private_paths_allowed",
        "allowed_paths",
        "forbidden_next_actions",
    }
    allowed_capsule_fields = required_capsule_fields | {
        "accepted_binding_source",
        "allowed_outputs",
        "expected_artifacts",
        "forbidden_changes",
        "goal",
        "known_store_mutations",
        "ordered_steps",
        "packet_state",
        "pause_gates",
        "plan_lane_state",
        "prerequisite_receipts",
        "prerequisites",
        "read_paths",
        "rollback",
        "risk_class",
        "route_manifest_sha256",
        "t3_request_digest",
        "verification",
        "verification_family",
        "worker_tier",
        "promotion_evidence_sha256",
    }
    if not set(capsule) <= allowed_capsule_fields:
        raise MissionContractError("legacy_capsule_fields_invalid")
    if not required_capsule_fields <= set(capsule):
        raise MissionContractError("legacy_capsule_fields_invalid")
    if capsule.get("schema_version") != "weak_agent_dispatch.v1":
        raise MissionContractError("legacy_capsule_schema_invalid")
    if capsule.get("packet_id") != packet_id or capsule.get("dispatch_lane") != dispatch_lane:
        raise MissionContractError("legacy_dispatch_binding_invalid")
    if capsule.get("external_effect_limit") != 0:
        raise MissionContractError("legacy_external_effect_granted")
    if capsule.get("authority_consumption_allowed") is not False:
        raise MissionContractError("legacy_authority_consumption_granted")
    if capsule.get("secret_values_allowed") is not False or capsule.get("private_paths_allowed") is not False:
        raise MissionContractError("legacy_sensitive_surface_granted")
    capsule_paths = _paths(capsule.get("allowed_paths"), "legacy_capsule_allowed_paths")
    capsule_forbidden = _strings(
        capsule.get("forbidden_next_actions"), "legacy_capsule_forbidden_next_actions", allow_empty=True
    )
    if capsule_paths != packet_paths or capsule_forbidden != forbidden:
        raise MissionContractError("legacy_dispatch_scope_mismatch")
    if any(
        not path_in_scope(registered.allowed_paths, path.rstrip("/"))
        for path in packet_paths
    ):
        raise MissionContractError("legacy_scope_widens_registered_mission")
    mutations = capsule.get("known_store_mutations")
    if mutations is not None:
        if not isinstance(mutations, list) or mutations:
            raise MissionContractError("legacy_store_mutation_granted")
    outputs = capsule.get("allowed_outputs")
    if outputs is not None:
        safe_outputs = _strings(outputs, "legacy_allowed_outputs")
        blocked_phrases = (
            "provider secret",
            "credential",
            "target write",
            "production",
            "deployment",
            "destructive",
            "external effect",
        )
        if any(
            phrase in item.lower()
            for item in safe_outputs
            for phrase in blocked_phrases
        ):
            raise MissionContractError("legacy_output_surface_forbidden")
    return LegacyMissionProjection(
        PROJECTION_SCHEMA_VERSION,
        f"legacy-packet:{packet_id}",
        packet_id,
        dispatch_lane,
        packet_paths,
        False,
        False,
        False,
        0,
        LEGACY_LIFECYCLE_WRITER,
    )


__all__ = [
    "Budget",
    "Grant",
    "LegacyMissionProjection",
    "MaintenanceMission",
    "MissionContractError",
    "OwnerApproval",
    "RepositoryIdentity",
    "RollbackBoundary",
    "Stage",
    "StopRule",
    "WorkCard",
    "campaign_mission",
    "json_sha256",
    "path_in_scope",
    "stop_category",
    "validate_current_mission",
    "validate_legacy_compatibility",
    "validate_owner_approval",
    "validate_registered_campaign",
    "validate_stage",
    "validate_workcard",
]
