"""Provider-free Mission, Stage, and WorkCard contract.

This module is deliberately a pure wire-contract boundary.  It does not read
the repository, call GitHub or a Provider, start a process, persist state, or
grant authority.  The existing packet controller remains the only lifecycle
writer while this contract is introduced as a read-only compatibility layer.
"""

from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
import re
import secrets
from dataclasses import dataclass, field, replace
from pathlib import PurePosixPath
from typing import Any


def _now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


SCHEMA_VERSION = "maintenance_mission.v1"
STAGE_SCHEMA_VERSION = "maintenance_stage.v1"
WORKCARD_SCHEMA_VERSION = "maintenance_workcard.v1"
PROJECTION_SCHEMA_VERSION = "legacy_mission_projection.v1"
ACCEPTANCE_LEDGER_SCHEMA_VERSION = "mission_acceptance_ledger.v1"
ACCEPTANCE_OBLIGATION_SCHEMA_VERSION = "mission_acceptance_obligation.v1"

SCIENTIFIC_TERMINAL_DISPOSITIONS = frozenset(
    {
        "COMPLETE",
        "GO",
        "NO_GO",
        "INSUFFICIENT",
        "INCOMPARABLE",
        "REJECT",
        "SATURATED",
        "TRANSFER_FAILED",
        "REPLICATION_FAILED",
        "SUPERSEDED_BY_ACCEPTED_EVIDENCE",
        "NOT_JUSTIFIED_BY_PRECEDING_GATE",
    }
)

NON_TERMINAL_OPERATIONAL_STATES = frozenset(
    {
        "BLOCKED_AUTHORITY",
        "WAITING",
        "PROPOSING",
        "MISSING_EVIDENCE",
        "STAGE_EXHAUSTION",
        "LACK_OF_PROVIDER_EXECUTION",
        "OUTCOME_UNKNOWN",
        "IN_PROGRESS",
    }
)

GATE_HALTING_DISPOSITIONS = frozenset(
    {
        "NO_GO",
        "REJECT",
        "INSUFFICIENT",
        "INCOMPARABLE",
        "TRANSFER_FAILED",
        "REPLICATION_FAILED",
        "NOT_JUSTIFIED_BY_PRECEDING_GATE",
    }
)

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
ACCEPTED_MAIN_REFERENCE = re.compile(r"^accepted-main:[0-9a-f]{40}$")
GIT_REVERT_REFERENCE = re.compile(r"^revert:[0-9a-f]{40}$")
DOCUMENT_RESTORE_REFERENCE = re.compile(
    r"^document:[A-Za-z0-9_.//-]{1,512}:[0-9a-f]{64}$"
)
TIMESTAMP = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?Z$"
)
_ACTIVE_ACTIVATION_NONCES: set[str] = set()

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
    {
        "read",
        "write",
        "test",
        "branch",
        "draft_pr",
        "review",
        "ci_repair",
        "quarantine_exact_owned_candidate",
    }
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

LIFECYCLE_COORDINATOR = "scripts/agent-control/steward.py"
# ``repository-owner`` is a proposal placeholder only.  It is deliberately
# not an authenticated identity: a RUNNING Mission must carry the identity
# attested by the GitHub Issue-comment transport.
PROPOSAL_OWNER_IDENTITY = "repository-owner"
AUTHENTICATED_OWNER_IDENTITIES = ("github:Igzela",)
TRUSTED_OWNER_IDENTITIES = (PROPOSAL_OWNER_IDENTITY, *AUTHENTICATED_OWNER_IDENTITIES)
CAMPAIGN_MISSION_ID = "AUTONOMOUS-STEWARD-MIGRATION-2026-08-27"
CAMPAIGN_REPOSITORY = "Igzela/token-efficient-agent-harness-lab"
CAMPAIGN_SOURCE_REF = "autonomous-steward-migration-plan-2026-08-27"
CAMPAIGN_SOURCE_SHA256 = "4b6eacaa4ff58337a02a6a73f458ffb0e4d3cb4e71f256c1024b3dd6205e1d39"
CAMPAIGN_BASE_SHA = "7f0e5afd22a9441073e1ac71d981dfc74060a948"
CAMPAIGN_ALLOWED_PATHS = (
    "AGENTS.md",
    "README.md",
    "START_HERE.md",
    "docs/",
    "engine/src/context_working_set.rs",
    "engine/src/storage/local_product_store/",
    "scripts/agent-control/",
    "scripts/check_agent_handoff.py",
    "scripts/project_context.py",
    "scripts/session_context.py",
    "tests/",
    "tools/",
)
LEGACY_COMPATIBILITY_PATHS = ("AGENTS.md", "docs/", "engine/", "scripts/", "tests/")


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
    if not isinstance(value, dict) or not (set(value) <= fields) or not (required <= set(value)):
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
    if scope == candidate:
        return True
    if scope.endswith("/"):
        return candidate == scope[:-1] or candidate.startswith(scope)
    basename = scope.rsplit("/", 1)[-1]
    if "." not in basename:
        return candidate.startswith(f"{scope}/")
    return False


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

    def proposal_wire(self) -> dict[str, str]:
        """Return the immutable source/repository proposal identity.

        The accepted-main SHA is an activation binding, not an approval
        semantic.  Keeping it out of the proposal digest lets the registered
        Mission be re-bound to the freshly verified accepted main without
        changing the owner-approved objective or scope.
        """

        return {
            "repository": self.repository,
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
        reference_patterns = {
            "git_revert": GIT_REVERT_REFERENCE,
            "restore_accepted_main": ACCEPTED_MAIN_REFERENCE,
            "document_restore": DOCUMENT_RESTORE_REFERENCE,
        }
        if reference_patterns[strategy].fullmatch(reference) is None:
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
class AcceptanceObligation:
    obligation_id: str
    description: str
    category: str
    dependencies: tuple[str, ...] = ()
    required_paths: tuple[str, ...] = ()
    disposition: str | None = None
    evidence: dict[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        object.__setattr__(self, "required_paths", tuple(sorted(self.required_paths)))

    def proposal_wire(self) -> dict[str, Any]:
        return {
            "schema_version": ACCEPTANCE_OBLIGATION_SCHEMA_VERSION,
            "obligation_id": self.obligation_id,
            "description": self.description,
            "category": self.category,
            "dependencies": list(self.dependencies),
            "required_paths": list(self.required_paths),
        }

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": ACCEPTANCE_OBLIGATION_SCHEMA_VERSION,
            "obligation_id": self.obligation_id,
            "description": self.description,
            "category": self.category,
            "dependencies": list(self.dependencies),
            "required_paths": list(self.required_paths),
            "disposition": self.disposition,
            "evidence": dict(self.evidence),
        }

    @classmethod
    def from_wire(cls, value: object) -> AcceptanceObligation:
        fields = {
            "schema_version",
            "obligation_id",
            "description",
            "category",
            "dependencies",
            "required_paths",
            "disposition",
            "evidence",
        }
        wire = _mapping(value, fields, fields - {"required_paths", "disposition", "evidence"}, "obligation_fields_invalid")
        if wire["schema_version"] != ACCEPTANCE_OBLIGATION_SCHEMA_VERSION:
            raise MissionContractError("obligation_schema_unsupported")
        obligation_id = _identifier(wire["obligation_id"], "obligation_id")
        description = _text(wire["description"], "obligation_description")
        category = _text(wire["category"], "obligation_category", max_chars=64)
        dependencies = _strings(wire.get("dependencies", []), "obligation_dependencies", allow_empty=True)
        required_paths = _paths(wire.get("required_paths", []), "obligation_required_paths", allow_empty=True)
        disposition = wire.get("disposition")
        if disposition is not None:
            disposition = _text(disposition, "obligation_disposition", max_chars=64)
            if disposition in NON_TERMINAL_OPERATIONAL_STATES:
                raise MissionContractError("operational_state_not_scientific_terminal")
            if disposition not in SCIENTIFIC_TERMINAL_DISPOSITIONS:
                raise MissionContractError("obligation_disposition_invalid")
        evidence = wire.get("evidence")
        if evidence is not None and not isinstance(evidence, dict):
            raise MissionContractError("obligation_evidence_invalid")
        return cls(
            obligation_id=obligation_id,
            description=description,
            category=category,
            dependencies=tuple(dependencies),
            required_paths=tuple(required_paths),
            disposition=disposition,
            evidence=dict(evidence or {}),
        )


@dataclass(frozen=True)
class MissionAcceptanceLedger:
    obligations: tuple[AcceptanceObligation, ...]

    def proposal_wire(self) -> dict[str, Any]:
        return {
            "schema_version": ACCEPTANCE_LEDGER_SCHEMA_VERSION,
            "obligations": [ob.proposal_wire() for ob in self.obligations],
        }

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": ACCEPTANCE_LEDGER_SCHEMA_VERSION,
            "obligations": [ob.to_wire() for ob in self.obligations],
        }

    @classmethod
    def from_wire(cls, value: object) -> MissionAcceptanceLedger:
        if not isinstance(value, dict):
            raise MissionContractError("acceptance_ledger_invalid")
        if value.get("schema_version") != ACCEPTANCE_LEDGER_SCHEMA_VERSION:
            raise MissionContractError("acceptance_ledger_schema_unsupported")
        raw_obs = value.get("obligations")
        if not isinstance(raw_obs, list) or not raw_obs:
            raise MissionContractError("acceptance_ledger_empty")
        obligations = tuple(AcceptanceObligation.from_wire(item) for item in raw_obs)
        ledger = cls(obligations)
        ledger.validate()
        return ledger

    def validate(self) -> None:
        ids = [ob.obligation_id for ob in self.obligations]
        if len(set(ids)) != len(ids):
            raise MissionContractError("acceptance_ledger_duplicate_obligation")
        id_set = set(ids)
        for ob in self.obligations:
            for dep in ob.dependencies:
                if dep not in id_set:
                    raise MissionContractError("obligation_dependency_missing")
                if dep == ob.obligation_id:
                    raise MissionContractError("obligation_self_dependency")
        visited: dict[str, int] = {}
        def visit(node_id: str) -> None:
            if visited.get(node_id) == 0:
                raise MissionContractError("obligation_dependency_cycle")
            if visited.get(node_id) == 1:
                return
            visited[node_id] = 0
            ob = next(o for o in self.obligations if o.obligation_id == node_id)
            for dep in ob.dependencies:
                visit(dep)
            visited[node_id] = 1

        for ob in self.obligations:
            visit(ob.obligation_id)

    def is_terminal(self) -> bool:
        return all(ob.disposition in SCIENTIFIC_TERMINAL_DISPOSITIONS for ob in self.obligations)

    def unresolved_eligible(self) -> tuple[AcceptanceObligation, ...]:
        terminal_ids = {ob.obligation_id for ob in self.obligations if ob.disposition in SCIENTIFIC_TERMINAL_DISPOSITIONS}
        eligible = []
        for ob in self.obligations:
            if ob.disposition is None:
                if all(dep in terminal_ids for dep in ob.dependencies):
                    eligible.append(ob)
        return tuple(eligible)

    def get(self, obligation_id: str) -> AcceptanceObligation | None:
        for ob in self.obligations:
            if ob.obligation_id == obligation_id:
                return ob
        return None

    def disposition_obligation(
        self,
        obligation_id: str,
        disposition: str,
        evidence: dict[str, Any] | None = None,
    ) -> MissionAcceptanceLedger:
        if disposition in NON_TERMINAL_OPERATIONAL_STATES:
            raise MissionContractError("operational_state_not_scientific_terminal")
        if disposition not in SCIENTIFIC_TERMINAL_DISPOSITIONS:
            raise MissionContractError("obligation_disposition_invalid")

        existing = self.get(obligation_id)
        if existing is None:
            raise MissionContractError("obligation_not_found")

        if disposition == "NOT_JUSTIFIED_BY_PRECEDING_GATE":
            has_halting_upstream = False
            for dep_id in existing.dependencies:
                dep_ob = self.get(dep_id)
                if dep_ob is not None and dep_ob.disposition in GATE_HALTING_DISPOSITIONS:
                    has_halting_upstream = True
                    break
            if not has_halting_upstream:
                raise MissionContractError("not_justified_requires_halting_preceding_gate")

        new_evidence = dict(evidence or {})
        if existing.disposition is not None:
            if existing.disposition == disposition and existing.evidence == new_evidence:
                return self
            raise MissionContractError("contradictory_obligation_evidence")

        updated_obs = []
        for ob in self.obligations:
            if ob.obligation_id == obligation_id:
                updated_obs.append(replace(ob, disposition=disposition, evidence=new_evidence))
            else:
                updated_obs.append(ob)
        new_ledger = MissionAcceptanceLedger(tuple(updated_obs))
        new_ledger.validate()
        return new_ledger


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
    acceptance_ledger: MissionAcceptanceLedger | None = None
    predecessor_mission_id: str | None = None
    # Runtime-only proof produced by activate_current_mission.  It is
    # deliberately absent from the wire contract and therefore cannot be
    # forged by changing a serialized lifecycle state.
    _activation_nonce: str | None = field(default=None, repr=False, compare=False)

    def proposal_wire(self) -> dict[str, Any]:
        """Return the approved semantic payload, excluding approval metadata."""

        wire = {
            "schema_version": SCHEMA_VERSION,
            "mission_id": self.mission_id,
            "objective": self.objective,
            "completion_conditions": list(self.completion_conditions),
            "repository_identity": self.repository_identity.proposal_wire(),
            "allowed_paths": list(self.allowed_paths),
            "allowed_change_types": list(self.allowed_change_types),
            "forbidden_changes": list(self.forbidden_changes),
            "standing_grants": [grant.to_wire() for grant in self.standing_grants],
            "budget": self.budget.to_wire(),
            "quality_checks": list(self.quality_checks),
            "stop_rules": [rule.to_wire() for rule in self.stop_rules],
            "rollback": self.rollback.to_wire(),
        }
        if self.acceptance_ledger is not None:
            wire["acceptance_ledger"] = self.acceptance_ledger.proposal_wire()
        if self.predecessor_mission_id is not None:
            wire["predecessor_mission_id"] = self.predecessor_mission_id
        return wire

    @property
    def computed_proposal_sha256(self) -> str:
        return json_sha256(self.proposal_wire())

    def to_wire(self) -> dict[str, Any]:
        return {
            **self.proposal_wire(),
            "state": self.state,
            "repository_identity": self.repository_identity.to_wire(),
            "proposal_sha256": self.proposal_sha256,
            "owner_approval": self.owner_approval.to_wire(),
        }

    @classmethod
    def from_wire(cls, value: object) -> MaintenanceMission:
        required_fields = {
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
        allowed_fields = required_fields | {"acceptance_ledger", "predecessor_mission_id"}
        wire = _mapping(value, allowed_fields, required_fields, "mission_fields_invalid")
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
        if approval.owner_identity not in TRUSTED_OWNER_IDENTITIES:
            raise MissionContractError("owner_approval_identity_untrusted")
        acceptance_ledger = None
        if "acceptance_ledger" in wire and wire["acceptance_ledger"] is not None:
            acceptance_ledger = MissionAcceptanceLedger.from_wire(wire["acceptance_ledger"])
        predecessor_mission_id = None
        if "predecessor_mission_id" in wire and wire["predecessor_mission_id"] is not None:
            predecessor_mission_id = _identifier(wire["predecessor_mission_id"], "predecessor_mission_id")
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
            acceptance_ledger,
            predecessor_mission_id,
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


def _validate_stage_identity(stage: Stage, mission: MaintenanceMission) -> Stage:
    model = Stage.from_wire(stage.to_wire())
    if model.mission_id != mission.mission_id:
        raise MissionContractError("stage_mission_binding_invalid")
    if model.repository_identity != mission.repository_identity:
        raise MissionContractError("stage_repository_identity_invalid")
    if model.rollback != mission.rollback:
        raise MissionContractError("stage_rollback_widens_mission")
    return model


def validate_stage(
    stage: Stage,
    mission: MaintenanceMission,
    cards: tuple[WorkCard, ...] = (),
    *,
    observed_integration_pr: int | None = None,
    observed_exact_head: str | None = None,
) -> Stage:
    """Validate Stage identity, observed PR/head, and its complete card graph."""

    model = _validate_stage_identity(stage, mission)
    if model.integration_pr is not None:
        if (
            model.integration_pr != observed_integration_pr
            or model.exact_head != observed_exact_head
        ):
            raise MissionContractError("stage_exact_head_mismatch")
    elif observed_integration_pr is not None or observed_exact_head is not None:
        raise MissionContractError("stage_integration_binding_invalid")
    if not isinstance(cards, tuple) or not cards:
        raise MissionContractError("stage_workcard_graph_incomplete")
    if len(set(model.workcard_ids)) != len(model.workcard_ids):
        raise MissionContractError("stage_workcard_ids_duplicated")
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
    stage_model = _validate_stage_identity(stage, mission)
    if model.stage_id != stage_model.stage_id:
        raise MissionContractError("workcard_stage_binding_invalid")
    if model.card_id not in stage_model.workcard_ids:
        raise MissionContractError("workcard_not_in_stage")
    if model.rollback != stage_model.rollback:
        raise MissionContractError("workcard_rollback_widens_stage")
    if model.max_attempts > mission.budget.max_attempts:
        raise MissionContractError("workcard_budget_exceeded")
    if any(not path_in_scope(mission.allowed_paths, path.rstrip("/")) for path in model.allowed_paths):
        raise MissionContractError("workcard_scope_widens_mission")
    return model


def validate_owner_approval(
    mission: MaintenanceMission,
) -> MaintenanceMission:
    """Require the registered owner identity for this provider-free campaign.

    The wire value identifies the claimed approver. Authenticated comment
    evidence belongs to the future intake owner; this PR1 contract never
    treats an arbitrary caller-supplied identity as authentication.
    """

    model = MaintenanceMission.from_wire(mission.to_wire())
    if model.owner_approval.owner_identity not in TRUSTED_OWNER_IDENTITIES:
        raise MissionContractError("owner_approval_identity_untrusted")
    if (
        model.state in {"RUNNING", "VERIFYING", "INTEGRATING", "COMPLETE"}
        and model.owner_approval.owner_identity not in AUTHENTICATED_OWNER_IDENTITIES
    ):
        raise MissionContractError("running_owner_approval_not_authenticated")
    return model


@dataclass(frozen=True)
class OwnerApprovalEvidence:
    """Facts authenticated by a trusted external approval transport.

    This is deliberately a value object, not an authenticator.  The transport
    owner (currently the GitHub Issue-comment reader) must first verify the
    actor and comment identity, then build this exact binding.  The mission
    contract only compares the already-authenticated facts; replay protection
    remains the journal's atomic responsibility.
    """

    transport: str
    repository: str
    mission_id: str
    approval_id: str
    owner_identity: str
    proposal_sha256: str
    accepted_main_sha: str
    evidence_id: str

    def __post_init__(self) -> None:
        if self.transport != "github_issue_comment":
            raise MissionContractError("approval_transport_invalid")
        if REPOSITORY.fullmatch(self.repository) is None:
            raise MissionContractError("approval_repository_invalid")
        _identifier(self.mission_id, "approval_mission_id")
        _identifier(self.approval_id, "approval_id")
        _identifier(self.evidence_id, "approval_evidence_id")
        if self.owner_identity not in AUTHENTICATED_OWNER_IDENTITIES:
            raise MissionContractError("owner_approval_identity_untrusted")
        _sha(self.proposal_sha256, "approval_proposal_sha256")
        _sha(self.accepted_main_sha, "approval_accepted_main_sha", SHA40)


class AuthenticatedOwnerApprovalValidator:
    """Validate one externally authenticated approval evidence record.

    Unlike the historical identity-only helper, this class cannot establish
    authentication from a caller-provided owner string.  It accepts a concrete
    GitHub-comment evidence binding supplied by the trusted transport.  The
    caller cannot use this class for replay protection: the SQLite journal
    atomically consumes the evidence identity after this pure verification.
    """

    def __init__(self, evidence: OwnerApprovalEvidence):
        if not isinstance(evidence, OwnerApprovalEvidence):
            raise MissionContractError("approval_evidence_required")
        self.evidence = evidence

    def verify(self, approval: OwnerApproval | dict[str, Any], proposal_sha256: str) -> bool:
        if not isinstance(approval, OwnerApproval):
            try:
                approval = OwnerApproval.from_wire(approval)
            except Exception:
                return False
        return (
            approval.owner_identity == self.evidence.owner_identity
            and approval.proposal_sha256 == proposal_sha256 == self.evidence.proposal_sha256
            and approval.approval_id == self.evidence.approval_id
        )


def validate_authenticated_owner_approval(
    approval: OwnerApproval | dict[str, Any],
    proposal_sha256: str,
    owner_authenticator: object,
) -> OwnerApproval:
    """Validate externally authenticated approval facts without activating.

    Keeping this operation free of runtime activation or replay mutation gives
    the service a safe verify → journal-consume → activate ordering.
    """

    if SHA256.fullmatch(proposal_sha256) is None:
        raise MissionContractError("activation_proposal_invalid")
    try:
        normalized = (
            approval if isinstance(approval, OwnerApproval) else OwnerApproval.from_wire(approval)
        )
    except (TypeError, ValueError, MissionContractError) as exc:
        raise MissionContractError("activation_approval_invalid") from exc
    if normalized.proposal_sha256 != proposal_sha256:
        raise MissionContractError("activation_approval_mismatch")
    verifier = getattr(owner_authenticator, "verify", None)
    if not callable(verifier):
        raise MissionContractError("activation_authenticator_missing")
    try:
        authenticated = verifier(normalized, proposal_sha256)
    except Exception as exc:
        raise MissionContractError("activation_authentication_failed") from exc
    if authenticated is not True:
        raise MissionContractError("activation_authentication_failed")
    return normalized


def validate_current_mission(
    mission: MaintenanceMission,
    *,
    repository: str,
    base_sha: str,
    branch: str,
    source_ref: str,
    source_sha256: str,
    require_running: bool = False,
    registered_mission: MaintenanceMission | None = None,
) -> MaintenanceMission:
    """Validate a Mission against one verified current checkout.

    State and accepted-main are mutable activation bindings.  All remaining
    mission semantics and the owner approval remain equal to the registered
    or proposed mission contract.
    """

    model = validate_owner_approval(mission)
    registered = (
        registered_mission
        if registered_mission is not None
        else campaign_mission()
        if model.mission_id == CAMPAIGN_MISSION_ID
        else model
    )
    if (
        model.mission_id != registered.mission_id
        or model.proposal_wire() != registered.proposal_wire()
        or model.proposal_sha256 != registered.proposal_sha256
    ):
        raise MissionContractError("mission_registration_invalid")
    if model.state not in {"RUNNING", "VERIFYING", "INTEGRATING", "COMPLETE"} and (
        model.owner_approval != registered.owner_approval
    ):
        raise MissionContractError("mission_registration_invalid")
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
    activation_nonce = getattr(mission, "_activation_nonce", None)
    if require_running and (
        not isinstance(activation_nonce, str)
        or activation_nonce not in _ACTIVE_ACTIVATION_NONCES
    ):
        raise MissionContractError("mission_activation_missing")
    if require_running and model.state != "RUNNING":
        raise MissionContractError("mission_not_running")
    if isinstance(activation_nonce, str):
        return replace(model, _activation_nonce=activation_nonce)
    return model


def restore_durable_activation(mission: MaintenanceMission) -> MaintenanceMission:
    """Restore the process-local activation capability after journal replay.

    The durable proof is the service-owned ``MISSION_ACTIVATED`` journal
    event.  The service calls this helper only after it has verified that event
    and its exact stored mission payload.  The nonce remains process-local so
    a serialized ``state=RUNNING`` value alone never activates a Mission.
    """

    model = MaintenanceMission.from_wire(mission.to_wire())
    if model.state != "RUNNING":
        raise MissionContractError("mission_not_running")
    nonce = secrets.token_hex(16)
    _ACTIVE_ACTIVATION_NONCES.add(nonce)
    return replace(model, _activation_nonce=nonce)


def activate_current_mission(
    *,
    repository: str,
    base_sha: str,
    branch: str,
    source_ref: str,
    source_sha256: str,
    proposal_sha256: str,
    owner_approval: OwnerApproval | dict[str, Any],
    owner_authenticator: object,
    mission: MaintenanceMission | None = None,
) -> MaintenanceMission:
    """Activate the Mission on the freshly verified accepted main.

    This is the sole current-main activation constructor.  It does not change
    the approved proposal, owner approval, scope, budget, or forbidden
    effects; it only binds the immutable Mission to the observed checkout and
    moves its lifecycle projection to ``RUNNING``.
    """

    registered = (
        validate_registered_campaign()
        if mission is None
        else mission
    )
    approval = validate_authenticated_owner_approval(
        owner_approval, proposal_sha256, owner_authenticator
    )
    activation_nonce = secrets.token_hex(16)
    _ACTIVE_ACTIVATION_NONCES.add(activation_nonce)
    activated = MaintenanceMission(
        **{
            **registered.__dict__,
            "state": "RUNNING",
            "owner_approval": approval,
            "repository_identity": RepositoryIdentity(
                repository,
                base_sha,
                branch,
                source_ref,
                source_sha256,
            ),
            "_activation_nonce": activation_nonce,
        }
    )
    return validate_current_mission(
        activated,
        repository=repository,
        base_sha=base_sha,
        branch=branch,
        source_ref=source_ref,
        source_sha256=source_sha256,
        registered_mission=registered,
    )


def build_research_acceptance_ledger() -> MissionAcceptanceLedger:
    """Canonical 18-node research acceptance ledger for the closed-loop research mainline."""
    obligations = (
        AcceptanceObligation(
            obligation_id="common_rwe_evidence_basis",
            description="Reconcile and validate frozen RWE corpus, protocol, schedule, task bindings, and baseline seeds.",
            category="evidence_basis",
            dependencies=(),
            required_paths=("engine/src/rwe", "docs/ROADMAP.md"),
        ),
        AcceptanceObligation(
            obligation_id="contemporary_rwe_replay",
            description="Execute or evaluate contemporary RWE old/new replay against frozen comparison manifest and evidence gates.",
            category="evaluation",
            dependencies=("common_rwe_evidence_basis",),
            required_paths=("engine/src/rwe", "docs/ROADMAP.md"),
        ),
        AcceptanceObligation(
            obligation_id="mx1_c1_1x2x1",
            description="MX1 C1 ladder Rung 1 (1x2x1): isolate Model effects across frozen model descriptors with arm-zero harness and no-projection strategy.",
            category="ladder",
            dependencies=("common_rwe_evidence_basis",),
            required_paths=("engine/src/harness_evolution.rs", "engine/src/harness_evolution_eval.rs"),
        ),
        AcceptanceObligation(
            obligation_id="mx1_c1_1x2x3",
            description="MX1 C1 ladder Rung 2 (1x2x3): evaluate Strategy and ModelxStrategy interactions across baseline, memory-only, and skill-only strategies.",
            category="ladder",
            dependencies=("mx1_c1_1x2x1",),
            required_paths=("engine/src/harness_evolution.rs", "engine/src/harness_evolution_eval.rs"),
        ),
        AcceptanceObligation(
            obligation_id="mx1_c1_2x2x3",
            description="MX1 C1 ladder Rung 3 (2x2x3): evaluate Harness and higher-order interactions with confined second harness.",
            category="ladder",
            dependencies=("mx1_c1_1x2x3",),
            required_paths=("engine/src/harness_evolution.rs", "engine/src/harness_evolution_eval.rs"),
        ),
        AcceptanceObligation(
            obligation_id="cws_strategy_evidence",
            description="Evaluate CWS runtime projection, residency, and default-off analysis boundaries against hard quality and safety gates.",
            category="evaluation",
            dependencies=("common_rwe_evidence_basis",),
            required_paths=("engine/src/rwe", "docs/ROADMAP.md"),
        ),
        AcceptanceObligation(
            obligation_id="harness_evolution",
            description="Evaluate candidate Pareto archive, prediction outcomes, and mutation hypotheses on sealed holdouts.",
            category="evaluation",
            dependencies=("mx1_c1_2x2x3",),
            required_paths=("engine/src/harness_evolution.rs", "engine/src/harness_evolution_eval.rs"),
        ),
        AcceptanceObligation(
            obligation_id="level_1",
            description="Evaluate Level-1 candidate eligibility, lower-rung evidence completeness, and hard quality/safety gates.",
            category="gate",
            dependencies=("harness_evolution",),
            required_paths=("docs/ROADMAP.md", "docs/ARCHITECTURE.md"),
        ),
        AcceptanceObligation(
            obligation_id="transfer",
            description="Evaluate cross-task and cross-domain transfer evidence before any harness candidate advancement.",
            category="transfer",
            dependencies=("level_1",),
            required_paths=("docs/ROADMAP.md", "docs/ARCHITECTURE.md"),
        ),
        AcceptanceObligation(
            obligation_id="replication",
            description="Evaluate multi-seed replication fidelity and deterministic variance bounds.",
            category="replication",
            dependencies=("level_1",),
            required_paths=("docs/ROADMAP.md", "docs/ARCHITECTURE.md"),
        ),
        AcceptanceObligation(
            obligation_id="memory",
            description="Evaluate durable memory version retrieval efficiency, retention bounds, and eviction safety.",
            category="capability",
            dependencies=("level_1",),
            required_paths=("docs/ROADMAP.md", "docs/ARCHITECTURE.md"),
        ),
        AcceptanceObligation(
            obligation_id="skill",
            description="Evaluate acquired skill reuse, execution boundary preservation, and tool-allowlist conformance.",
            category="capability",
            dependencies=("level_1",),
            required_paths=("docs/ROADMAP.md", "docs/ARCHITECTURE.md"),
        ),
        AcceptanceObligation(
            obligation_id="level_2",
            description="Evaluate Level-2 candidate prerequisites, lower-rung evidence completion, and comparability.",
            category="gate",
            dependencies=("level_1", "transfer", "replication"),
            required_paths=("docs/ROADMAP.md", "docs/ARCHITECTURE.md"),
        ),
        AcceptanceObligation(
            obligation_id="adoption_decision",
            description="Explicit human owner adoption review and decision; no autonomous self-adoption or production replacement.",
            category="adoption",
            dependencies=("level_2",),
            required_paths=("docs/ROADMAP.md", "docs/ARCHITECTURE.md"),
        ),
        AcceptanceObligation(
            obligation_id="meta",
            description="Evaluate Meta research program prerequisites, evaluator invariance, and recursive control boundaries.",
            category="meta",
            dependencies=("level_2",),
            required_paths=("docs/ROADMAP.md", "docs/ARCHITECTURE.md"),
        ),
        AcceptanceObligation(
            obligation_id="r4",
            description="R4 evaluation: atomic journal append and crash recovery under high concurrency.",
            category="meta",
            dependencies=("meta",),
            required_paths=("docs/ROADMAP.md", "docs/ARCHITECTURE.md"),
        ),
        AcceptanceObligation(
            obligation_id="r5",
            description="R5 evaluation: distributed observer consistency and cross-worktree reconciliation boundaries.",
            category="meta",
            dependencies=("meta",),
            required_paths=("docs/ROADMAP.md", "docs/ARCHITECTURE.md"),
        ),
        AcceptanceObligation(
            obligation_id="r6",
            description="R6 evaluation: long-horizon recursive task decomposition and bounded delegation contracts.",
            category="meta",
            dependencies=("meta",),
            required_paths=("docs/ROADMAP.md", "docs/ARCHITECTURE.md"),
        ),
    )
    ledger = MissionAcceptanceLedger(obligations)
    ledger.validate()
    return ledger


def compile_proposal_mission(
    raw_request: str,
    *,
    repository: str,
    base_sha: str,
    branch: str = "main",
    source_ref: str = "main",
    source_sha256: str = "",
    mission_id: str | None = None,
    acceptance_ledger: MissionAcceptanceLedger | None = None,
    predecessor_mission_id: str | None = None,
) -> tuple[MaintenanceMission, str]:
    """Compile an untrusted natural language request into a proposed MaintenanceMission."""

    import shadow_steward
    proposal = shadow_steward.compile_proposal(raw_request)
    sha_source = source_sha256 or hashlib.sha256(base_sha.encode("ascii")).hexdigest()
    mid = mission_id or f"MISSION-{proposal.proposal_sha256[:16].upper()}"
    identity = RepositoryIdentity(
        repository=repository,
        base_sha=base_sha,
        branch=branch,
        source_ref=source_ref,
        source_sha256=sha_source,
    )
    paths = tuple(sorted(proposal.requested_paths or ("README.md",)))
    grants = (
        Grant(
            grant_id="repository-maintenance",
            grant_type="repository_maintenance",
            allowed_paths=paths,
            allowed_operations=tuple(sorted(SAFE_OPERATIONS)),
            max_uses=32,
        ),
    )
    budget = Budget(
        max_attempts=32,
        max_retries=31,
        max_runtime_seconds=7 * 24 * 60 * 60,
        max_calls=10_000,
        max_cost_micros=0,
        max_external_effects=0,
    )
    stops = tuple(
        StopRule(code, STOP_CATEGORIES.get(code, "PAUSED_FOR_OWNER"), f"Stop rule for {code}")
        for code in (
            "WORKER_FAILED",
            "WORKER_TIMEOUT",
            "TEST_FAILED",
            "CI_FAILED",
            "REVIEW_CHANGES_REQUESTED",
            "MAIN_DRIFT",
            "SCOPE_EXCEEDED",
            "AUTHORITY_REQUIRED",
            "REQUIREMENT_CONFLICT",
            "EXTERNAL_OUTCOME_UNKNOWN",
            "SAFETY_CONFLICT",
        )
    )
    rollback = RollbackBoundary("restore_accepted_main", f"accepted-main:{base_sha}", ("git_diff_check", "test_baseline_parity"))
    quality_checks = ("focused_checks_required", "full_checks_required", "exact_head_review_required", "k2_scheduler_observed")
    wire = {
        "schema_version": SCHEMA_VERSION,
        "mission_id": mid,
        "objective": raw_request.strip(),
        "completion_conditions": list(quality_checks),
        "repository_identity": identity.proposal_wire(),
        "allowed_paths": list(paths),
        "allowed_change_types": list(proposal.change_types),
        "forbidden_changes": ["outside-approved-scope/"],
        "standing_grants": [g.to_wire() for g in grants],
        "budget": budget.to_wire(),
        "quality_checks": list(quality_checks),
        "stop_rules": [s.to_wire() for s in stops],
        "rollback": rollback.to_wire(),
    }
    if acceptance_ledger is not None:
        wire["acceptance_ledger"] = acceptance_ledger.proposal_wire()
    if predecessor_mission_id is not None:
        wire["predecessor_mission_id"] = predecessor_mission_id

    proposal_sha256 = json_sha256(wire)
    approval = OwnerApproval(
        owner_identity="repository-owner",
        proposal_sha256=proposal_sha256,
        approval_id="pending-approval",
        approved_at=_now(),
    )
    mission = MaintenanceMission(
        mission_id=mid,
        state="PROPOSING",
        objective=raw_request.strip(),
        completion_conditions=quality_checks,
        repository_identity=identity,
        allowed_paths=paths,
        allowed_change_types=proposal.change_types,
        forbidden_changes=("outside-approved-scope/",),
        standing_grants=grants,
        budget=budget,
        quality_checks=quality_checks,
        stop_rules=stops,
        rollback=rollback,
        proposal_sha256=proposal_sha256,
        owner_approval=approval,
        acceptance_ledger=acceptance_ledger,
        predecessor_mission_id=predecessor_mission_id,
    )
    return mission, proposal_sha256


def build_research_successor_mission(
    *,
    base_sha: str,
    predecessor_mission_id: str = "MISSION-RESEARCH-20260901",
    mission_id: str = "MISSION-RESEARCH-20260901-SUCCESSOR",
    repository: str = "Igzela/token-efficient-agent-harness-lab",
    branch: str = "main",
    source_ref: str = "main",
    source_sha256: str = "",
) -> MaintenanceMission:
    """Construct an owner-approvable research successor Mission bound to accepted main."""
    raw_request = (
        "Complete bounded closed loop research mainline and obtain actual RWE MX1 C1 CWS "
        "Harness Evolution L1 L2 evidence and disposition transfer replication memory skill adoption "
        "Meta R4 R5 R6 through finite frozen canonical experiments with common task corpus evaluator "
        "Harness Model Strategy descriptors schedule budgets identities protocol seeds lifecycle "
        "analysis and results with documentation tests workflow source changes in docs/ROADMAP.md "
        "docs/ARCHITECTURE.md docs/AUTONOMY.md docs/RUNBOOK.md engine/src/rwe engine/src/harness_evolution.rs "
        "engine/src/harness_evolution_eval.rs engine/src/storage/local_product_store scripts/agent-control "
        "tests/test_mission_contract.py tests/test_agent_shadow_steward.py."
    )
    ledger = build_research_acceptance_ledger()
    mission, _ = compile_proposal_mission(
        raw_request,
        repository=repository,
        base_sha=base_sha,
        branch=branch,
        source_ref=source_ref,
        source_sha256=source_sha256,
        mission_id=mission_id,
        acceptance_ledger=ledger,
        predecessor_mission_id=predecessor_mission_id,
    )
    return mission


def validate_execution_scope(
    mission: MaintenanceMission,
    paths: tuple[str, ...] | list[str],
    operations: tuple[str, ...] | list[str],
) -> None:
    """Require the bounded repository-maintenance grant for one execution."""

    model = MaintenanceMission.from_wire(mission.to_wire())
    requested_paths = _paths(list(paths), "execution_paths")
    requested_operations = _strings(list(operations), "execution_operations")
    grants = [
        grant
        for grant in model.standing_grants
        if grant.grant_type == "repository_maintenance"
    ]
    if len(grants) != 1:
        raise MissionContractError("repository_maintenance_grant_missing")
    grant = grants[0]
    if any(operation not in grant.allowed_operations for operation in requested_operations):
        raise MissionContractError("execution_operation_outside_grant")
    if any(
        not any(path_in_scope((scope,), path) for scope in grant.allowed_paths)
        for path in requested_paths
    ):
        raise MissionContractError("execution_path_outside_grant")


def validate_standing_recovery_grant(
    mission: MaintenanceMission,
    *,
    repository: str,
) -> Grant:
    """Validate that the active mission possesses standing repository-maintenance recovery authority."""

    model = MaintenanceMission.from_wire(mission.to_wire())
    if model.repository_identity.repository != repository:
        raise MissionContractError("recovery_grant_repository_mismatch")
    grants = [
        grant
        for grant in model.standing_grants
        if grant.grant_type == "repository_maintenance"
    ]
    if not grants:
        raise MissionContractError("repository_maintenance_grant_missing")
    grant = grants[0]
    if "quarantine_exact_owned_candidate" not in grant.allowed_operations:
        raise MissionContractError("recovery_operation_outside_grant")
    return grant


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
            "migration-repository-maintenance",
            "repository_maintenance",
            CAMPAIGN_ALLOWED_PATHS,
            tuple(sorted(SAFE_OPERATIONS)),
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
        CAMPAIGN_ALLOWED_PATHS,
        tuple(sorted(SAFE_CHANGE_TYPES)),
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
            f"accepted-main:{CAMPAIGN_BASE_SHA}",
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
    validate_registered_campaign()
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
    if packet_id == "PE7-AUTONOMOUS-STEWARD-PR4A":
        required_boundary_markers = (
            "only the parent packet coordinator may submit the bound Draft PR through pr_binding.py:create_or_update_pr.",
            "Child execution, repair, and review sessions must not receive GitHub write credentials or Provider secrets.",
            "Do not switch the lifecycle writer or perform a canary/single-writer cutover.",
        )
        if any(
            not any(marker in item for item in capsule_forbidden)
            for marker in required_boundary_markers
        ):
            raise MissionContractError("legacy_pr4a_boundary_missing")
    if any(
        not path_in_scope(LEGACY_COMPATIBILITY_PATHS, path.rstrip("/"))
        for path in packet_paths
    ):
        raise MissionContractError("legacy_scope_widens_safe_surface")
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
        LIFECYCLE_COORDINATOR,
    )


__all__ = [
    "AuthenticatedOwnerApprovalValidator",
    "Budget",
    "Grant",
    "LegacyMissionProjection",
    "LIFECYCLE_COORDINATOR",
    "MaintenanceMission",
    "MissionContractError",
    "OwnerApproval",
    "OwnerApprovalEvidence",
    "RepositoryIdentity",
    "RollbackBoundary",
    "Stage",
    "StopRule",
    "WorkCard",
    "activate_current_mission",
    "campaign_mission",
    "compile_proposal_mission",
    "json_sha256",
    "path_in_scope",
    "stop_category",
    "validate_current_mission",
    "validate_execution_scope",
    "validate_authenticated_owner_approval",
    "validate_legacy_compatibility",
    "validate_owner_approval",
    "validate_registered_campaign",
    "validate_stage",
    "validate_workcard",
    "restore_durable_activation",
    "validate_standing_recovery_grant",
]
