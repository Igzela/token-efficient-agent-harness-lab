#!/usr/bin/env python3
"""Route minimal repository context and recover interrupted local sessions.

Accepted documents and GitHub remain authoritative.  The checkpoint written by
this tool is a bounded, non-authoritative projection in Git's private path; it
can prove that a later local checkout still matches a handoff, but it cannot
select a packet, grant authority, or make lifecycle/CI/review decisions.
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
import stat
import subprocess
import sys
import tempfile
from typing import Any


ROOT = Path(__file__).resolve().parents[1]

ROUTE_SCHEMA = "agent_context_routes.v1"
CHECKPOINT_SCHEMA = "agent_session_handoff.v1"
CHECKPOINT_AUTHORITY = "non_authoritative_local_projection"
CHECKPOINT_BASENAME = "agent-session-handoff.v1.json"

MAX_ROUTE_DOCUMENT_BYTES = 128 * 1024
MAX_ROUTE_DOCUMENTS = 8
MAX_CHECKPOINT_BYTES = 64 * 1024
MAX_DIRTY_PATHS = 5_000
MAX_PATH_CHARS = 512
MAX_TEXT_CHARS = 2_048

SHA40 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
PACKET_ID = re.compile(r"^[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+$")
PACKET_TOKEN = re.compile(r"\b[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+\b")
OPTION_ID = re.compile(r"^[a-z][a-z0-9-]{0,31}$")
ROUTE_MARKER = re.compile(
    r"<!--\s*agent-context-routes:v1\s*(?P<payload>\{.*?\})\s*-->", re.DOTALL
)
WEAK_DISPATCH_MARKER = re.compile(
    r"<!--\s*weak-agent-dispatch:v1\s*(?P<payload>\{.*?\})\s*-->", re.DOTALL
)
PACKET_HEADING = re.compile(
    r"^#{2,3} Packet (?P<packet>[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+)(?:\b.*)?$",
    re.MULTILINE,
)
PACKET_STATE = re.compile(r"^\*\*State:\*\*\s*`(?P<state>[A-Z_]+)`", re.MULTILINE)
STAGE_HEADING = re.compile(r"^## Stage .+$", re.MULTILINE)

ROLES = frozenset({"planning", "coding", "review", "ci-repair", "operator", "contributor"})
WORK_STATES = frozenset({"WIP", "STABLE", "BLOCKED", "OUTCOME_UNKNOWN"})
VERIFICATION_STATES = frozenset({"PASS", "FAIL", "NOT_RUN", "BLOCKED"})
EXECUTABLE_PACKET_STATES = frozenset({"READY_FOR_EXECUTION", "IN_PROGRESS"})
CANONICAL_DOCUMENTS = frozenset(
    {
        "START_HERE.md",
        "AGENTS.md",
        "README.md",
        "CLAUDE.md",
        "docs/ARCHITECTURE_BOOK.md",
        "docs/CURRENT_STATUS.md",
        "docs/NEXT_DECISION.md",
        "docs/FUTURE_ROUTE.md",
        "docs/MODULE_MAP.md",
        "docs/REAL_WORLD_TESTING_PLAYBOOK.md",
        "docs/RUNBOOK.md",
    }
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
class PacketBinding:
    packet_id: str
    state: str
    source_path: str
    packet_sha256: str
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
    ) -> PacketBinding:
        if isinstance(value, cls):
            value = value.to_wire()
        wire = _wire_mapping(value, "packet_binding_invalid")
        allowed_fields = {
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
        required_fields = {
            "packet_id",
            "state",
            "source_path",
            "packet_sha256",
            "allowed_paths",
        }
        if not required_fields.issubset(wire) or not set(wire).issubset(allowed_fields):
            raise SessionContextError("packet_binding_fields_invalid")
        packet_id = wire.get("packet_id")
        if not isinstance(packet_id, str) or not PACKET_ID.fullmatch(packet_id):
            raise SessionContextError("packet_id_invalid")
        state = _bounded_text(wire.get("state"), "packet_state", max_chars=64)
        if not re.fullmatch(r"[A-Z_]+", state):
            raise SessionContextError("packet_state_invalid")
        source_path = _repo_path(wire.get("source_path"), "packet_source_path")
        packet_sha256 = _validate_sha(
            wire.get("packet_sha256"), "packet_sha256", SHA256
        )
        raw_allowed = wire.get("allowed_paths")
        if not isinstance(raw_allowed, list):
            raise SessionContextError("packet_allowed_paths_invalid")
        try:
            allowed_paths = tuple(
                sorted({_repo_path(item, "packet_allowed_path") for item in raw_allowed})
            )
        except SessionContextError as exc:
            raise SessionContextError("packet_allowed_paths_invalid") from exc
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
            raise SessionContextError("packet_execution_authority_invalid")
        checkpoint_allowed = wire.get("checkpoint_allowed", execution_authorized)
        if not isinstance(checkpoint_allowed, bool):
            raise SessionContextError("packet_checkpoint_authority_invalid")
        dispatch_lane_value = wire.get("dispatch_lane")
        dispatch_lane = (
            None
            if dispatch_lane_value is None
            else _bounded_text(
                dispatch_lane_value,
                "packet_dispatch_lane",
                max_chars=128,
            )
        )
        model = cls(
            packet_id=packet_id,
            state=state,
            source_path=source_path,
            packet_sha256=packet_sha256,
            allowed_paths=allowed_paths,
            forbidden_next_actions=forbidden_next_actions,
            execution_authorized=execution_authorized,
            checkpoint_allowed=checkpoint_allowed,
            dispatch_lane=dispatch_lane,
        )
        if require_checkpoint and (
            model.source_path != "docs/NEXT_DECISION.md"
            or model.state not in EXECUTABLE_PACKET_STATES
            or not model.checkpoint_allowed
        ):
            raise SessionContextError("packet_not_executable")
        return model

    def to_wire(self) -> dict[str, object]:
        return {
            "packet_id": self.packet_id,
            "state": self.state,
            "source_path": self.source_path,
            "packet_sha256": self.packet_sha256,
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
    packet_id: str
    packet_state: str
    packet_sha256: str
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
            "packet_id": self.packet_id,
            "packet_state": self.packet_state,
            "packet_sha256": self.packet_sha256,
            "documents": list(self.documents),
            "included_options": list(self.included_options),
            "execution_authorized": self.execution_authorized,
            "checkpoint_allowed": self.checkpoint_allowed,
            "bootstrap_order": list(self.bootstrap_order),
        }


@dataclass(frozen=True)
class PacketExtract:
    schema_version: str
    authority: str
    accepted_main_sha: str
    source_path: str
    document_sha256: str
    packet_id: str
    packet_state: str
    packet_sha256: str
    profile_id: str | None
    worker_tier: str | None
    prerequisites: tuple[str, ...]
    execution_authorized: bool
    stage_heading: str | None
    global_contract: str
    packet_text: str

    def to_wire(self) -> dict[str, object]:
        return {
            "schema_version": self.schema_version,
            "authority": self.authority,
            "accepted_main_sha": self.accepted_main_sha,
            "source_path": self.source_path,
            "document_sha256": self.document_sha256,
            "packet_id": self.packet_id,
            "packet_state": self.packet_state,
            "packet_sha256": self.packet_sha256,
            "profile_id": self.profile_id,
            "worker_tier": self.worker_tier,
            "prerequisites": list(self.prerequisites),
            "execution_authorized": self.execution_authorized,
            "stage_heading": self.stage_heading,
            "global_contract": self.global_contract,
            "packet_text": self.packet_text,
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
    packet_id: str
    packet_state: str
    packet_source_path: str
    packet_sha256: str
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
    verification_results: tuple[VerificationResult, ...]
    next_action: str
    forbidden_next_actions: tuple[str, ...]
    checkpoint_id: str

    def unsigned_wire(self) -> dict[str, object]:
        return {
            "schema_version": self.schema_version,
            "projection_authority": self.projection_authority,
            "packet_id": self.packet_id,
            "packet_state": self.packet_state,
            "packet_source_path": self.packet_source_path,
            "packet_sha256": self.packet_sha256,
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
        packet: PacketBinding,
        role: str,
        work_state: str,
        completed_step: str,
        owned_paths: tuple[str, ...],
        preserve_paths: tuple[str, ...],
        verification_results: tuple[VerificationResult, ...],
        next_action: str,
        forbidden_next_actions: tuple[str, ...],
    ) -> SessionCheckpoint:
        candidate = cls(
            schema_version=CHECKPOINT_SCHEMA,
            projection_authority=CHECKPOINT_AUTHORITY,
            packet_id=packet.packet_id,
            packet_state=packet.state,
            packet_source_path=packet.source_path,
            packet_sha256=packet.packet_sha256,
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
        packet_id = wire.get("packet_id")
        if not isinstance(packet_id, str) or not PACKET_ID.fullmatch(packet_id):
            raise SessionContextError("checkpoint_packet_invalid")
        packet_state = wire.get("packet_state")
        if not isinstance(packet_state, str) or packet_state not in EXECUTABLE_PACKET_STATES:
            raise SessionContextError("checkpoint_packet_state_invalid")
        if wire.get("packet_source_path") != "docs/NEXT_DECISION.md":
            raise SessionContextError("checkpoint_packet_source_invalid")
        packet_sha256 = _validate_sha(
            wire.get("packet_sha256"), "checkpoint_packet_sha256", SHA256
        )
        accepted_main_sha = _validate_sha(
            wire.get("accepted_main_sha"), "checkpoint_accepted_main_sha", SHA40
        )
        head_sha = _validate_sha(wire.get("head_sha"), "checkpoint_head_sha", SHA40)
        branch = _bounded_text(wire.get("branch"), "checkpoint_branch", max_chars=256)
        if branch.startswith("-"):
            raise SessionContextError("checkpoint_branch_invalid")
        role = wire.get("role")
        if not isinstance(role, str) or role not in ROLES:
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
        raw_results = wire.get("verification_results")
        if not isinstance(raw_results, list) or len(raw_results) > 100:
            raise SessionContextError("verification_results_invalid")
        verification_results = tuple(
            VerificationResult.from_wire(item) for item in raw_results
        )
        if len({item.check for item in verification_results}) != len(verification_results):
            raise SessionContextError("verification_results_invalid")
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
            packet_id=packet_id,
            packet_state=packet_state,
            packet_source_path="docs/NEXT_DECISION.md",
            packet_sha256=packet_sha256,
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
    packet_id: str | None
    checkpoint_id: str | None
    next_permitted_action: str
    forbidden_next_actions: tuple[str, ...]

    def to_wire(self) -> dict[str, object]:
        return {
            "schema_version": self.schema_version,
            "authority": self.authority,
            "disposition": self.disposition,
            "reason": self.reason,
            "packet_id": self.packet_id,
            "checkpoint_id": self.checkpoint_id,
            "next_permitted_action": self.next_permitted_action,
            "forbidden_next_actions": list(self.forbidden_next_actions),
        }


def parse_route_contract(document: str) -> RouteContract:
    """Parse the sole machine-readable role router from ``START_HERE.md``."""

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
        if (
            not isinstance(required, list)
            or not required
            or len(required) > maximum
            or required[0] != "START_HERE.md"
            or len(required) != len(set(required))
            or any(path not in CANONICAL_DOCUMENTS for path in required)
        ):
            raise SessionContextError("route_contract_required_invalid")
        if not isinstance(optional, dict) or len(optional) > MAX_ROUTE_DOCUMENTS:
            raise SessionContextError("route_contract_optional_invalid")
        for option, path in optional.items():
            if (
                not isinstance(option, str)
                or not OPTION_ID.fullmatch(option)
                or path not in CANONICAL_DOCUMENTS
                or path in required
            ):
                raise SessionContextError("route_contract_optional_invalid")
        normalized_roles.append(
            (
                role,
                RouteRole(
                    required=tuple(required),
                    optional=tuple(sorted(optional.items())),
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
    packet: object,
    include: list[str] | None = None,
) -> dict[str, object]:
    """Return one bounded document manifest for a known repository role."""

    _validate_sha(accepted_main_sha, "accepted_main_sha", SHA40)
    if not isinstance(contract, RouteContract) or role not in ROLES:
        raise SessionContextError("role_unsupported")
    packet_model = PacketBinding.from_wire(packet)
    selected = include or []
    if len(selected) != len(set(selected)):
        raise SessionContextError("route_option_duplicated")
    route = contract.role_for(role)
    optional = route.option_map()
    unknown = [option for option in selected if option not in optional]
    if unknown:
        raise SessionContextError("route_option_unsupported")
    documents = (*route.required, *(optional[option] for option in selected))
    if len(documents) > MAX_ROUTE_DOCUMENTS or len(documents) != len(set(documents)):
        raise SessionContextError("route_document_limit_exceeded")
    return ContextRoute(
        schema_version="agent_context_route.v1",
        authority="accepted_documents_select_context; route_grants_no_execution_authority",
        accepted_main_sha=accepted_main_sha,
        role=role,
        packet_id=packet_model.packet_id,
        packet_state=packet_model.state,
        packet_sha256=packet_model.packet_sha256,
        documents=tuple(documents),
        included_options=tuple(selected),
        execution_authorized=packet_model.execution_authorized,
        checkpoint_allowed=packet_model.checkpoint_allowed,
        bootstrap_order=(
            "read the returned documents in order",
            "run scripts/project_context.py and verify accepted main/live frontier",
            "run session_context.py resume before touching an existing worktree",
            "stop on any DECISION_REQUIRED disposition",
        ),
    ).to_wire()


def _packet_blocks(document: str) -> list[tuple[str, int, int, str]]:
    headings = list(PACKET_HEADING.finditer(document))
    blocks: list[tuple[str, int, int, str]] = []
    for index, heading in enumerate(headings):
        end = headings[index + 1].start() if index + 1 < len(headings) else len(document)
        blocks.append(
            (
                heading.group("packet"),
                heading.start(),
                end,
                document[heading.start() : end].rstrip() + "\n",
            )
        )
    return blocks


def _one_packet_block(document: str, packet_id: str) -> tuple[int, int, str]:
    matches = [item for item in _packet_blocks(document) if item[0] == packet_id]
    if not matches:
        raise SessionContextError("packet_missing")
    if len(matches) != 1:
        raise SessionContextError("packet_duplicated")
    _packet, start, end, block = matches[0]
    return start, end, block


def _packet_state(block: str) -> str:
    states = PACKET_STATE.findall(block)
    if len(states) != 1:
        raise SessionContextError("packet_state_missing_or_ambiguous")
    return states[0]


def extract_packet(
    document: str,
    *,
    packet_id: str,
    accepted_main_sha: str,
    source_path: str,
) -> dict[str, object]:
    """Extract one packet without carrying the neighboring route portfolio."""

    if not isinstance(packet_id, str) or not PACKET_ID.fullmatch(packet_id):
        raise SessionContextError("packet_id_invalid")
    _validate_sha(accepted_main_sha, "accepted_main_sha", SHA40)
    if source_path not in {"docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md"}:
        raise SessionContextError("packet_source_invalid")
    start, _end, block = _one_packet_block(document, packet_id)
    state = _packet_state(block)
    stages = [match for match in STAGE_HEADING.finditer(document, 0, start)]
    stage_heading = stages[-1].group(0) if stages else None
    boundaries = [match.start() for match in STAGE_HEADING.finditer(document)]
    inventory = re.search(r"^## Portfolio Inventory Manifest$", document, re.MULTILINE)
    if inventory:
        boundaries.append(inventory.start())
    global_end = min(boundaries) if boundaries else start
    global_contract = document[:global_end].rstrip() + "\n"
    prerequisite_line = re.search(
        r"^\*\*Prerequisites?:\*\*\s*(?P<value>.+)$", block, re.MULTILINE
    )
    prerequisites = (
        re.findall(r"[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+", prerequisite_line.group("value"))
        if prerequisite_line
        else []
    )
    prerequisites = list(dict.fromkeys(item for item in prerequisites if item != packet_id))
    profile = re.search(r"^\*\*Execution profile:\*\*\s*`?([^`\n]+)`?", block, re.MULTILINE)
    tier = re.search(r"^\*\*Worker tier:\*\*\s*`?([^`\n]+)`?", block, re.MULTILINE)
    is_future = source_path == "docs/FUTURE_ROUTE.md"
    return PacketExtract(
        schema_version="agent_packet_extract.v1",
        authority="routing_projection_only" if is_future else "current_document_projection",
        accepted_main_sha=accepted_main_sha,
        source_path=source_path,
        document_sha256=hashlib.sha256(document.encode("utf-8")).hexdigest(),
        packet_id=packet_id,
        packet_state=state,
        packet_sha256=hashlib.sha256(block.encode("utf-8")).hexdigest(),
        profile_id=profile.group(1).strip() if profile else None,
        worker_tier=tier.group(1).strip() if tier else None,
        prerequisites=tuple(prerequisites),
        execution_authorized=not is_future and state in EXECUTABLE_PACKET_STATES,
        stage_heading=stage_heading,
        global_contract=global_contract,
        packet_text=block,
    ).to_wire()


def current_packet_binding(next_document: str, accepted_main_sha: str) -> dict[str, object]:
    """Bind the routed packet without interpreting prose as new authority."""

    _validate_sha(accepted_main_sha, "accepted_main_sha", SHA40)
    markers = list(WEAK_DISPATCH_MARKER.finditer(next_document))
    if len(markers) == 1:
        try:
            payload = json.loads(markers[0].group("payload"))
        except json.JSONDecodeError as exc:
            raise SessionContextError("weak_dispatch_json_invalid") from exc
        if not isinstance(payload, dict) or payload.get("schema_version") != "weak_agent_dispatch.v1":
            raise SessionContextError("weak_dispatch_invalid")
        packet_id = payload.get("packet_id")
        if not isinstance(packet_id, str) or not PACKET_ID.fullmatch(packet_id):
            raise SessionContextError("packet_id_invalid")
        start, end, block = _one_packet_block(next_document, packet_id)
        if markers[0].start() < start or markers[0].end() > end:
            raise SessionContextError("weak_dispatch_outside_packet")
        state = _packet_state(block)
        active = re.search(
            r"^## Active Routing$(?P<body>.*?)(?=^## |\Z)",
            next_document,
            re.MULTILINE | re.DOTALL,
        )
        if not active or packet_id not in active.group("body"):
            raise SessionContextError("packet_not_current_route")
        raw_allowed = payload.get("allowed_paths")
        if not isinstance(raw_allowed, list) or not raw_allowed:
            raise SessionContextError("packet_allowed_paths_invalid")
        try:
            allowed_paths = sorted(
                {_repo_path(item, "packet_allowed_path") for item in raw_allowed}
            )
        except SessionContextError as exc:
            raise SessionContextError("packet_allowed_paths_invalid") from exc
        forbidden = payload.get("forbidden_next_actions")
        forbidden_next = _bounded_string_list(forbidden, "forbidden_next_actions")
        return PacketBinding.from_wire(
            {
                "packet_id": packet_id,
                "state": state,
                "source_path": "docs/NEXT_DECISION.md",
                "packet_sha256": hashlib.sha256(block.encode("utf-8")).hexdigest(),
                "allowed_paths": allowed_paths,
                "forbidden_next_actions": forbidden_next,
                "execution_authorized": False,
                "checkpoint_allowed": state in EXECUTABLE_PACKET_STATES,
                "dispatch_lane": payload.get("dispatch_lane"),
            }
        ).to_wire()
    if markers:
        raise SessionContextError("weak_dispatch_duplicated")

    active = re.search(
        r"^## Active Routing$(?P<body>.*?)(?=^## |\Z)",
        next_document,
        re.MULTILINE | re.DOTALL,
    )
    routed = PACKET_TOKEN.findall(active.group("body") if active else "")
    routed = list(dict.fromkeys(routed))
    if len(routed) != 1:
        raise SessionContextError("current_packet_missing_or_ambiguous")
    _start, _end, block = _one_packet_block(next_document, routed[0])
    return PacketBinding(
        packet_id=routed[0],
        state=_packet_state(block),
        source_path="docs/NEXT_DECISION.md",
        packet_sha256=hashlib.sha256(block.encode("utf-8")).hexdigest(),
        allowed_paths=(),
        forbidden_next_actions=(
            "Do not execute without a machine-bound dispatch contract.",
        ),
        execution_authorized=False,
        checkpoint_allowed=False,
        dispatch_lane=None,
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


def build_checkpoint(
    *,
    snapshot: object,
    packet: object,
    role: str,
    work_state: str,
    completed_step: str,
    owned_paths: list[str],
    verification_results: list[dict[str, str]],
    next_action: str,
    forbidden_next_actions: list[str],
) -> dict[str, object]:
    """Build a digest-bound handoff for the exact current worktree."""

    snapshot_model = CheckoutSnapshot.from_wire(snapshot)
    packet_model = PacketBinding.from_wire(packet, require_checkpoint=True)
    if role not in ROLES:
        raise SessionContextError("role_unsupported")
    if work_state not in WORK_STATES:
        raise SessionContextError("work_state_invalid")
    dirty_paths = list(snapshot_model.dirty_paths)
    owned = sorted({_repo_path(path, "owned_path", allow_directory=False) for path in owned_paths})
    if not set(owned).issubset(dirty_paths):
        raise SessionContextError("owned_path_not_dirty")
    allowed = list(packet_model.allowed_paths)
    if any(not _path_is_allowed(path, allowed) for path in owned):
        raise SessionContextError("owned_path_not_allowed")
    preserve = sorted(set(dirty_paths) - set(owned))
    checks = _verification_models(verification_results)
    forbidden = tuple(
        _bounded_string_list(forbidden_next_actions, "forbidden_next_actions")
    )
    receipt = SessionCheckpoint.create(
        snapshot=snapshot_model,
        packet=packet_model,
        role=role,
        work_state=work_state,
        completed_step=_bounded_text(completed_step, "completed_step"),
        owned_paths=tuple(owned),
        preserve_paths=tuple(preserve),
        verification_results=checks,
        next_action=_bounded_text(next_action, "next_action"),
        forbidden_next_actions=forbidden,
    )
    return SessionCheckpoint.from_wire(receipt.to_wire()).to_wire()


def validate_checkpoint(receipt: object) -> dict[str, object]:
    """Validate one checkpoint without trusting its claimed disposition."""

    return SessionCheckpoint.from_wire(receipt).to_wire()


def _disposition(
    receipt: SessionCheckpoint | None,
    packet: PacketBinding,
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
        packet_id=packet.packet_id,
        checkpoint_id=receipt.checkpoint_id if receipt else None,
        next_permitted_action=action,
        forbidden_next_actions=(
            receipt.forbidden_next_actions
            if receipt
            else packet.forbidden_next_actions
        ),
    ).to_wire()


def classify_resume(
    receipt: object | None,
    *,
    snapshot: object,
    packet: object,
) -> dict[str, object]:
    """Classify exact recovery as RESUME, REPAIR, or DECISION_REQUIRED."""

    snapshot_model = CheckoutSnapshot.from_wire(snapshot)
    try:
        packet_model = PacketBinding.from_wire(packet, require_checkpoint=True)
    except SessionContextError:
        try:
            packet_projection = PacketBinding.from_wire(packet)
        except SessionContextError:
            packet_projection = None
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
            reason="packet_not_executable",
            packet_id=packet_projection.packet_id if packet_projection else None,
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
                    packet_projection.forbidden_next_actions
                    if receipt is None and packet_projection
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
                packet_model,
                "RESUME",
                "clean_accepted_baseline",
                "Enter the current packet only through its authorized dispatch lane.",
            )
        return _disposition(
            None,
            packet_model,
            "DECISION_REQUIRED",
            "checkpoint_missing_for_noncanonical_checkout",
            "Identify the owner of the existing branch/WIP before changing any file.",
        )
    receipt_model = SessionCheckpoint.from_wire(receipt)
    if receipt_model.work_state == "OUTCOME_UNKNOWN":
        return _disposition(
            receipt_model,
            packet_model,
            "DECISION_REQUIRED",
            "checkpoint_outcome_unknown",
            "Resolve external-effect status with the named authority owner.",
        )
    if receipt_model.work_state == "BLOCKED":
        return _disposition(
            receipt_model,
            packet_model,
            "DECISION_REQUIRED",
            "checkpoint_blocked",
            "Resolve the recorded blocker before resuming implementation.",
        )
    if snapshot_model.accepted_main_sha != receipt_model.accepted_main_sha:
        return _disposition(
            receipt_model,
            packet_model,
            "DECISION_REQUIRED",
            "accepted_main_changed",
            "Rebase the plan through the canonical planning owner; do not reuse this receipt.",
        )
    if (
        packet_model.packet_id != receipt_model.packet_id
        or packet_model.packet_sha256 != receipt_model.packet_sha256
        or packet_model.source_path != receipt_model.packet_source_path
    ):
        return _disposition(
            receipt_model,
            packet_model,
            "DECISION_REQUIRED",
            "packet_binding_changed",
            "Refresh the packet contract and obtain a replacement checkpoint.",
        )
    if snapshot_model.branch != receipt_model.branch:
        return _disposition(
            receipt_model,
            packet_model,
            "DECISION_REQUIRED",
            "branch_changed",
            "Return to the bound branch or obtain an owner-approved replacement handoff.",
        )
    current_paths = set(snapshot_model.dirty_paths)
    prior_paths = set(receipt_model.dirty_paths)
    preserve = set(receipt_model.preserve_paths)
    missing_preserve = preserve - current_paths
    if missing_preserve:
        return _disposition(
            receipt_model,
            packet_model,
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
            packet_model,
            "DECISION_REQUIRED",
            "preserved_path_changed",
            "Identify the owner of changed preserved work before continuing.",
        )
    added = current_paths - prior_paths
    allowed = list(packet_model.allowed_paths)
    if any(not _path_is_allowed(path, allowed) for path in added):
        return _disposition(
            receipt_model,
            packet_model,
            "DECISION_REQUIRED",
            "unbound_dirty_paths",
            "Identify and bind the new dirty paths before changing the worktree.",
        )
    if added:
        return _disposition(
            receipt_model,
            packet_model,
            "REPAIR",
            "uncheckpointed_allowed_changes",
            "Audit the added in-scope changes, rerun focused checks, and replace the checkpoint.",
        )
    if snapshot_model.head_sha != receipt_model.head_sha:
        return _disposition(
            receipt_model,
            packet_model,
            "REPAIR",
            "exact_head_changed",
            "Audit the new exact head and replace the stale checkpoint before implementation.",
        )
    if snapshot_model.worktree_sha256 != receipt_model.worktree_sha256:
        return _disposition(
            receipt_model,
            packet_model,
            "REPAIR",
            "worktree_changed_within_bound_paths",
            "Audit the changed owned paths, rerun focused checks, and replace the checkpoint.",
        )
    statuses = {item.status for item in receipt_model.verification_results}
    if "BLOCKED" in statuses:
        return _disposition(
            receipt_model,
            packet_model,
            "DECISION_REQUIRED",
            "verification_blocked",
            "Resolve the recorded verification blocker with its owner.",
        )
    if "FAIL" in statuses:
        return _disposition(
            receipt_model,
            packet_model,
            "REPAIR",
            "verification_failed",
            "Repair the recorded failure within packet scope and replace the checkpoint.",
        )
    return _disposition(
        receipt_model,
        packet_model,
        "RESUME",
        "exact_checkpoint_match",
        receipt_model.next_action,
    )


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
        reader = lambda path: project_context.git_show_text(sha, path)
        source_binding = sha
    elif source == "working-tree":
        reader = lambda path: (ROOT / path).read_text(encoding="utf-8")
        source_binding = "working_tree_unaccepted"
    else:
        raise SessionContextError("document_source_invalid")
    documents = {
        path: reader(path)
        for path in ("START_HERE.md", "docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md")
    }
    if any(not value for value in documents.values()):
        raise SessionContextError("canonical_document_unavailable")
    return {
        "accepted_main_sha": sha,
        "accepted_main_source": baseline.get("source"),
        "document_source": source,
        "document_source_binding": source_binding,
        "documents": documents,
    }


def _parse_verification(values: list[str]) -> list[dict[str, str]]:
    results: list[dict[str, str]] = []
    for value in values:
        check, separator, status_value = value.rpartition("=")
        if not separator:
            raise SessionContextError("verification_argument_invalid")
        results.append({"check": check, "status": status_value})
    return _verification_results(results)


def _render_route(route: dict[str, Any]) -> str:
    lines = [
        "# Session Context Route",
        "",
        f"- Accepted main: `{route['accepted_main_sha']}`",
        f"- Role: `{route['role']}`",
        f"- Packet: `{route['packet_id']}` (`{route['packet_state']}`)",
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
    elif value.get("schema_version") == "agent_packet_extract.v1":
        print(
            f"<!-- accepted-main: {value['accepted_main_sha']} ; "
            f"packet-sha256: {value['packet_sha256']} ; execution-authorized: false -->"
        )
        print(value["global_contract"], end="")
        if value.get("stage_heading"):
            print(value["stage_heading"])
            print()
        print(value["packet_text"], end="")
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

    extract = subparsers.add_parser("extract-packet", help="Extract one current/future packet.")
    extract.add_argument("--packet", required=True)
    extract.add_argument("--source", choices=("accepted", "working-tree"), default="accepted")
    extract.add_argument("--offline", action="store_true")
    extract.add_argument("--format", choices=("markdown", "json"), default="markdown")

    checkpoint = subparsers.add_parser("checkpoint", help="Replace the local handoff projection.")
    checkpoint.add_argument("--role", choices=sorted(ROLES), default="coding")
    checkpoint.add_argument("--packet")
    checkpoint.add_argument("--work-state", choices=sorted(WORK_STATES), default="WIP")
    checkpoint.add_argument("--completed-step", required=True)
    checkpoint.add_argument("--owned-path", action="append", default=[])
    checkpoint.add_argument("--verification", action="append", default=[])
    checkpoint.add_argument("--next-action", required=True)
    checkpoint.add_argument("--forbidden-action", action="append", default=[])
    checkpoint.add_argument("--offline", action="store_true")
    checkpoint.add_argument("--no-write", action="store_true")

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
        packet = current_packet_binding(documents["docs/NEXT_DECISION.md"], accepted_main_sha)
        if source != "accepted":
            packet_model = PacketBinding.from_wire(packet)
            packet = replace(
                packet_model,
                execution_authorized=False,
                checkpoint_allowed=False,
            ).to_wire()

        if args.command == "route":
            contract = parse_route_contract(documents["START_HERE.md"])
            value = build_route(
                contract,
                role=args.role,
                accepted_main_sha=accepted_main_sha,
                packet=packet,
                include=args.include,
            )
            value["document_source"] = loaded["document_source"]
            value["document_source_binding"] = loaded["document_source_binding"]
            _print(value, args.format)
            return 0

        if args.command == "extract-packet":
            source_path = (
                "docs/NEXT_DECISION.md"
                if args.packet == packet["packet_id"]
                else "docs/FUTURE_ROUTE.md"
            )
            value = extract_packet(
                documents[source_path],
                packet_id=args.packet,
                accepted_main_sha=accepted_main_sha,
                source_path=source_path,
            )
            if source != "accepted":
                value["execution_authorized"] = False
                value["authority"] = "working_tree_unaccepted_projection_only"
            _print(value, args.format)
            return 0

        snapshot = capture_checkout(accepted_main_sha)
        if args.command == "checkpoint":
            if args.packet and args.packet != packet["packet_id"]:
                raise SessionContextError("checkpoint_packet_not_current")
            forbidden = args.forbidden_action or packet.get("forbidden_next_actions", [])
            receipt = build_checkpoint(
                snapshot=snapshot,
                packet=packet,
                role=args.role,
                work_state=args.work_state,
                completed_step=args.completed_step,
                owned_paths=sorted(set(args.owned_path)),
                verification_results=_parse_verification(args.verification),
                next_action=args.next_action,
                forbidden_next_actions=forbidden,
            )
            if not args.no_write:
                write_checkpoint(receipt)
            _print(
                {
                    "schema_version": CHECKPOINT_SCHEMA,
                    "checkpoint_id": receipt["checkpoint_id"],
                    "packet_id": receipt["packet_id"],
                    "head_sha": receipt["head_sha"],
                    "work_state": receipt["work_state"],
                    "owned_path_count": len(receipt["owned_paths"]),
                    "preserve_path_count": len(receipt["preserve_paths"]),
                    "storage": "git_private_projection" if not args.no_write else "not_written",
                    "authority": CHECKPOINT_AUTHORITY,
                },
                "json",
            )
            return 0

        if args.command == "resume":
            receipt = read_checkpoint()
            value = classify_resume(receipt, snapshot=snapshot, packet=packet)
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
