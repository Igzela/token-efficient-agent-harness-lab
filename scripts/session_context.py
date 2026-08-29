#!/usr/bin/env python3
"""Route minimal repository context and recover interrupted local sessions.

Accepted documents and GitHub remain authoritative.  The checkpoint written by
this tool is a bounded, non-authoritative projection in Git's private path; it
can prove that a later local checkout still matches a handoff, but it cannot
select a mission, grant authority, or make lifecycle/CI/review decisions. Its
verification evidence is bound to the accepted dispatch capsule's exact
ordered verification contract; a rehashed object with a substituted evidence
set or an inconsistent work-state/verification-state invariant is rejected
rather than resumed.
"""

from __future__ import annotations

import argparse
from collections.abc import Mapping
from dataclasses import dataclass, fields as dataclass_fields, replace
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import shlex
import stat
import subprocess
import sys
import tempfile
from typing import Any


ROOT = Path(__file__).resolve().parents[1]

ROUTE_SCHEMA = "agent_context_routes.v1"
CHECKPOINT_SCHEMA = "agent_session_handoff.v1"
ENTRY_SCHEMA = "agent_session_entry.v1"
CHECKPOINT_AUTHORITY = "non_authoritative_local_projection"
ENTRY_AUTHORITY = "accepted_context_projection_only; grants_no_new_authority"
CHECKPOINT_BASENAME = "agent-session-handoff.v1.json"

MAX_ROUTE_DOCUMENT_BYTES = 128 * 1024
MAX_ROUTE_DOCUMENTS = 6
MAX_CHECKPOINT_BYTES = 64 * 1024
MAX_DISPATCH_CAPSULE_BYTES = 12 * 1024
MAX_ENTRY_BYTES = 16 * 1024
MAX_DIRTY_PATHS = 5_000
MAX_PATH_CHARS = 512
MAX_TEXT_CHARS = 2_048
MISSION_CONTRACT_PATH = ROOT / "scripts" / "agent-control" / "mission_contract.py"

SHA40 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
MISSION_ID = re.compile(r"^[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+$")
OPTION_ID = re.compile(r"^[a-z][a-z0-9-]{0,31}$")
ROUTE_MARKER = re.compile(
    r"<!--\s*agent-context-routes:v1\s*(?P<payload>\{.*?\})\s*-->", re.DOTALL
)

ROLES = frozenset({"planning", "coding", "review", "ci-repair", "operator", "contributor"})
WORK_STATES = frozenset({"WIP", "STABLE", "BLOCKED", "OUTCOME_UNKNOWN"})
VERIFICATION_STATES = frozenset({"PASS", "FAIL", "NOT_RUN", "BLOCKED"})
EXECUTABLE_MISSION_STATES = frozenset({"READY_FOR_EXECUTION", "IN_PROGRESS"})
ENTRY_CONTEXT_MODES = frozenset(
    {"FRESH_MISSION", "RESUME_CHECKPOINT", "REPAIR", "STOP"}
)


def _checkpoint_write_commands(
    *,
    role: str,
    mission_id: str,
    context_mode: str,
    checkpoint_allowed: bool,
    verification_commands: tuple[str, ...],
) -> dict[str, str | None] | None:
    """Return fixed checkpoint commands with no caller-controlled text slots."""

    if role != "coding" or not checkpoint_allowed or context_mode == "STOP":
        return None
    common = [
        "uv",
        "run",
        "--no-project",
        "python",
        "scripts/session_context.py",
        "checkpoint-auto",
        "--role",
        role,
        "--mission",
        mission_id,
    ]
    stable_available = all(
        _safe_verification_argv(command) is not None
        for command in verification_commands
    )
    return {
        "wip": shlex.join([*common, "--work-state", "WIP", "--offline"]),
        "stable": (
            shlex.join(
                [*common, "--work-state", "STABLE", "--verify", "--offline"]
            )
            if stable_available
            else None
        ),
    }


def _safe_verification_argv(command: str) -> tuple[str, ...] | None:
    """Parse the narrow provider-free command forms checkpoint-auto may execute."""

    if any(character in command for character in ";&|><\n\r"):
        return None
    try:
        argv = tuple(shlex.split(command))
    except ValueError:
        return None
    if not argv:
        return None
    if argv[0] == "cargo" and len(argv) >= 2:
        if argv[1] in {"build", "check", "clippy", "test"}:
            return argv
        if argv[1] == "fmt" and "--check" in argv[2:]:
            return argv
    if argv[:4] == ("uv", "run", "--no-project", "python"):
        python_args = argv[4:]
        if python_args[:2] == ("-m", "unittest") and len(python_args) >= 3:
            return argv
        if python_args in {
            ("tools/check_security_baseline.py",),
            ("scripts/check_agent_handoff.py",),
        }:
            return argv
    if argv == ("python", "scripts/check_agent_handoff.py"):
        return argv
    if argv == ("git", "diff", "--check"):
        return argv
    if len(argv) == 2 and argv[0] == "bash":
        if argv[1] in {
            "scripts/check_wire_codegen_drift.sh",
            "scripts/verify_rust_typescript_stack.sh",
        }:
            return argv
    return None
DISPATCH_CAPSULE_FIELDS = frozenset(
    {
        "accepted_binding_source",
        "allowed_outputs",
        "allowed_paths",
        "authority_consumption_allowed",
        "dispatch_lane",
        "expected_artifacts",
        "external_effect_limit",
        "forbidden_changes",
        "forbidden_next_actions",
        "goal",
        "known_store_mutations",
        "ordered_steps",
        "mission_id",
        "mission_state",
        "pause_gates",
        "plan_lane_state",
        "promotion_evidence_sha256",
        "prerequisite_receipts",
        "prerequisites",
        "private_paths_allowed",
        "read_paths",
        "rollback",
        "risk_class",
        "route_manifest_sha256",
        "schema_version",
        "secret_values_allowed",
        "verification",
        "verification_family",
        "worker_tier",
        "t3_request_digest",
    }
)
CANONICAL_DOCUMENTS = frozenset(
    {
        "START_HERE.md",
        "AGENTS.md",
        "README.md",
        "CLAUDE.md",
        "docs/ARCHITECTURE.md",
        "docs/AUTONOMY.md",
        "docs/ROADMAP.md",
        "docs/RUNBOOK.md",
    }
)
CANONICAL_DOCUMENT_PATHS = (
    "START_HERE.md",
    "AGENTS.md",
    "README.md",
    "docs/ARCHITECTURE.md",
    "docs/AUTONOMY.md",
    "docs/ROADMAP.md",
    "docs/RUNBOOK.md",
)
class SessionContextError(ValueError):
    """A bounded context or recovery contract could not be proved."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


def _canonical_json(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def _json_sha256(value: object) -> str:
    return hashlib.sha256(_canonical_json(value).encode("utf-8")).hexdigest()


def _bounded_text(value: object, field: str, *, max_chars: int = MAX_TEXT_CHARS) -> str:
    if not isinstance(value, str) or not value.strip() or len(value) > max_chars:
        raise SessionContextError(f"{field}_invalid")
    if any(ord(character) < 32 and character not in "\t\n" for character in value):
        raise SessionContextError(f"{field}_invalid")
    return value.strip()


def _validate_sha(value: object, field: str, pattern: re.Pattern[str]) -> str:
    if not isinstance(value, str) or not pattern.fullmatch(value):
        raise SessionContextError(f"{field}_invalid")
    return value


def _repo_path(value: object, field: str = "path", *, allow_directory: bool = True) -> str:
    if not isinstance(value, str) or not value or len(value) > MAX_PATH_CHARS:
        raise SessionContextError(f"{field}_invalid")
    if "\\" in value or "\x00" in value or any(character.isspace() for character in value):
        raise SessionContextError(f"{field}_invalid")
    directory = value.endswith("/")
    candidate = value[:-1] if directory else value
    path = PurePosixPath(candidate)
    if (
        not candidate
        or path.is_absolute()
        or any(part in {"", ".", ".."} for part in path.parts)
        or str(path) != candidate
        or (directory and not allow_directory)
    ):
        raise SessionContextError(f"{field}_invalid")
    return candidate + ("/" if directory else "")


def _bounded_string_list(
    value: object,
    field: str,
    *,
    allow_empty: bool = False,
    max_items: int = 200,
) -> list[str]:
    if not isinstance(value, list) or len(value) > max_items:
        raise SessionContextError(f"{field}_invalid")
    result = [_bounded_text(item, field) for item in value]
    if not allow_empty and not result:
        raise SessionContextError(f"{field}_invalid")
    if len(result) != len(set(result)):
        raise SessionContextError(f"{field}_duplicated")
    return result


def _path_list(value: object, field: str, *, allow_empty: bool = True) -> list[str]:
    if not isinstance(value, list) or len(value) > MAX_DIRTY_PATHS:
        raise SessionContextError(f"{field}_invalid")
    result = [_repo_path(item, field, allow_directory=False) for item in value]
    if not allow_empty and not result:
        raise SessionContextError(f"{field}_invalid")
    if result != sorted(set(result)):
        raise SessionContextError(f"{field}_not_sorted_or_unique")
    return result


def _dispatch_path_list(value: object, field: str) -> list[str]:
    if not isinstance(value, list) or not value or len(value) > 50:
        raise SessionContextError(f"{field}_invalid")
    try:
        result = sorted({_repo_path(item, field) for item in value})
    except SessionContextError as exc:
        raise SessionContextError(f"{field}_invalid") from exc
    if len(result) != len(value):
        raise SessionContextError(f"{field}_invalid")
    return result


def _path_is_allowed(path: str, allowed_paths: list[str]) -> bool:
    for allowed in allowed_paths:
        if allowed.endswith("/") and path.startswith(allowed):
            return True
        if path == allowed:
            return True
    return False


def _wire_mapping(value: object, reason: str) -> Mapping[str, object]:
    if not isinstance(value, Mapping):
        raise SessionContextError(reason)
    return value


@dataclass(frozen=True)
class RouteRole:
    required: tuple[str, ...]
    optional: tuple[tuple[str, str], ...]

    def option_map(self) -> dict[str, str]:
        return dict(self.optional)

    def to_wire(self) -> dict[str, object]:
        return {"required": list(self.required), "optional": self.option_map()}


@dataclass(frozen=True)
class RouteContract:
    schema_version: str
    max_required_documents: int
    roles: tuple[tuple[str, RouteRole], ...]

    def role_for(self, role: str) -> RouteRole:
        try:
            return dict(self.roles)[role]
        except KeyError as exc:
            raise SessionContextError("role_unsupported") from exc

    def to_wire(self) -> dict[str, object]:
        return {
            "schema_version": self.schema_version,
            "max_required_documents": self.max_required_documents,
            "roles": {role: route.to_wire() for role, route in self.roles},
        }


@dataclass(frozen=True)
class MissionBinding:
    mission_id: str
    state: str
    source_path: str
    mission_sha256: str
    allowed_paths: tuple[str, ...]
    forbidden_next_actions: tuple[str, ...]
    execution_authorized: bool
    checkpoint_allowed: bool
    dispatch_lane: str | None

    @classmethod
    def from_wire(
        cls,
        value: object,
        *,
        require_checkpoint: bool = False,
    ) -> MissionBinding:
        if isinstance(value, cls):
            value = value.to_wire()
        wire = _wire_mapping(value, "mission_binding_invalid")
        allowed_fields = {
            "mission_id",
            "state",
            "source_path",
            "mission_sha256",
            "allowed_paths",
            "forbidden_next_actions",
            "execution_authorized",
            "checkpoint_allowed",
            "dispatch_lane",
        }
        required_fields = {
            "mission_id",
            "state",
            "source_path",
            "mission_sha256",
            "allowed_paths",
        }
        if not required_fields.issubset(wire) or not set(wire).issubset(allowed_fields):
            raise SessionContextError("mission_binding_fields_invalid")
        mission_id = wire.get("mission_id")
        if not isinstance(mission_id, str) or not MISSION_ID.fullmatch(mission_id):
            raise SessionContextError("mission_id_invalid")
        state = _bounded_text(wire.get("state"), "mission_state", max_chars=64)
        if not re.fullmatch(r"[A-Z0-9_]+", state):
            raise SessionContextError("mission_state_invalid")
        source_path = _repo_path(wire.get("source_path"), "mission_source_path")
        mission_sha256 = _validate_sha(
            wire.get("mission_sha256"), "mission_sha256", SHA256
        )
        raw_allowed = wire.get("allowed_paths")
        if not isinstance(raw_allowed, list):
            raise SessionContextError("mission_allowed_paths_invalid")
        try:
            allowed_paths = tuple(
                sorted({_repo_path(item, "mission_allowed_path") for item in raw_allowed})
            )
        except SessionContextError as exc:
            raise SessionContextError("mission_allowed_paths_invalid") from exc
        raw_forbidden = wire.get("forbidden_next_actions", [])
        forbidden_next_actions = tuple(
            _bounded_string_list(
                raw_forbidden,
                "forbidden_next_actions",
                allow_empty=True,
            )
        )
        execution_authorized = wire.get("execution_authorized", False)
        if not isinstance(execution_authorized, bool):
            raise SessionContextError("mission_execution_authority_invalid")
        checkpoint_allowed = wire.get("checkpoint_allowed", execution_authorized)
        if not isinstance(checkpoint_allowed, bool):
            raise SessionContextError("mission_checkpoint_authority_invalid")
        dispatch_lane_value = wire.get("dispatch_lane")
        dispatch_lane = (
            None
            if dispatch_lane_value is None
            else _bounded_text(
                dispatch_lane_value,
                "mission_dispatch_lane",
                max_chars=128,
            )
        )
        model = cls(
            mission_id=mission_id,
            state=state,
            source_path=source_path,
            mission_sha256=mission_sha256,
            allowed_paths=allowed_paths,
            forbidden_next_actions=forbidden_next_actions,
            execution_authorized=execution_authorized,
            checkpoint_allowed=checkpoint_allowed,
            dispatch_lane=dispatch_lane,
        )
        if require_checkpoint and (
            model.source_path != "docs/AUTONOMY.md"
            or model.state not in EXECUTABLE_MISSION_STATES
            or not model.checkpoint_allowed
        ):
            raise SessionContextError("mission_not_executable")
        return model

    def to_wire(self) -> dict[str, object]:
        return {
            "mission_id": self.mission_id,
            "state": self.state,
            "source_path": self.source_path,
            "mission_sha256": self.mission_sha256,
            "allowed_paths": list(self.allowed_paths),
            "forbidden_next_actions": list(self.forbidden_next_actions),
            "execution_authorized": self.execution_authorized,
            "checkpoint_allowed": self.checkpoint_allowed,
            "dispatch_lane": self.dispatch_lane,
        }


@dataclass(frozen=True)
class ContextRoute:
    schema_version: str
    authority: str
    accepted_main_sha: str
    role: str
    mission_id: str
    mission_state: str
    mission_sha256: str
    documents: tuple[str, ...]
    included_options: tuple[str, ...]
    execution_authorized: bool
    checkpoint_allowed: bool
    bootstrap_order: tuple[str, ...]

    def to_wire(self) -> dict[str, object]:
        return {
            "schema_version": self.schema_version,
            "authority": self.authority,
            "accepted_main_sha": self.accepted_main_sha,
            "role": self.role,
            "mission_id": self.mission_id,
            "mission_state": self.mission_state,
            "mission_sha256": self.mission_sha256,
            "documents": list(self.documents),
            "included_options": list(self.included_options),
            "execution_authorized": self.execution_authorized,
            "checkpoint_allowed": self.checkpoint_allowed,
            "bootstrap_order": list(self.bootstrap_order),
        }


@dataclass(frozen=True)
class CheckoutSnapshot:
    accepted_main_sha: str
    head_sha: str
    branch: str
    detached: bool
    dirty_paths: tuple[str, ...]
    path_digests: tuple[tuple[str, str], ...]
    worktree_sha256: str

    @classmethod
    def from_wire(cls, value: object) -> CheckoutSnapshot:
        if isinstance(value, cls):
            value = value.to_wire()
        wire = _wire_mapping(value, "checkout_snapshot_invalid")
        if set(wire) != {field.name for field in dataclass_fields(cls)}:
            raise SessionContextError("checkout_snapshot_fields_invalid")
        accepted_main_sha = _validate_sha(
            wire.get("accepted_main_sha"), "accepted_main_sha", SHA40
        )
        head_sha = _validate_sha(wire.get("head_sha"), "head_sha", SHA40)
        branch = _bounded_text(wire.get("branch"), "branch", max_chars=256)
        detached = wire.get("detached")
        if detached is not False or branch.startswith("-"):
            raise SessionContextError("detached_or_invalid_branch")
        dirty_paths = tuple(_path_list(wire.get("dirty_paths"), "dirty_paths"))
        raw_digests = wire.get("path_digests")
        if not isinstance(raw_digests, Mapping) or set(raw_digests) != set(dirty_paths):
            raise SessionContextError("path_digests_invalid")
        path_digests: list[tuple[str, str]] = []
        for path in dirty_paths:
            _repo_path(path, "path_digest_key", allow_directory=False)
            digest = _validate_sha(raw_digests[path], "path_digest", SHA256)
            path_digests.append((path, digest))
        worktree_sha256 = _validate_sha(
            wire.get("worktree_sha256"), "worktree_sha256", SHA256
        )
        return cls(
            accepted_main_sha=accepted_main_sha,
            head_sha=head_sha,
            branch=branch,
            detached=False,
            dirty_paths=dirty_paths,
            path_digests=tuple(path_digests),
            worktree_sha256=worktree_sha256,
        )

    def digest_map(self) -> dict[str, str]:
        return dict(self.path_digests)

    def to_wire(self) -> dict[str, object]:
        return {
            "accepted_main_sha": self.accepted_main_sha,
            "head_sha": self.head_sha,
            "branch": self.branch,
            "detached": self.detached,
            "dirty_paths": list(self.dirty_paths),
            "path_digests": self.digest_map(),
            "worktree_sha256": self.worktree_sha256,
        }


@dataclass(frozen=True)
class VerificationResult:
    check: str
    status: str

    @classmethod
    def from_wire(cls, value: object) -> VerificationResult:
        wire = _wire_mapping(value, "verification_results_invalid")
        if set(wire) != {"check", "status"}:
            raise SessionContextError("verification_results_invalid")
        check = _bounded_text(wire.get("check"), "verification_check", max_chars=256)
        status_value = wire.get("status")
        if not isinstance(status_value, str) or status_value not in VERIFICATION_STATES:
            raise SessionContextError("verification_results_invalid")
        return cls(check=check, status=status_value)

    def to_wire(self) -> dict[str, str]:
        return {"check": self.check, "status": self.status}


@dataclass(frozen=True)
class SessionCheckpoint:
    schema_version: str
    projection_authority: str
    mission_id: str
    mission_state: str
    mission_source_path: str
    mission_sha256: str
    accepted_main_sha: str
    head_sha: str
    branch: str
    role: str
    work_state: str
    completed_step: str
    owned_paths: tuple[str, ...]
    preserve_paths: tuple[str, ...]
    dirty_paths: tuple[str, ...]
    path_digests: tuple[tuple[str, str], ...]
    worktree_sha256: str
    verification_contract_sha256: str
    verification_results: tuple[VerificationResult, ...]
    next_action: str
    forbidden_next_actions: tuple[str, ...]
    checkpoint_id: str

    def unsigned_wire(self) -> dict[str, object]:
        return {
            "schema_version": self.schema_version,
            "projection_authority": self.projection_authority,
            "mission_id": self.mission_id,
            "mission_state": self.mission_state,
            "mission_source_path": self.mission_source_path,
            "mission_sha256": self.mission_sha256,
            "accepted_main_sha": self.accepted_main_sha,
            "head_sha": self.head_sha,
            "branch": self.branch,
            "role": self.role,
            "work_state": self.work_state,
            "completed_step": self.completed_step,
            "owned_paths": list(self.owned_paths),
            "preserve_paths": list(self.preserve_paths),
            "dirty_paths": list(self.dirty_paths),
            "path_digests": dict(self.path_digests),
            "worktree_sha256": self.worktree_sha256,
            "verification_contract_sha256": self.verification_contract_sha256,
            "verification_results": [item.to_wire() for item in self.verification_results],
            "next_action": self.next_action,
            "forbidden_next_actions": list(self.forbidden_next_actions),
        }

    def to_wire(self) -> dict[str, object]:
        return {**self.unsigned_wire(), "checkpoint_id": self.checkpoint_id}

    @classmethod
    def create(
        cls,
        *,
        snapshot: CheckoutSnapshot,
        mission: MissionBinding,
        role: str,
        work_state: str,
        completed_step: str,
        owned_paths: tuple[str, ...],
        preserve_paths: tuple[str, ...],
        verification_contract_sha256: str,
        verification_results: tuple[VerificationResult, ...],
        next_action: str,
        forbidden_next_actions: tuple[str, ...],
    ) -> SessionCheckpoint:
        candidate = cls(
            schema_version=CHECKPOINT_SCHEMA,
            projection_authority=CHECKPOINT_AUTHORITY,
            mission_id=mission.mission_id,
            mission_state=mission.state,
            mission_source_path=mission.source_path,
            mission_sha256=mission.mission_sha256,
            accepted_main_sha=snapshot.accepted_main_sha,
            head_sha=snapshot.head_sha,
            branch=snapshot.branch,
            role=role,
            work_state=work_state,
            completed_step=completed_step,
            owned_paths=owned_paths,
            preserve_paths=preserve_paths,
            dirty_paths=snapshot.dirty_paths,
            path_digests=snapshot.path_digests,
            worktree_sha256=snapshot.worktree_sha256,
            verification_contract_sha256=verification_contract_sha256,
            verification_results=verification_results,
            next_action=next_action,
            forbidden_next_actions=forbidden_next_actions,
            checkpoint_id="",
        )
        return replace(candidate, checkpoint_id=_json_sha256(candidate.unsigned_wire()))

    @classmethod
    def from_wire(cls, value: object) -> SessionCheckpoint:
        if isinstance(value, cls):
            value = value.to_wire()
        wire = _wire_mapping(value, "checkpoint_fields_invalid")
        if set(wire) != {field.name for field in dataclass_fields(cls)}:
            raise SessionContextError("checkpoint_fields_invalid")
        if wire.get("schema_version") != CHECKPOINT_SCHEMA:
            raise SessionContextError("checkpoint_version_unsupported")
        if wire.get("projection_authority") != CHECKPOINT_AUTHORITY:
            raise SessionContextError("checkpoint_authority_invalid")
        mission_id = wire.get("mission_id")
        if not isinstance(mission_id, str) or not MISSION_ID.fullmatch(mission_id):
            raise SessionContextError("checkpoint_mission_invalid")
        mission_state = wire.get("mission_state")
        if not isinstance(mission_state, str) or mission_state not in EXECUTABLE_MISSION_STATES:
            raise SessionContextError("checkpoint_mission_state_invalid")
        if wire.get("mission_source_path") != "docs/AUTONOMY.md":
            raise SessionContextError("checkpoint_mission_source_invalid")
        mission_sha256 = _validate_sha(
            wire.get("mission_sha256"), "checkpoint_mission_sha256", SHA256
        )
        accepted_main_sha = _validate_sha(
            wire.get("accepted_main_sha"), "checkpoint_accepted_main_sha", SHA40
        )
        head_sha = _validate_sha(wire.get("head_sha"), "checkpoint_head_sha", SHA40)
        branch = _bounded_text(wire.get("branch"), "checkpoint_branch", max_chars=256)
        if branch.startswith("-"):
            raise SessionContextError("checkpoint_branch_invalid")
        role = wire.get("role")
        if role != "coding":
            raise SessionContextError("checkpoint_role_invalid")
        work_state = wire.get("work_state")
        if not isinstance(work_state, str) or work_state not in WORK_STATES:
            raise SessionContextError("checkpoint_work_state_invalid")
        completed_step = _bounded_text(
            wire.get("completed_step"), "checkpoint_completed_step"
        )
        owned_paths = tuple(_path_list(wire.get("owned_paths"), "checkpoint_owned_paths"))
        preserve_paths = tuple(
            _path_list(wire.get("preserve_paths"), "checkpoint_preserve_paths")
        )
        dirty_paths = tuple(_path_list(wire.get("dirty_paths"), "checkpoint_dirty_paths"))
        if set(owned_paths) & set(preserve_paths) or tuple(
            sorted((*owned_paths, *preserve_paths))
        ) != dirty_paths:
            raise SessionContextError("checkpoint_path_partition_invalid")
        raw_digests = wire.get("path_digests")
        if not isinstance(raw_digests, Mapping) or set(raw_digests) != set(dirty_paths):
            raise SessionContextError("checkpoint_path_digests_invalid")
        path_digests = tuple(
            (
                path,
                _validate_sha(
                    raw_digests[path], "checkpoint_path_digest", SHA256
                ),
            )
            for path in dirty_paths
        )
        worktree_sha256 = _validate_sha(
            wire.get("worktree_sha256"), "checkpoint_worktree_sha256", SHA256
        )
        verification_contract_sha256 = _validate_sha(
            wire.get("verification_contract_sha256"),
            "checkpoint_verification_contract_sha256",
            SHA256,
        )
        raw_results = wire.get("verification_results")
        if not isinstance(raw_results, list) or len(raw_results) > 100:
            raise SessionContextError("verification_results_invalid")
        verification_results = tuple(
            VerificationResult.from_wire(item) for item in raw_results
        )
        if len({item.check for item in verification_results}) != len(verification_results):
            raise SessionContextError("verification_results_invalid")
        _enforce_verification_invariant(work_state, verification_results)
        next_action = _bounded_text(wire.get("next_action"), "checkpoint_next_action")
        forbidden_next_actions = tuple(
            _bounded_string_list(
                wire.get("forbidden_next_actions"),
                "checkpoint_forbidden_next_actions",
            )
        )
        checkpoint_id = _validate_sha(wire.get("checkpoint_id"), "checkpoint_id", SHA256)
        model = cls(
            schema_version=CHECKPOINT_SCHEMA,
            projection_authority=CHECKPOINT_AUTHORITY,
            mission_id=mission_id,
            mission_state=mission_state,
            mission_source_path="docs/AUTONOMY.md",
            mission_sha256=mission_sha256,
            accepted_main_sha=accepted_main_sha,
            head_sha=head_sha,
            branch=branch,
            role=role,
            work_state=work_state,
            completed_step=completed_step,
            owned_paths=owned_paths,
            preserve_paths=preserve_paths,
            dirty_paths=dirty_paths,
            path_digests=path_digests,
            worktree_sha256=worktree_sha256,
            verification_contract_sha256=verification_contract_sha256,
            verification_results=verification_results,
            next_action=next_action,
            forbidden_next_actions=forbidden_next_actions,
            checkpoint_id=checkpoint_id,
        )
        if model.checkpoint_id != _json_sha256(model.unsigned_wire()):
            raise SessionContextError("checkpoint_digest_mismatch")
        return model


@dataclass(frozen=True)
class ResumeDisposition:
    schema_version: str
    authority: str
    disposition: str
    reason: str
    mission_id: str | None
    checkpoint_id: str | None
    next_permitted_action: str
    forbidden_next_actions: tuple[str, ...]

    def to_wire(self) -> dict[str, object]:
        return {
            "schema_version": self.schema_version,
            "authority": self.authority,
            "disposition": self.disposition,
            "reason": self.reason,
            "mission_id": self.mission_id,
            "checkpoint_id": self.checkpoint_id,
            "next_permitted_action": self.next_permitted_action,
            "forbidden_next_actions": list(self.forbidden_next_actions),
        }


@dataclass(frozen=True)
class SessionEntry:
    schema_version: str
    authority: str
    accepted_main_sha: str
    document_source: str
    document_source_binding: str
    checkout_snapshot_json: str
    role: str
    mission_id: str
    mission_state: str
    mission_sha256: str
    context_mode: str
    resume_disposition: str
    resume_reason: str
    checkpoint_json: str | None
    checkpoint_id: str | None
    next_permitted_action: str
    allowed_paths: tuple[str, ...]
    owned_paths: tuple[str, ...]
    forbidden_next_actions: tuple[str, ...]
    targeted_reads: tuple[str, ...]
    verification_commands: tuple[str, ...]
    verification_contract_sha256: str | None
    deferred_documents: tuple[str, ...]
    context_policy: str
    dispatch_capsule_json: str | None
    execution_authorized: bool
    checkpoint_allowed: bool
    checkpoint_write_commands_json: str | None
    entry_sha256: str

    def unsigned_wire(self) -> dict[str, object]:
        return {
            "schema_version": self.schema_version,
            "authority": self.authority,
            "accepted_main_sha": self.accepted_main_sha,
            "document_source": self.document_source,
            "document_source_binding": self.document_source_binding,
            "checkout_snapshot": json.loads(self.checkout_snapshot_json),
            "role": self.role,
            "mission_id": self.mission_id,
            "mission_state": self.mission_state,
            "mission_sha256": self.mission_sha256,
            "context_mode": self.context_mode,
            "resume_disposition": self.resume_disposition,
            "resume_reason": self.resume_reason,
            "checkpoint": (
                None if self.checkpoint_json is None else json.loads(self.checkpoint_json)
            ),
            "checkpoint_id": self.checkpoint_id,
            "next_permitted_action": self.next_permitted_action,
            "allowed_paths": list(self.allowed_paths),
            "owned_paths": list(self.owned_paths),
            "forbidden_next_actions": list(self.forbidden_next_actions),
            "targeted_reads": list(self.targeted_reads),
            "verification_commands": list(self.verification_commands),
            "verification_contract_sha256": self.verification_contract_sha256,
            "deferred_documents": list(self.deferred_documents),
            "context_policy": self.context_policy,
            "dispatch_capsule": (
                None
                if self.dispatch_capsule_json is None
                else json.loads(self.dispatch_capsule_json)
            ),
            "execution_authorized": self.execution_authorized,
            "checkpoint_allowed": self.checkpoint_allowed,
            "checkpoint_write_commands": (
                None
                if self.checkpoint_write_commands_json is None
                else json.loads(self.checkpoint_write_commands_json)
            ),
        }

    def to_wire(self) -> dict[str, object]:
        return {**self.unsigned_wire(), "entry_sha256": self.entry_sha256}

    @classmethod
    def create(cls, **values: object) -> SessionEntry:
        candidate = cls(entry_sha256="", **values)
        model = replace(
            candidate,
            entry_sha256=_json_sha256(candidate.unsigned_wire()),
        )
        if len(_canonical_json(model.to_wire()).encode("utf-8")) > MAX_ENTRY_BYTES:
            raise SessionContextError("session_entry_too_large")
        return model

    @classmethod
    def from_wire(cls, value: object) -> SessionEntry:
        wire = _wire_mapping(value, "session_entry_fields_invalid")
        internal_fields = {
            "checkout_snapshot_json",
            "checkpoint_json",
            "checkpoint_write_commands_json",
            "dispatch_capsule_json",
        }
        expected_fields = {
            *(field.name for field in dataclass_fields(cls) if field.name not in internal_fields),
            "checkout_snapshot",
            "checkpoint",
            "checkpoint_write_commands",
            "dispatch_capsule",
        }
        if set(wire) != expected_fields:
            raise SessionContextError("session_entry_fields_invalid")
        try:
            wire_size = len(_canonical_json(wire).encode("utf-8"))
        except (TypeError, ValueError) as exc:
            raise SessionContextError("session_entry_fields_invalid") from exc
        if wire_size > MAX_ENTRY_BYTES:
            raise SessionContextError("session_entry_too_large")
        if wire.get("schema_version") != ENTRY_SCHEMA:
            raise SessionContextError("session_entry_version_unsupported")
        if wire.get("authority") != ENTRY_AUTHORITY:
            raise SessionContextError("session_entry_authority_invalid")
        accepted_main_sha = _validate_sha(
            wire.get("accepted_main_sha"), "session_entry_accepted_main_sha", SHA40
        )
        document_source = wire.get("document_source")
        document_source_binding = wire.get("document_source_binding")
        if document_source == "accepted":
            if document_source_binding != accepted_main_sha:
                raise SessionContextError("session_entry_source_binding_invalid")
        elif document_source == "working-tree":
            if document_source_binding != "working_tree_unaccepted":
                raise SessionContextError("session_entry_source_binding_invalid")
        else:
            raise SessionContextError("session_entry_source_invalid")
        checkout_snapshot = CheckoutSnapshot.from_wire(wire.get("checkout_snapshot"))
        if checkout_snapshot.accepted_main_sha != accepted_main_sha:
            raise SessionContextError("session_entry_checkout_binding_invalid")
        role = wire.get("role")
        if not isinstance(role, str) or role not in ROLES:
            raise SessionContextError("session_entry_role_invalid")
        mission_id = wire.get("mission_id")
        if not isinstance(mission_id, str) or not MISSION_ID.fullmatch(mission_id):
            raise SessionContextError("session_entry_mission_invalid")
        mission_state = _bounded_text(
            wire.get("mission_state"), "session_entry_mission_state", max_chars=64
        )
        if not re.fullmatch(r"[A-Z0-9_]+", mission_state):
            raise SessionContextError("session_entry_mission_state_invalid")
        mission_sha256 = _validate_sha(
            wire.get("mission_sha256"), "session_entry_mission_sha256", SHA256
        )
        context_mode = wire.get("context_mode")
        if context_mode not in ENTRY_CONTEXT_MODES:
            raise SessionContextError("session_entry_context_mode_invalid")
        resume_disposition = wire.get("resume_disposition")
        if resume_disposition not in {"RESUME", "REPAIR", "DECISION_REQUIRED"}:
            raise SessionContextError("session_entry_disposition_invalid")
        resume_reason = _bounded_text(
            wire.get("resume_reason"), "session_entry_resume_reason", max_chars=128
        )
        checkpoint_value = wire.get("checkpoint_id")
        checkpoint_id = (
            None
            if checkpoint_value is None
            else _validate_sha(checkpoint_value, "session_entry_checkpoint_id", SHA256)
        )
        raw_checkpoint = wire.get("checkpoint")
        checkpoint = (
            None
            if raw_checkpoint is None
            else SessionCheckpoint.from_wire(raw_checkpoint)
        )
        if (checkpoint is None) != (checkpoint_id is None) or (
            checkpoint is not None and checkpoint.checkpoint_id != checkpoint_id
        ):
            raise SessionContextError("session_entry_checkpoint_binding_invalid")
        next_permitted_action = _bounded_text(
            wire.get("next_permitted_action"), "session_entry_next_action"
        )
        raw_allowed = wire.get("allowed_paths")
        if not isinstance(raw_allowed, list):
            raise SessionContextError("session_entry_allowed_paths_invalid")
        allowed_paths = tuple(
            sorted({_repo_path(item, "session_entry_allowed_path") for item in raw_allowed})
        )
        if list(allowed_paths) != raw_allowed:
            raise SessionContextError("session_entry_allowed_paths_invalid")
        owned_paths = tuple(
            _path_list(wire.get("owned_paths"), "session_entry_owned_paths")
        )
        if any(
            not _path_is_allowed(path, list(allowed_paths)) for path in owned_paths
        ):
            raise SessionContextError("session_entry_owned_path_not_allowed")
        forbidden_next_actions = tuple(
            _bounded_string_list(
                wire.get("forbidden_next_actions"),
                "session_entry_forbidden_next_actions",
                allow_empty=True,
            )
        )
        targeted_reads = tuple(
            _bounded_string_list(
                wire.get("targeted_reads"),
                "session_entry_targeted_reads",
                allow_empty=True,
                max_items=50,
            )
        )
        verification_commands = tuple(
            _bounded_string_list(
                wire.get("verification_commands"),
                "session_entry_verification_commands",
                allow_empty=True,
                max_items=50,
            )
        )
        raw_deferred = wire.get("deferred_documents")
        if not isinstance(raw_deferred, list) or len(raw_deferred) > MAX_ROUTE_DOCUMENTS:
            raise SessionContextError("session_entry_deferred_documents_invalid")
        deferred_documents = tuple(
            _repo_path(item, "session_entry_deferred_document", allow_directory=False)
            for item in raw_deferred
        )
        if (
            len(deferred_documents) != len(set(deferred_documents))
            or not deferred_documents
            or deferred_documents[0] != "START_HERE.md"
            or any(path not in CANONICAL_DOCUMENTS for path in deferred_documents)
        ):
            raise SessionContextError("session_entry_deferred_documents_invalid")
        context_policy = _bounded_text(
            wire.get("context_policy"), "session_entry_context_policy"
        )
        raw_capsule = wire.get("dispatch_capsule")
        capsule = (
            None
            if raw_capsule is None
            else _canonical_dispatch_capsule(raw_capsule)
        )
        if capsule is None and context_mode != "STOP":
            raise SessionContextError("session_entry_dispatch_binding_invalid")
        verification_contract_sha256 = (
            None
            if wire.get("verification_contract_sha256") is None
            else _validate_sha(
                wire.get("verification_contract_sha256"),
                "session_entry_verification_contract_sha256",
                SHA256,
            )
        )
        if capsule is not None:
            capsule_allowed = sorted(
                {
                    _repo_path(item, "dispatch_allowed_path")
                    for item in capsule.get("allowed_paths", [])
                }
            )
            capsule_forbidden = _bounded_string_list(
                capsule.get("forbidden_next_actions"),
                "dispatch_forbidden_next_actions",
                allow_empty=True,
            )
            if (
                capsule.get("mission_id") != mission_id
                or capsule_allowed != list(allowed_paths)
                or capsule_forbidden != list(forbidden_next_actions)
            ):
                raise SessionContextError("session_entry_dispatch_binding_invalid")
            expected_contract = _verification_contract_sha256(
                mission_id, mission_sha256, capsule["verification"]
            )
            if verification_contract_sha256 != expected_contract:
                raise SessionContextError(
                    "session_entry_verification_contract_invalid"
                )
        elif verification_contract_sha256 is not None:
            raise SessionContextError("session_entry_verification_contract_invalid")
        execution_authorized = wire.get("execution_authorized")
        checkpoint_allowed = wire.get("checkpoint_allowed")
        if not isinstance(execution_authorized, bool) or not isinstance(
            checkpoint_allowed, bool
        ):
            raise SessionContextError("session_entry_authority_flags_invalid")
        raw_checkpoint_commands = wire.get("checkpoint_write_commands")
        checkpoint_write_commands = None
        if raw_checkpoint_commands is not None:
            if not isinstance(raw_checkpoint_commands, Mapping) or set(
                raw_checkpoint_commands
            ) != {"wip", "stable"}:
                raise SessionContextError("session_entry_checkpoint_commands_invalid")
            checkpoint_write_commands = {
                "wip": _bounded_text(
                    raw_checkpoint_commands.get("wip"),
                    "session_entry_checkpoint_command",
                ),
                "stable": (
                    None
                    if raw_checkpoint_commands.get("stable") is None
                    else _bounded_text(
                        raw_checkpoint_commands.get("stable"),
                        "session_entry_checkpoint_command",
                    )
                ),
            }
            if any(
                character in command
                for command in checkpoint_write_commands.values()
                if command is not None
                for character in "<>\n\r"
            ):
                raise SessionContextError("session_entry_checkpoint_commands_invalid")
        expected_disposition = {
            "FRESH_MISSION": "RESUME",
            "RESUME_CHECKPOINT": "RESUME",
            "REPAIR": "REPAIR",
            "STOP": "DECISION_REQUIRED",
        }[context_mode]
        if resume_disposition != expected_disposition:
            raise SessionContextError("session_entry_mode_invalid")
        if context_mode != "STOP" and mission_state not in EXECUTABLE_MISSION_STATES:
            raise SessionContextError("session_entry_mode_invalid")
        checkout_dirty_paths = set(checkout_snapshot.dirty_paths)
        if any(path not in checkout_dirty_paths for path in owned_paths):
            raise SessionContextError("session_entry_owned_path_not_in_checkout")
        if checkpoint is None:
            if owned_paths:
                raise SessionContextError("session_entry_checkpoint_binding_invalid")
        elif (
            checkpoint.accepted_main_sha != accepted_main_sha
            or checkpoint.mission_id != mission_id
            or checkpoint.mission_state != mission_state
            or checkpoint.mission_sha256 != mission_sha256
            or checkpoint.owned_paths != owned_paths
        ):
            raise SessionContextError("session_entry_checkpoint_binding_invalid")
        if context_mode == "FRESH_MISSION":
            expected_reads = capsule["read_paths"]
            if (
                checkpoint is not None
                or checkout_snapshot.branch != "main"
                or checkout_snapshot.head_sha != accepted_main_sha
                or checkout_snapshot.dirty_paths
            ):
                raise SessionContextError("session_entry_checkout_mode_invalid")
        elif context_mode == "RESUME_CHECKPOINT":
            expected_reads = list(owned_paths)
            if checkpoint is None:
                raise SessionContextError("session_entry_mode_invalid")
        elif context_mode == "REPAIR":
            expected_reads = []
            if checkpoint is None:
                raise SessionContextError("session_entry_mode_invalid")
        else:
            expected_reads = []
        if list(targeted_reads) != expected_reads:
            raise SessionContextError("session_entry_targeted_reads_invalid")
        if capsule is not None and list(verification_commands) != capsule["verification"]:
            raise SessionContextError("session_entry_verification_invalid")
        if (
            (document_source == "working-tree" and context_mode != "STOP")
            or (context_mode == "STOP" and (execution_authorized or checkpoint_allowed))
            or (context_mode != "STOP" and role == "coding" and not checkpoint_allowed)
            or (role != "coding" and checkpoint_allowed)
            or (
                execution_authorized
                and context_mode not in {"FRESH_MISSION", "RESUME_CHECKPOINT"}
            )
        ):
            raise SessionContextError("session_entry_authority_flags_invalid")
        expected_checkpoint_commands = _checkpoint_write_commands(
            role=role,
            mission_id=mission_id,
            context_mode=context_mode,
            checkpoint_allowed=checkpoint_allowed,
            verification_commands=verification_commands,
        )
        if checkpoint_write_commands != expected_checkpoint_commands:
            raise SessionContextError("session_entry_checkpoint_commands_invalid")
        if document_source == "accepted" and context_mode != "STOP":
            assert capsule is not None
            mission_projection = MissionBinding.from_wire(
                {
                    "mission_id": mission_id,
                    "state": mission_state,
                    "source_path": "docs/AUTONOMY.md",
                    "mission_sha256": mission_sha256,
                    "allowed_paths": list(allowed_paths),
                    "forbidden_next_actions": list(forbidden_next_actions),
                    "execution_authorized": execution_authorized,
                    "checkpoint_allowed": True,
                    "dispatch_lane": capsule["dispatch_lane"],
                },
                require_checkpoint=True,
            )
            rebuilt = classify_resume(
                checkpoint,
                snapshot=checkout_snapshot,
                mission=mission_projection,
                dispatch_capsule=capsule,
            )
            if (
                rebuilt["disposition"] != resume_disposition
                or rebuilt["reason"] != resume_reason
                or rebuilt["checkpoint_id"] != checkpoint_id
                or rebuilt["next_permitted_action"] != next_permitted_action
                or rebuilt["forbidden_next_actions"]
                != list(forbidden_next_actions)
            ):
                raise SessionContextError("session_entry_recovery_binding_invalid")
        entry_sha256 = _validate_sha(
            wire.get("entry_sha256"), "session_entry_sha256", SHA256
        )
        model = cls(
            schema_version=ENTRY_SCHEMA,
            authority=ENTRY_AUTHORITY,
            accepted_main_sha=accepted_main_sha,
            document_source=document_source,
            document_source_binding=document_source_binding,
            checkout_snapshot_json=_canonical_json(checkout_snapshot.to_wire()),
            role=role,
            mission_id=mission_id,
            mission_state=mission_state,
            mission_sha256=mission_sha256,
            context_mode=context_mode,
            resume_disposition=resume_disposition,
            resume_reason=resume_reason,
            checkpoint_json=(
                None if checkpoint is None else _canonical_json(checkpoint.to_wire())
            ),
            checkpoint_id=checkpoint_id,
            next_permitted_action=next_permitted_action,
            allowed_paths=allowed_paths,
            owned_paths=owned_paths,
            forbidden_next_actions=forbidden_next_actions,
            targeted_reads=targeted_reads,
            verification_commands=verification_commands,
            verification_contract_sha256=verification_contract_sha256,
            deferred_documents=deferred_documents,
            context_policy=context_policy,
            dispatch_capsule_json=(
                None if capsule is None else _canonical_json(capsule)
            ),
            execution_authorized=execution_authorized,
            checkpoint_allowed=checkpoint_allowed,
            checkpoint_write_commands_json=(
                None
                if checkpoint_write_commands is None
                else _canonical_json(checkpoint_write_commands)
            ),
            entry_sha256=entry_sha256,
        )
        if model.entry_sha256 != _json_sha256(model.unsigned_wire()):
            raise SessionContextError("session_entry_digest_mismatch")
        return model


def parse_route_contract(
    document: str,
) -> RouteContract:
    """Parse the sole machine-readable role router from ``START_HERE.md``.

    Only the seven canonical documents are valid route members. Legacy
    document paths are rejected so accepted-main navigation cannot drift back
    to the removed control-plane contract.
    """

    allowed_documents = CANONICAL_DOCUMENTS

    if not isinstance(document, str) or len(document.encode("utf-8")) > MAX_ROUTE_DOCUMENT_BYTES:
        raise SessionContextError("route_document_unavailable_or_too_large")
    markers = list(ROUTE_MARKER.finditer(document))
    if len(markers) != 1:
        raise SessionContextError("route_contract_missing_or_duplicated")
    try:
        payload = json.loads(markers[0].group("payload"))
    except json.JSONDecodeError as exc:
        raise SessionContextError("route_contract_json_invalid") from exc
    if not isinstance(payload, dict) or set(payload) != {
        "schema_version",
        "max_required_documents",
        "roles",
    }:
        raise SessionContextError("route_contract_fields_invalid")
    if payload["schema_version"] != ROUTE_SCHEMA:
        raise SessionContextError("route_contract_version_unsupported")
    maximum = payload["max_required_documents"]
    if isinstance(maximum, bool) or not isinstance(maximum, int) or not 1 <= maximum <= 6:
        raise SessionContextError("route_contract_limit_invalid")
    roles = payload["roles"]
    if not isinstance(roles, dict) or set(roles) != ROLES:
        raise SessionContextError("route_contract_roles_invalid")
    normalized_roles: list[tuple[str, RouteRole]] = []
    for role in sorted(ROLES):
        route = roles[role]
        if not isinstance(route, dict) or set(route) != {"required", "optional"}:
            raise SessionContextError("route_contract_role_fields_invalid")
        required = route["required"]
        optional = route["optional"]
        if not isinstance(required, list) or not required or any(
            not isinstance(path, str) for path in required
        ):
            raise SessionContextError("route_contract_required_invalid")
        normalized_required = list(dict.fromkeys(required))
        if (
            len(normalized_required) > maximum
            or normalized_required[0] != "START_HERE.md"
            or any(path not in allowed_documents for path in normalized_required)
        ):
            raise SessionContextError("route_contract_required_invalid")
        if not isinstance(optional, dict) or len(optional) > MAX_ROUTE_DOCUMENTS:
            raise SessionContextError("route_contract_optional_invalid")
        normalized_optional: dict[str, str] = {}
        for option, path in optional.items():
            normalized_path = path
            if (
                not isinstance(option, str)
                or not OPTION_ID.fullmatch(option)
                or normalized_path not in allowed_documents
            ):
                raise SessionContextError("route_contract_optional_invalid")
            if normalized_path not in normalized_required:
                if normalized_path in normalized_optional.values():
                    raise SessionContextError("route_contract_optional_invalid")
                normalized_optional[option] = normalized_path
        normalized_roles.append(
            (
                role,
                RouteRole(
                    required=tuple(normalized_required),
                    optional=tuple(sorted(normalized_optional.items())),
                ),
            )
        )
    return RouteContract(
        schema_version=ROUTE_SCHEMA,
        max_required_documents=maximum,
        roles=tuple(normalized_roles),
    )


def build_route(
    contract: RouteContract,
    *,
    role: str,
    accepted_main_sha: str,
    mission: object,
    include: list[str] | None = None,
) -> dict[str, object]:
    """Return one bounded document manifest for a known repository role."""

    _validate_sha(accepted_main_sha, "accepted_main_sha", SHA40)
    if not isinstance(contract, RouteContract) or role not in ROLES:
        raise SessionContextError("role_unsupported")
    mission_model = MissionBinding.from_wire(mission)
    selected = include or []
    if len(selected) != len(set(selected)):
        raise SessionContextError("route_option_duplicated")
    route = contract.role_for(role)
    optional = route.option_map()
    unknown = [option for option in selected if option not in optional]
    if unknown:
        raise SessionContextError("route_option_unsupported")
    documents = (*route.required, *(optional[option] for option in selected))
    if (
        len(documents) > contract.max_required_documents
        or len(documents) != len(set(documents))
    ):
        raise SessionContextError("route_document_limit_exceeded")
    return ContextRoute(
        schema_version="agent_context_route.v1",
        authority="accepted_documents_select_context; route_grants_no_execution_authority",
        accepted_main_sha=accepted_main_sha,
        role=role,
        mission_id=mission_model.mission_id,
        mission_state=mission_model.state,
        mission_sha256=mission_model.mission_sha256,
        documents=tuple(documents),
        included_options=tuple(selected),
        execution_authorized=False,
        checkpoint_allowed=False,
        bootstrap_order=(
            "read the returned documents in order",
            "run scripts/project_context.py and verify accepted main/live frontier",
            "run session_context.py resume before touching an existing worktree",
            "stop on any DECISION_REQUIRED disposition",
        ),
    ).to_wire()


def _verification_models(value: object) -> tuple[VerificationResult, ...]:
    if not isinstance(value, list) or len(value) > 100:
        raise SessionContextError("verification_results_invalid")
    results: list[VerificationResult] = []
    seen: set[str] = set()
    for item in value:
        result = VerificationResult.from_wire(item)
        if result.check in seen:
            raise SessionContextError("verification_results_invalid")
        seen.add(result.check)
        results.append(result)
    return tuple(results)


def _verification_results(value: object) -> list[dict[str, str]]:
    return [item.to_wire() for item in _verification_models(value)]


def _verification_contract_sha256(
    mission_id: str, mission_sha256: str, verification: list[str]
) -> str:
    """Bind the checkpoint's verification evidence to one canonical contract.

    The contract digest is computed from the accepted dispatch capsule's exact
    ordered verification commands plus the bound mission identity. A checkpoint
    produced under a different required set, order, or mission cannot match the
    digest, so a rehashed or substituted evidence set is never silently
    accepted as stable proof.
    """

    ordered = _bounded_string_list(verification, "verification", max_items=50)
    return _json_sha256(
        {
            "schema_version": CHECKPOINT_SCHEMA,
            "mission_id": mission_id,
            "mission_sha256": mission_sha256,
            "verification": ordered,
        }
    )


def _enforce_verification_invariant(
    work_state: str, results: tuple[VerificationResult, ...]
) -> None:
    """Enforce the single fail-closed work_state/verification-state invariant.

    WIP may carry only NOT_RUN results (plus recorded FAIL results, which can
    only drive REPAIR) and never PASS: partial results are not acceptance
    evidence. STABLE requires a non-empty result set of exactly PASS: no
    NOT_RUN, no FAIL, no BLOCKED, no missing evidence. BLOCKED and
    OUTCOME_UNKNOWN carry nothing consumable. Caller-supplied PASS can never
    create a trusted stable path.
    """

    statuses = {item.status for item in results}
    if work_state == "WIP":
        if statuses - {"NOT_RUN", "FAIL"}:
            raise SessionContextError("wip_verification_pass_invalid")
        return
    if work_state == "STABLE":
        if not results or statuses != {"PASS"}:
            raise SessionContextError("stable_verification_incomplete")
        return
    if work_state in {"BLOCKED", "OUTCOME_UNKNOWN"}:
        if results and statuses != {"NOT_RUN"}:
            raise SessionContextError("blocked_verification_invalid")
        return
    raise SessionContextError("work_state_invalid")


def _build_checkpoint(
    *,
    snapshot: object,
    mission: object,
    role: str,
    work_state: str,
    completed_step: str,
    owned_paths: list[str],
    verification_commands: list[str],
    verification_results: list[dict[str, str]],
    next_action: str,
    forbidden_next_actions: list[str],
) -> dict[str, object]:
    """Build a digest-bound handoff for the exact current worktree."""

    snapshot_model = CheckoutSnapshot.from_wire(snapshot)
    mission_model = MissionBinding.from_wire(mission, require_checkpoint=True)
    if role != "coding":
        raise SessionContextError("checkpoint_role_invalid")
    if work_state not in WORK_STATES:
        raise SessionContextError("work_state_invalid")
    dirty_paths = list(snapshot_model.dirty_paths)
    owned = sorted({_repo_path(path, "owned_path", allow_directory=False) for path in owned_paths})
    if not set(owned).issubset(dirty_paths):
        raise SessionContextError("owned_path_not_dirty")
    allowed = list(mission_model.allowed_paths)
    if any(not _path_is_allowed(path, allowed) for path in owned):
        raise SessionContextError("owned_path_not_allowed")
    preserve = sorted(set(dirty_paths) - set(owned))
    checks = _verification_models(verification_results)
    required = _bounded_string_list(
        verification_commands, "verification_commands", max_items=50
    )
    if [item.check for item in checks] != required:
        raise SessionContextError("verification_results_contract_mismatch")
    _enforce_verification_invariant(work_state, checks)
    forbidden = tuple(
        _bounded_string_list(forbidden_next_actions, "forbidden_next_actions")
    )
    receipt = SessionCheckpoint.create(
        snapshot=snapshot_model,
        mission=mission_model,
        role=role,
        work_state=work_state,
        completed_step=_bounded_text(completed_step, "completed_step"),
        owned_paths=tuple(owned),
        preserve_paths=tuple(preserve),
        verification_contract_sha256=_verification_contract_sha256(
            mission_model.mission_id, mission_model.mission_sha256, required
        ),
        verification_results=checks,
        next_action=_bounded_text(next_action, "next_action"),
        forbidden_next_actions=forbidden,
    )
    return SessionCheckpoint.from_wire(receipt.to_wire()).to_wire()


def build_auto_checkpoint(
    *,
    snapshot: object,
    mission: object,
    dispatch_capsule: object,
    role: str,
) -> dict[str, object]:
    """Build a fixed-text WIP checkpoint from accepted mission and checkout facts."""

    snapshot_model = CheckoutSnapshot.from_wire(snapshot)
    mission_model = MissionBinding.from_wire(mission, require_checkpoint=True)
    capsule = _canonical_dispatch_capsule(dispatch_capsule)
    _bind_dispatch_capsule(mission_model, capsule)
    if role != "coding":
        raise SessionContextError("checkpoint_auto_role_invalid")
    return _build_auto_checkpoint(
        snapshot_model=snapshot_model,
        mission_model=mission_model,
        capsule=capsule,
        role=role,
        work_state="WIP",
        verification_results=[
            {"check": check, "status": "NOT_RUN"}
            for check in capsule["verification"]
        ],
    )


def build_stable_auto_checkpoint(
    *,
    snapshot: object,
    mission: object,
    dispatch_capsule: object,
    role: str,
) -> dict[str, object]:
    """Run every safe declared check and build STABLE only after exact success.

    The passed snapshot is the pre-verification capture. Every required
    verification command must return zero, and a fresh post-verification
    capture must prove that the accepted main, head, branch, dirty-path set,
    per-path digests, and worktree are unchanged while the commands ran. Any
    drift means another agent or user changed the protected subjects mid-run:
    no STABLE checkpoint is written and the caller must re-verify.
    """

    snapshot_model = CheckoutSnapshot.from_wire(snapshot)
    mission_model = MissionBinding.from_wire(mission, require_checkpoint=True)
    capsule = _canonical_dispatch_capsule(dispatch_capsule)
    _bind_dispatch_capsule(mission_model, capsule)
    if role != "coding":
        raise SessionContextError("checkpoint_auto_role_invalid")
    parsed = [_safe_verification_argv(check) for check in capsule["verification"]]
    if any(argv is None for argv in parsed):
        raise SessionContextError("checkpoint_auto_verification_not_executable")
    results: list[dict[str, str]] = []
    for check, argv in zip(capsule["verification"], parsed, strict=True):
        assert argv is not None
        completed = subprocess.run(
            list(argv),
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        if completed.returncode != 0:
            raise SessionContextError("checkpoint_auto_verification_failed")
        results.append({"check": check, "status": "PASS"})
    after = capture_checkout(snapshot_model.accepted_main_sha)
    if CheckoutSnapshot.from_wire(after) != snapshot_model:
        raise SessionContextError("checkout_changed_during_verification")
    return _build_auto_checkpoint(
        snapshot_model=snapshot_model,
        mission_model=mission_model,
        capsule=capsule,
        role=role,
        work_state="STABLE",
        verification_results=results,
    )


def _build_auto_checkpoint(
    *,
    snapshot_model: CheckoutSnapshot,
    mission_model: MissionBinding,
    capsule: dict[str, object],
    role: str,
    work_state: str,
    verification_results: list[dict[str, str]],
) -> dict[str, object]:
    owned_paths = [
        path
        for path in snapshot_model.dirty_paths
        if _path_is_allowed(path, list(mission_model.allowed_paths))
    ]
    if not owned_paths:
        raise SessionContextError("checkpoint_auto_no_owned_paths")
    terminal = work_state == "STABLE"
    return _build_checkpoint(
        snapshot=snapshot_model.to_wire(),
        mission=mission_model.to_wire(),
        role=role,
        work_state=work_state,
        completed_step=(
            f"{mission_model.mission_id} implementation complete."
            if terminal
            else f"Captured bounded WIP for {mission_model.mission_id}; completion not asserted."
        ),
        owned_paths=owned_paths,
        verification_commands=list(capsule["verification"]),
        verification_results=verification_results,
        next_action=(
            "No permitted next mission; terminal STABLE checkpoint."
            if terminal
            else "Inspect only the checkpoint-owned paths, then continue the earliest "
            "incomplete ordered step from the bound dispatch capsule."
        ),
        forbidden_next_actions=list(mission_model.forbidden_next_actions),
    )


def validate_checkpoint(receipt: object) -> dict[str, object]:
    """Validate one checkpoint without trusting its claimed disposition."""

    return SessionCheckpoint.from_wire(receipt).to_wire()


def _disposition(
    receipt: SessionCheckpoint | None,
    mission: MissionBinding,
    disposition: str,
    reason: str,
    action: str,
) -> dict[str, object]:
    if disposition not in {"RESUME", "REPAIR", "DECISION_REQUIRED"}:
        raise SessionContextError("resume_disposition_invalid")
    return ResumeDisposition(
        schema_version="agent_session_resume.v1",
        authority="recovery_projection_only",
        disposition=disposition,
        reason=reason,
        mission_id=mission.mission_id,
        checkpoint_id=receipt.checkpoint_id if receipt else None,
        next_permitted_action=action,
        forbidden_next_actions=(
            receipt.forbidden_next_actions
            if receipt
            else mission.forbidden_next_actions
        ),
    ).to_wire()


def classify_resume(
    receipt: object | None,
    *,
    snapshot: object,
    mission: object,
    dispatch_capsule: object | None = None,
) -> dict[str, object]:
    """Classify exact recovery as RESUME, REPAIR, or DECISION_REQUIRED.

    ``dispatch_capsule`` must be the current accepted dispatch contract for
    the bound mission. Its ordered verification set defines the only evidence
    that can support a STABLE boundary; a checkpoint whose verification
    contract, exact result set, or work-state invariant does not match is
    rejected rather than silently adapted.
    """

    snapshot_model = CheckoutSnapshot.from_wire(snapshot)
    try:
        mission_model = MissionBinding.from_wire(mission, require_checkpoint=True)
    except SessionContextError:
        try:
            mission_projection = MissionBinding.from_wire(mission)
        except SessionContextError:
            mission_projection = None
        try:
            receipt_projection = (
                SessionCheckpoint.from_wire(receipt) if receipt is not None else None
            )
        except SessionContextError:
            receipt_projection = None
        return ResumeDisposition(
            schema_version="agent_session_resume.v1",
            authority="recovery_projection_only",
            disposition="DECISION_REQUIRED",
            reason="mission_not_executable",
            mission_id=mission_projection.mission_id if mission_projection else None,
            checkpoint_id=(
                receipt_projection.checkpoint_id if receipt_projection else None
            ),
            next_permitted_action=(
                "Refresh accepted planning authority; do not continue the prior work."
            ),
            forbidden_next_actions=(
                receipt_projection.forbidden_next_actions
                if receipt_projection
                else (
                    mission_projection.forbidden_next_actions
                    if receipt is None and mission_projection
                    else ()
                )
            ),
        ).to_wire()
    if receipt is None:
        if (
            not snapshot_model.dirty_paths
            and snapshot_model.head_sha == snapshot_model.accepted_main_sha
            and snapshot_model.branch == "main"
        ):
            return _disposition(
                None,
                mission_model,
                "RESUME",
                "clean_accepted_baseline",
                "Enter the current mission only through its authorized dispatch lane.",
            )
        return _disposition(
            None,
            mission_model,
            "DECISION_REQUIRED",
            "checkpoint_missing_for_noncanonical_checkout",
            "Identify the owner of the existing branch/WIP before changing any file.",
        )
    receipt_model = SessionCheckpoint.from_wire(receipt)
    if receipt_model.work_state == "OUTCOME_UNKNOWN":
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "checkpoint_outcome_unknown",
            "Resolve external-effect status with the named authority owner.",
        )
    if receipt_model.work_state == "BLOCKED":
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "checkpoint_blocked",
            "Resolve the recorded blocker before resuming implementation.",
        )
    if snapshot_model.accepted_main_sha != receipt_model.accepted_main_sha:
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "accepted_main_changed",
            "Rebase the plan through the canonical planning owner; do not reuse this receipt.",
        )
    if (
        mission_model.mission_id != receipt_model.mission_id
        or mission_model.mission_sha256 != receipt_model.mission_sha256
        or mission_model.source_path != receipt_model.mission_source_path
    ):
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "mission_binding_changed",
            "Refresh the mission contract and obtain a replacement checkpoint.",
        )
    if dispatch_capsule is None:
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "verification_contract_unavailable",
            "Refresh the accepted dispatch contract before resuming any work.",
        )
    capsule = _canonical_dispatch_capsule(dispatch_capsule)
    _bind_dispatch_capsule(mission_model, capsule)
    required = list(
        _bounded_string_list(capsule["verification"], "verification", max_items=50)
    )
    expected_contract = _verification_contract_sha256(
        mission_model.mission_id, mission_model.mission_sha256, required
    )
    if receipt_model.verification_contract_sha256 != expected_contract:
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "verification_contract_changed",
            "Refresh the checkpoint against the current accepted verification contract.",
        )
    if [item.check for item in receipt_model.verification_results] != required:
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "verification_evidence_invalid",
            "The checkpoint evidence set does not match the accepted verification contract.",
        )
    if snapshot_model.branch != receipt_model.branch:
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "branch_changed",
            "Return to the bound branch or obtain an owner-approved replacement handoff.",
        )
    current_paths = set(snapshot_model.dirty_paths)
    allowed = list(mission_model.allowed_paths)
    owned = set(receipt_model.owned_paths)
    if (
        any(not _path_is_allowed(path, allowed) for path in owned)
        or not owned.issubset(current_paths)
    ):
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "checkpoint_owned_paths_invalid",
            "Discard the forged or stale checkpoint and recover ownership before continuing.",
        )
    prior_paths = set(receipt_model.dirty_paths)
    preserve = set(receipt_model.preserve_paths)
    missing_preserve = preserve - current_paths
    if missing_preserve:
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "preserved_path_missing",
            "Recover or reconcile preserved user/other-agent work before continuing.",
        )
    snapshot_digests = snapshot_model.digest_map()
    receipt_digests = dict(receipt_model.path_digests)
    changed_preserve = {
        path
        for path in preserve
        if snapshot_digests.get(path) != receipt_digests.get(path)
    }
    if changed_preserve:
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "preserved_path_changed",
            "Identify the owner of changed preserved work before continuing.",
        )
    added = current_paths - prior_paths
    if any(not _path_is_allowed(path, allowed) for path in added):
        return _disposition(
            receipt_model,
            mission_model,
            "DECISION_REQUIRED",
            "unbound_dirty_paths",
            "Identify and bind the new dirty paths before changing the worktree.",
        )
    if added:
        return _disposition(
            receipt_model,
            mission_model,
            "REPAIR",
            "uncheckpointed_allowed_changes",
            "Audit the added in-scope changes, rerun focused checks, and replace the checkpoint.",
        )
    if snapshot_model.head_sha != receipt_model.head_sha:
        return _disposition(
            receipt_model,
            mission_model,
            "REPAIR",
            "exact_head_changed",
            "Audit the new exact head and replace the stale checkpoint before implementation.",
        )
    if snapshot_model.worktree_sha256 != receipt_model.worktree_sha256:
        return _disposition(
            receipt_model,
            mission_model,
            "REPAIR",
            "worktree_changed_within_bound_paths",
            "Audit the changed owned paths, rerun focused checks, and replace the checkpoint.",
        )
    if any(item.status == "FAIL" for item in receipt_model.verification_results):
        return _disposition(
            receipt_model,
            mission_model,
            "REPAIR",
            "verification_failed",
            "Repair the recorded failure within mission scope and replace the checkpoint.",
        )
    return _disposition(
        receipt_model,
        mission_model,
        "RESUME",
        "exact_checkpoint_match",
        receipt_model.next_action,
    )


def _canonical_dispatch_capsule(value: object) -> dict[str, object]:
    try:
        encoded = _canonical_json(value).encode("utf-8")
        capsule = json.loads(encoded)
    except (TypeError, ValueError, json.JSONDecodeError) as exc:
        raise SessionContextError("dispatch_capsule_invalid") from exc
    if len(encoded) > MAX_DISPATCH_CAPSULE_BYTES:
        raise SessionContextError("dispatch_capsule_too_large")
    if not isinstance(capsule, dict):
        raise SessionContextError("dispatch_capsule_invalid")
    if not set(capsule).issubset(DISPATCH_CAPSULE_FIELDS):
        raise SessionContextError("dispatch_capsule_fields_invalid")
    required = {
        "schema_version",
        "mission_id",
        "dispatch_lane",
        "external_effect_limit",
        "authority_consumption_allowed",
        "secret_values_allowed",
        "private_paths_allowed",
        "plan_lane_state",
        "allowed_paths",
        "forbidden_next_actions",
        "read_paths",
        "verification",
    }
    if not required.issubset(capsule):
        raise SessionContextError("dispatch_capsule_fields_invalid")
    promotion_fields = {
        "promotion_evidence_sha256",
        "route_manifest_sha256",
        "risk_class",
        "verification_family",
    }
    if set(capsule).intersection(promotion_fields) not in (set(), promotion_fields):
        raise SessionContextError("dispatch_promotion_evidence_fields_invalid")
    if capsule.get("schema_version") != "weak_agent_dispatch.v1":
        raise SessionContextError("dispatch_capsule_version_unsupported")
    if (
        type(capsule.get("external_effect_limit")) is not int
        or capsule.get("external_effect_limit") != 0
        or capsule.get("authority_consumption_allowed") is not False
        or capsule.get("secret_values_allowed") is not False
        or capsule.get("private_paths_allowed") is not False
        or capsule.get("plan_lane_state") not in {
            "plan_lane_deferred_until_terminal_owners",
            "plan_lane_active",
        }
    ):
        raise SessionContextError("dispatch_safety_contract_invalid")
    mission_id = capsule.get("mission_id")
    if not isinstance(mission_id, str) or not MISSION_ID.fullmatch(mission_id):
        raise SessionContextError("dispatch_capsule_mission_invalid")
    if promotion_fields.issubset(capsule):
        _validate_sha(
            capsule["promotion_evidence_sha256"],
            "dispatch_promotion_evidence_sha256",
            SHA256,
        )
        _validate_sha(
            capsule["route_manifest_sha256"],
            "dispatch_route_manifest_sha256",
            SHA256,
        )
        _bounded_text(capsule["risk_class"], "dispatch_risk_class", max_chars=128)
        _bounded_text(
            capsule["verification_family"],
            "dispatch_verification_family",
            max_chars=128,
        )
    _bounded_text(capsule.get("dispatch_lane"), "dispatch_lane", max_chars=128)
    allowed_paths = _dispatch_path_list(
        capsule.get("allowed_paths"), "dispatch_allowed_paths"
    )
    _bounded_string_list(
        capsule.get("forbidden_next_actions"),
        "dispatch_forbidden_next_actions",
        allow_empty=True,
    )
    read_paths = _dispatch_path_list(
        capsule.get("read_paths"), "dispatch_read_paths"
    )
    if not set(allowed_paths).issubset(read_paths):
        raise SessionContextError("dispatch_read_paths_scope_invalid")
    _bounded_string_list(
        capsule.get("verification"),
        "dispatch_verification",
        max_items=50,
    )
    return capsule


def _bind_dispatch_capsule(
    mission: MissionBinding, capsule: dict[str, object]
) -> None:
    """Require an executable capsule to preserve its mission's exact scope."""

    if (
        capsule.get("mission_id") != mission.mission_id
        or capsule.get("mission_state") != mission.state
        or mission.dispatch_lane is None
        or capsule.get("dispatch_lane") != mission.dispatch_lane
    ):
        raise SessionContextError("dispatch_binding_invalid")
    capsule_allowed = tuple(
        _dispatch_path_list(capsule["allowed_paths"], "dispatch_allowed_paths")
    )
    if capsule_allowed != mission.allowed_paths:
        raise SessionContextError("dispatch_scope_binding_invalid")
    capsule_forbidden = tuple(
        _bounded_string_list(
            capsule["forbidden_next_actions"],
            "dispatch_forbidden_next_actions",
            allow_empty=True,
        )
    )
    if capsule_forbidden != mission.forbidden_next_actions:
        raise SessionContextError("dispatch_forbidden_binding_invalid")


def build_session_entry(
    *,
    contract: RouteContract,
    role: str,
    accepted_main_sha: str,
    document_source: str,
    document_source_binding: str,
    mission: object,
    dispatch_capsule: object | None,
    snapshot: object,
    checkpoint: object | None,
) -> dict[str, object]:
    """Compose one bounded startup projection for fresh or resumed work.

    ``dispatch_capsule`` is None when the current mission is not executable
    (for example a planning-parked window); the entry then fails closed with a
    ``DECISION_REQUIRED`` disposition and no execution or checkpoint surface.
    """

    mission_model = MissionBinding.from_wire(mission)
    route = build_route(
        contract,
        role=role,
        accepted_main_sha=accepted_main_sha,
        mission=mission_model,
    )
    capsule = (
        None
        if dispatch_capsule is None
        else _canonical_dispatch_capsule(dispatch_capsule)
    )
    if capsule is not None and mission_model.checkpoint_allowed:
        _bind_dispatch_capsule(mission_model, capsule)
    snapshot_model = CheckoutSnapshot.from_wire(snapshot)
    checkpoint_model = (
        SessionCheckpoint.from_wire(checkpoint) if checkpoint is not None else None
    )
    disposition = classify_resume(
        checkpoint_model,
        snapshot=snapshot_model,
        mission=mission_model,
        dispatch_capsule=capsule,
    )
    source_accepted = (
        document_source == "accepted" and document_source_binding == accepted_main_sha
    )
    if not source_accepted:
        context_mode = "STOP"
        disposition = ResumeDisposition(
            schema_version="agent_session_resume.v1",
            authority="recovery_projection_only",
            disposition="DECISION_REQUIRED",
            reason="unaccepted_document_source",
            mission_id=mission_model.mission_id,
            checkpoint_id=None,
            next_permitted_action=(
                "Refresh the accepted document projection before changing any file."
            ),
            forbidden_next_actions=mission_model.forbidden_next_actions,
        ).to_wire()
    elif disposition["disposition"] == "RESUME":
        context_mode = (
            "RESUME_CHECKPOINT"
            if disposition["reason"] == "exact_checkpoint_match"
            else "FRESH_MISSION"
        )
    elif disposition["disposition"] == "REPAIR":
        context_mode = "REPAIR"
    else:
        context_mode = "STOP"
    if context_mode == "RESUME_CHECKPOINT" and checkpoint_model is not None:
        owned_paths = checkpoint_model.owned_paths
        targeted_reads = checkpoint_model.owned_paths
    elif context_mode == "FRESH_MISSION":
        assert capsule is not None
        owned_paths = ()
        targeted_reads = tuple(
            _bounded_string_list(
                capsule["read_paths"],
                "dispatch_read_paths",
                max_items=50,
            )
        )
    else:
        owned_paths = (
            checkpoint_model.owned_paths
            if source_accepted and checkpoint_model is not None
            else ()
        )
        targeted_reads = ()
    verification_commands = tuple(
        _bounded_string_list(
            capsule["verification"],
            "dispatch_verification",
            max_items=50,
        )
        if capsule is not None
        else []
    )
    verification_contract_sha256 = (
        None
        if capsule is None
        else _verification_contract_sha256(
            mission_model.mission_id, mission_model.mission_sha256, list(verification_commands)
        )
    )
    checkpoint_allowed = (
        source_accepted
        and role == "coding"
        and disposition["disposition"] in {"RESUME", "REPAIR"}
        and mission_model.checkpoint_allowed
    )
    entry = SessionEntry.create(
        schema_version=ENTRY_SCHEMA,
        authority=ENTRY_AUTHORITY,
        accepted_main_sha=accepted_main_sha,
        document_source=document_source,
        document_source_binding=document_source_binding,
        checkout_snapshot_json=_canonical_json(snapshot_model.to_wire()),
        role=role,
        mission_id=mission_model.mission_id,
        mission_state=mission_model.state,
        mission_sha256=mission_model.mission_sha256,
        context_mode=context_mode,
        resume_disposition=str(disposition["disposition"]),
        resume_reason=str(disposition["reason"]),
        checkpoint_json=(
            _canonical_json(checkpoint_model.to_wire())
            if source_accepted and checkpoint_model is not None
            else None
        ),
        checkpoint_id=(
            str(disposition["checkpoint_id"])
            if disposition["checkpoint_id"] is not None
            else None
        ),
        next_permitted_action=str(disposition["next_permitted_action"]),
        allowed_paths=mission_model.allowed_paths,
        owned_paths=owned_paths,
        forbidden_next_actions=tuple(disposition["forbidden_next_actions"]),
        targeted_reads=targeted_reads,
        verification_commands=verification_commands,
        verification_contract_sha256=verification_contract_sha256,
        deferred_documents=tuple(route["documents"]),
        context_policy=(
            "This digest-bound entry is the complete startup context. Do not reread "
            "deferred canonical documents unless this entry reports a conflict, a "
            "missing fact, or a stop condition."
        ),
        dispatch_capsule_json=(
            None if capsule is None else _canonical_json(capsule)
        ),
        execution_authorized=(
            source_accepted
            and disposition["disposition"] == "RESUME"
            and mission_model.execution_authorized
        ),
        checkpoint_allowed=checkpoint_allowed,
        checkpoint_write_commands_json=(
            None
            if not checkpoint_allowed
            else _canonical_json(
                _checkpoint_write_commands(
                    role=role,
                    mission_id=mission_model.mission_id,
                    context_mode=context_mode,
                    checkpoint_allowed=checkpoint_allowed,
                    verification_commands=verification_commands,
                )
            )
        ),
    )
    return SessionEntry.from_wire(entry.to_wire()).to_wire()


def _run_git(arguments: list[str], *, binary: bool = False) -> bytes | str:
    result = subprocess.run(
        ["git", *arguments],
        cwd=ROOT,
        capture_output=True,
        text=not binary,
        timeout=30,
        check=False,
    )
    if result.returncode != 0:
        raise SessionContextError("git_state_unavailable")
    return result.stdout


def _content_digest(path: Path) -> str:
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        return hashlib.sha256(b"missing").hexdigest()
    digest = hashlib.sha256()
    digest.update(f"mode:{stat.S_IFMT(metadata.st_mode):o}:{stat.S_IMODE(metadata.st_mode):o}\0".encode())
    if stat.S_ISREG(metadata.st_mode):
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
    elif stat.S_ISLNK(metadata.st_mode):
        digest.update(b"symlink\0")
        digest.update(os.readlink(path).encode("utf-8", "surrogateescape"))
    elif stat.S_ISDIR(metadata.st_mode):
        digest.update(b"directory\0")
    else:
        raise SessionContextError("unsupported_dirty_path_type")
    return digest.hexdigest()


def capture_checkout(accepted_main_sha: str) -> dict[str, object]:
    """Capture content-bound repository-relative WIP without rendering content."""

    _validate_sha(accepted_main_sha, "accepted_main_sha", SHA40)
    head = str(_run_git(["rev-parse", "--verify", "HEAD"])).strip()
    branch = str(_run_git(["symbolic-ref", "--short", "-q", "HEAD"])).strip()
    _validate_sha(head, "head_sha", SHA40)
    if not branch:
        raise SessionContextError("detached_or_invalid_branch")
    path_sets: list[set[str]] = []
    for arguments in (
        ["diff", "--name-only", "-z", "HEAD"],
        ["diff", "--cached", "--name-only", "-z"],
        ["ls-files", "--others", "--exclude-standard", "-z"],
    ):
        raw = _run_git(arguments, binary=True)
        assert isinstance(raw, bytes)
        values = {
            _repo_path(item.decode("utf-8", "surrogateescape"), "dirty_path", allow_directory=False)
            for item in raw.split(b"\0")
            if item
        }
        path_sets.append(values)
    dirty_paths = sorted(set().union(*path_sets))
    if len(dirty_paths) > MAX_DIRTY_PATHS:
        raise SessionContextError("dirty_path_limit_exceeded")
    path_digests = {path: _content_digest(ROOT / path) for path in dirty_paths}
    working_patch = _run_git(["diff", "--binary", "HEAD", "--"], binary=True)
    staged_patch = _run_git(["diff", "--cached", "--binary", "--"], binary=True)
    assert isinstance(working_patch, bytes) and isinstance(staged_patch, bytes)
    worktree_input = {
        "head_sha": head,
        "branch": branch,
        "path_digests": path_digests,
        "working_patch_sha256": hashlib.sha256(working_patch).hexdigest(),
        "staged_patch_sha256": hashlib.sha256(staged_patch).hexdigest(),
    }
    return CheckoutSnapshot(
        accepted_main_sha=accepted_main_sha,
        head_sha=head,
        branch=branch,
        detached=False,
        dirty_paths=tuple(dirty_paths),
        path_digests=tuple(sorted(path_digests.items())),
        worktree_sha256=_json_sha256(worktree_input),
    ).to_wire()


def _checkpoint_path() -> Path:
    raw = str(_run_git(["rev-parse", "--git-path", CHECKPOINT_BASENAME])).strip()
    if not raw:
        raise SessionContextError("checkpoint_path_unavailable")
    path = Path(raw)
    return path if path.is_absolute() else ROOT / path


def write_checkpoint(receipt: object) -> None:
    receipt_model = SessionCheckpoint.from_wire(receipt)
    encoded = (
        json.dumps(receipt_model.to_wire(), indent=2, sort_keys=True) + "\n"
    ).encode("utf-8")
    if len(encoded) > MAX_CHECKPOINT_BYTES:
        raise SessionContextError("checkpoint_too_large")
    destination = _checkpoint_path()
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary_name: str | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb", dir=destination.parent, prefix=".agent-handoff-", delete=False
        ) as handle:
            temporary_name = handle.name
            os.chmod(temporary_name, 0o600)
            handle.write(encoded)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary_name, destination)
        temporary_name = None
    finally:
        if temporary_name:
            try:
                os.unlink(temporary_name)
            except FileNotFoundError:
                pass


def read_checkpoint() -> dict[str, object] | None:
    path = _checkpoint_path()
    if not path.exists():
        return None
    try:
        raw = path.read_bytes()
    except OSError as exc:
        raise SessionContextError("checkpoint_unavailable") from exc
    if len(raw) > MAX_CHECKPOINT_BYTES:
        raise SessionContextError("checkpoint_too_large")
    try:
        value = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise SessionContextError("checkpoint_json_invalid") from exc
    return SessionCheckpoint.from_wire(value).to_wire()


def _load_documents(*, source: str, offline: bool) -> dict[str, Any]:
    project_context_path = ROOT / "scripts" / "project_context.py"
    spec = importlib.util.spec_from_file_location(
        "session_context_project_context", project_context_path
    )
    if spec is None or spec.loader is None:
        raise SessionContextError("project_context_unavailable")
    project_context = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = project_context
    try:
        spec.loader.exec_module(project_context)
    except Exception as exc:
        raise SessionContextError("project_context_unavailable") from exc
    baseline = project_context.accepted_baseline(offline=offline)
    sha = baseline.get("sha")
    if not isinstance(sha, str) or not SHA40.fullmatch(sha):
        raise SessionContextError("accepted_main_unavailable")
    if source == "accepted":
        if not project_context.ensure_commit_available(sha, offline=offline):
            raise SessionContextError("accepted_main_commit_unavailable")

        def reader(path: str) -> str:
            content = project_context.git_show_text(sha, path)
            return content

        source_binding = sha
    elif source == "working-tree":
        reader = lambda path: (ROOT / path).read_text(encoding="utf-8")
        source_binding = "working_tree_unaccepted"
    else:
        raise SessionContextError("document_source_invalid")
    documents = {path: reader(path) for path in CANONICAL_DOCUMENT_PATHS}
    if any(not value for value in documents.values()):
        raise SessionContextError("canonical_document_unavailable")
    return {
        "accepted_main_sha": sha,
        "accepted_main_source": baseline.get("source"),
        "document_source": source,
        "document_source_binding": source_binding,
        "documents": documents,
    }


def _canonical_session_mission(documents: Mapping[str, str]) -> dict[str, object]:
    """Represent the absence of an executable WorkCard without inventing one."""

    source_path = "docs/AUTONOMY.md"
    source = documents.get(source_path)
    if not isinstance(source, str) or not source:
        raise SessionContextError("canonical_document_unavailable")
    return MissionBinding(
        mission_id="CI-SESSION-ROUTE",
        state="NO_ACTIVE_STAGE",
        source_path=source_path,
        mission_sha256=hashlib.sha256(source.encode("utf-8")).hexdigest(),
        allowed_paths=CANONICAL_DOCUMENT_PATHS,
        forbidden_next_actions=(
            "Do not infer execution authority from canonical context.",
            "Do not continue without an accepted executable WorkCard.",
        ),
        execution_authorized=False,
        checkpoint_allowed=False,
        dispatch_lane=None,
    ).to_wire()


def _render_route(route: dict[str, Any]) -> str:
    lines = [
        "# Session Context Route",
        "",
        f"- Accepted main: `{route['accepted_main_sha']}`",
        f"- Role: `{route['role']}`",
        f"- Mission: `{route['mission_id']}` (`{route['mission_state']}`)",
        f"- Execution authorized by this route: `{str(route['execution_authorized']).lower()}`",
        "",
        "Read in this order:",
        "",
    ]
    lines.extend(f"{index}. `{path}`" for index, path in enumerate(route["documents"], 1))
    lines.extend(["", "Then:", ""])
    lines.extend(f"- {item}" for item in route["bootstrap_order"])
    return "\n".join(lines) + "\n"


def _print(value: dict[str, Any], output_format: str) -> None:
    if output_format == "json":
        print(json.dumps(value, indent=2, sort_keys=True))
    elif value.get("schema_version") == "agent_context_route.v1":
        print(_render_route(value), end="")
    else:
        print(json.dumps(value, indent=2, sort_keys=True))


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    route = subparsers.add_parser("route", help="Return the minimal accepted document route.")
    route.add_argument("--role", required=True, choices=sorted(ROLES))
    route.add_argument("--include", action="append", default=[])
    route.add_argument("--source", choices=("accepted", "working-tree"), default="accepted")
    route.add_argument("--offline", action="store_true")
    route.add_argument("--format", choices=("markdown", "json"), default="markdown")

    enter = subparsers.add_parser(
        "enter", help="Compose one bounded fresh-or-resume startup projection."
    )
    enter.add_argument("--role", choices=sorted(ROLES), default="coding")
    enter.add_argument("--source", choices=("accepted", "working-tree"), default="accepted")
    enter.add_argument("--offline", action="store_true")
    enter.add_argument("--format", choices=("json",), default="json")

    checkpoint_auto = subparsers.add_parser(
        "checkpoint-auto",
        help="Replace the local handoff from accepted mission and checkout facts.",
    )
    checkpoint_auto.add_argument("--role", choices=("coding",), default="coding")
    checkpoint_auto.add_argument("--mission", required=True)
    checkpoint_auto.add_argument("--work-state", choices=("WIP", "STABLE"), required=True)
    checkpoint_auto.add_argument("--verify", action="store_true")
    checkpoint_auto.add_argument("--offline", action="store_true")
    checkpoint_auto.add_argument("--no-write", action="store_true")

    resume = subparsers.add_parser("resume", help="Rebuild state and classify continuation.")
    resume.add_argument("--offline", action="store_true")
    resume.add_argument("--format", choices=("json",), default="json")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        source = getattr(args, "source", "accepted")
        loaded = _load_documents(source=source, offline=args.offline)
        accepted_main_sha = loaded["accepted_main_sha"]
        documents = loaded["documents"]
        mission = _canonical_session_mission(documents)

        if args.command == "route":
            contract = parse_route_contract(
                documents["START_HERE.md"],
            )
            value = build_route(
                contract,
                role=args.role,
                accepted_main_sha=accepted_main_sha,
                mission=mission,
                include=args.include,
            )
            value["document_source"] = loaded["document_source"]
            value["document_source_binding"] = loaded["document_source_binding"]
            _print(value, args.format)
            return 0

        snapshot = capture_checkout(accepted_main_sha)
        dispatch_capsule = None
        if args.command == "enter":
            receipt = read_checkpoint()
            value = build_session_entry(
                contract=parse_route_contract(
                    documents["START_HERE.md"],
                ),
                role=args.role,
                accepted_main_sha=accepted_main_sha,
                document_source=loaded["document_source"],
                document_source_binding=loaded["document_source_binding"],
                mission=mission,
                dispatch_capsule=dispatch_capsule,
                snapshot=snapshot,
                checkpoint=receipt,
            )
            _print(value, args.format)
            return {"RESUME": 0, "REPAIR": 2, "DECISION_REQUIRED": 3}[
                value["resume_disposition"]
            ]

        if args.command == "checkpoint-auto":
            if args.mission != mission["mission_id"]:
                raise SessionContextError("checkpoint_mission_not_current")
            raise SessionContextError("current_mission_unavailable")

        if args.command == "resume":
            receipt = read_checkpoint()
            value = classify_resume(
                receipt,
                snapshot=snapshot,
                mission=mission,
                dispatch_capsule=dispatch_capsule,
            )
            _print(value, args.format)
            return {"RESUME": 0, "REPAIR": 2, "DECISION_REQUIRED": 3}[value["disposition"]]
    except (OSError, SessionContextError) as exc:
        reason = exc.reason if isinstance(exc, SessionContextError) else "document_read_failed"
        print(
            json.dumps(
                {
                    "schema_version": "agent_session_error.v1",
                    "disposition": "DECISION_REQUIRED",
                    "reason": reason,
                },
                sort_keys=True,
            )
        )
        return 3
    raise AssertionError("unreachable command")


if __name__ == "__main__":
    raise SystemExit(main())
