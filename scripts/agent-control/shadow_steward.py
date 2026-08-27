"""Provider-free Shadow Steward projections.

This module is a pure read-only seam for the planned Steward.  It accepts
bounded untrusted text, reduces it to digested policy facts, and returns
immutable recommendations.  It never retains the request, calls a Provider,
touches GitHub or the repository, persists state, consumes authority, or
executes a mutation.  ``mission_contract`` remains the owner of Mission,
Stage, and WorkCard validation; ``local_loop`` remains the legacy lifecycle
writer.
"""

from __future__ import annotations

from dataclasses import dataclass, field, replace
import re
from types import MappingProxyType
from typing import Any, Protocol

import mission_contract as contract


SCHEMA_VERSION = "shadow_steward.v1"
MAX_INTAKE_CHARS = 8 * 1024
MAX_ITEMS = 32
MAX_FAILURES = 100
MAX_ID_CHARS = 128
_PROJECTION_TOKEN = object()

SAFE_CHANGE_TYPES = (
    "documentation",
    "source",
    "tests",
    "configuration",
    "workflow",
)
SAFE_PATH = re.compile(
    r"(?<![A-Za-z0-9_.-])"
    r"(?:docs|scripts|tests|engine|sdk|dashboard|tools|wire_contract|codegen)"
    r"(?:/[A-Za-z0-9_.-]+)+(?![A-Za-z0-9_.-])"
)
IDENTIFIER = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")

ROUTINE_FAILURES = frozenset(
    code
    for code, category in contract.STOP_CATEGORIES.items()
    if category == "ROUTINE_RECOVERY"
)
OWNER_FAILURES = frozenset(
    code
    for code, category in contract.STOP_CATEGORIES.items()
    if category == "PAUSED_FOR_OWNER"
) | {"BUDGET_EXCEEDED"}
KNOWN_STOP_CODES = ROUTINE_FAILURES | OWNER_FAILURES
KNOWN_SOURCES = frozenset({"issue", "pr", "ci", "review"})
HISTORICAL_EVIDENCE = MappingProxyType({
    "github:issue:77:worker-failed": ("issue", "WORKER_FAILED", "failed"),
    "github:pr:630:review-changes-requested": (
        "pr",
        "REVIEW_CHANGES_REQUESTED",
        "failed",
    ),
    "github:ci:33108617013:macos-startup-race": (
        "ci",
        "CI_FAILED",
        "failed",
    ),
    "github:review:630:unknown-outcome": (
        "review",
        "EXTERNAL_OUTCOME_UNKNOWN",
        "outcome_unknown",
    ),
    "github:issue:77:safety-conflict": ("issue", "SAFETY_CONFLICT", "paused"),
})

_PATH_MARKERS = (
    "/home/",
    "/root/",
    "/tmp/",
    "~/",
    ".ssh",
    ".codex",
)
_SENSITIVE_PATH_NAMES = frozenset(
    {
        ".env",
        ".git",
        ".codex",
        ".ssh",
        "secrets",
        "credentials",
        "private",
    }
)
_SECRET_MARKERS = (
    "api key",
    "api_key",
    "apikey",
    "password",
    "credential",
    "secret",
    "bearer token",
    "private key",
)
_AUTHORITY_MARKERS = (
    "broaden scope",
    "broader scope",
    "broaden the scope",
    "expand scope",
    "expand the scope",
    "increase budget",
    "new permission",
    "new grant",
    "skip approval",
    "bypass review",
    "auto-merge",
    "automatically merge",
)
_PRODUCTION_MARKERS = (
    "production",
    "deploy",
    "release",
    "publish",
    "provider",
    "external effect",
    "target write",
    "github write",
    "install service",
    "merge to main",
    "ship to prod",
    " to prod",
    "go live",
)
_DESTRUCTIVE_MARKERS = (
    "destructive",
    "delete",
    "drop database",
    "destroy",
    "overwrite data",
    "wipe",
    "purge",
    "truncate",
    "erase",
)
_UNKNOWN_MARKERS = (
    "unknown outcome",
    "outcome_unknown",
    "may have been sent",
    "possibly sent",
    "ambiguous response",
    "uncertain whether",
    "uncertain if",
    "i do not know whether",
    "i do not know if",
    "don't know whether",
    "not sure whether",
)
_SCOPE_MARKERS = (
    "entire repository",
    "whole repository",
    "all files",
    "everything",
    "unbounded",
)
_HIGH_RISK_PATTERNS = (
    re.compile(
        r"\b(?:write|push|modify|change|update|create|close|merge|comment)\b"
        r".{0,48}\bgithub\b"
    ),
    re.compile(
        r"\bgithub\b.{0,48}\b(?:write|push|modify|change|update|create|close|merge|comment)\b"
    ),
    re.compile(
        r"\b(?:cannot|can't|unable to|do not know|don't know|not sure|unclear|uncertain)\b"
        r".{0,48}\b(?:whether|if|outcome|completed|sent|succeeded|succeed)\b"
    ),
    re.compile(r"\bprod(?:uction)?\b"),
)


class ShadowStewardError(ValueError):
    """Raised when a shadow input cannot be safely represented."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


class OwnerApprovalAuthenticator(Protocol):
    """Read-only seam for an existing authenticated owner transport.

    PR2 consumes this evidence but does not implement authentication.  A
    caller that only has a comment, a self-asserted identity, or a wire-shaped
    approval cannot satisfy this interface's verified result.
    """

    def verify(self, approval: contract.OwnerApproval, proposal_sha256: str) -> bool: ...


def _text(value: object, field: str, *, max_chars: int = MAX_INTAKE_CHARS) -> str:
    if not isinstance(value, str) or not value or len(value) > max_chars:
        raise ShadowStewardError(f"{field}_invalid")
    if any(char in value for char in "\x00\r\n"):
        raise ShadowStewardError(f"{field}_invalid")
    return value.strip()


def _identifier(value: object, field: str) -> str:
    value = _text(value, field, max_chars=MAX_ID_CHARS)
    if IDENTIFIER.fullmatch(value) is None:
        raise ShadowStewardError(f"{field}_invalid")
    return value


def _sha(value: object, field: str) -> str:
    if not isinstance(value, str) or SHA256.fullmatch(value) is None:
        raise ShadowStewardError(f"{field}_invalid")
    return value


def _items(value: object, field: str, *, allowed: frozenset[str] | None = None) -> tuple[str, ...]:
    if not isinstance(value, (list, tuple)) or not value or len(value) > MAX_ITEMS:
        raise ShadowStewardError(f"{field}_invalid")
    result: list[str] = []
    for item in value:
        item = _text(item, field, max_chars=256)
        if allowed is not None and item not in allowed:
            raise ShadowStewardError(f"{field}_invalid")
        result.append(item)
    if len(set(result)) != len(result):
        raise ShadowStewardError(f"{field}_duplicated")
    return tuple(result)


def _optional_items(value: object, field: str) -> tuple[str, ...]:
    if value == [] or value == ():
        return ()
    if not isinstance(value, (list, tuple)) or len(value) > MAX_ITEMS:
        raise ShadowStewardError(f"{field}_invalid")
    result = tuple(_text(item, field, max_chars=256) for item in value)
    if len(set(result)) != len(result):
        raise ShadowStewardError(f"{field}_duplicated")
    return result


def _mapping(value: object, fields: set[str], required: set[str], reason: str) -> dict[str, Any]:
    if not isinstance(value, dict) or not set(value) <= fields or not required <= set(value):
        raise ShadowStewardError(reason)
    return value


def _safe_paths(text: str) -> tuple[str, ...]:
    result = tuple(
        dict.fromkeys(
            match.group(0).rstrip(".,;:)") for match in SAFE_PATH.finditer(text)
        )
    )
    if len(result) > MAX_ITEMS:
        raise ShadowStewardError("requested_paths_too_large")
    return result


def _is_sensitive_path(path: str) -> bool:
    parts = set(path.casefold().split("/"))
    return bool(parts & _SENSITIVE_PATH_NAMES) or any(
        part.endswith((".pem", ".key")) or "secret" in part or "credential" in part
        for part in parts
    )


def _validated_non_private_paths(value: object, field: str) -> tuple[str, ...]:
    paths = _optional_items(value, field)
    if any(SAFE_PATH.fullmatch(path) is None for path in paths):
        raise ShadowStewardError(f"{field}_invalid")
    if any(_is_sensitive_path(path) for path in paths):
        raise ShadowStewardError("private_path_forbidden")
    return paths


def _matches_high_risk_pattern(text: str) -> bool:
    return any(pattern.search(text) is not None for pattern in _HIGH_RISK_PATTERNS)


def _validate_mission(mission: contract.MaintenanceMission) -> contract.MaintenanceMission:
    if not isinstance(mission, contract.MaintenanceMission):
        raise ShadowStewardError("mission_invalid")
    try:
        current = contract.validate_registered_campaign()
    except contract.MissionContractError as exc:
        raise ShadowStewardError("registered_mission_invalid") from exc
    if mission.to_wire() != current.to_wire():
        raise ShadowStewardError("mission_registration_invalid")
    return current


def _paused_projection(
    proposal_sha256: str,
    mission_id: str,
    stop: StopRecommendation,
) -> PlanProjection:
    return PlanProjection(
        SCHEMA_VERSION,
        proposal_sha256,
        mission_id,
        "PAUSED_FOR_OWNER",
        None,
        (),
        stop,
        0,
    )


@dataclass(frozen=True)
class Intake:
    """Redacted facts derived from one request; the request is not retained."""

    schema_version: str
    request_sha256: str
    requested_paths: tuple[str, ...]
    change_types: tuple[str, ...]
    risk_flags: tuple[str, ...]
    stop_codes: tuple[str, ...]
    intent: str

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "request_sha256": self.request_sha256,
            "requested_paths": list(self.requested_paths),
            "change_types": list(self.change_types),
            "risk_flags": list(self.risk_flags),
            "stop_codes": list(self.stop_codes),
            "intent": self.intent,
        }

    @classmethod
    def from_wire(cls, value: object) -> Intake:
        fields = {
            "schema_version",
            "request_sha256",
            "requested_paths",
            "change_types",
            "risk_flags",
            "stop_codes",
            "intent",
        }
        wire = _mapping(value, fields, fields, "intake_fields_invalid")
        if wire["schema_version"] != SCHEMA_VERSION:
            raise ShadowStewardError("intake_schema_unsupported")
        request_sha = _sha(wire["request_sha256"], "request_sha256")
        paths = _validated_non_private_paths(wire["requested_paths"], "requested_paths")
        change_types = _items(
            wire["change_types"], "change_types", allowed=frozenset(SAFE_CHANGE_TYPES)
        )
        risk_flags = _optional_items(wire["risk_flags"], "risk_flags")
        stop_codes = _optional_items(wire["stop_codes"], "stop_codes")
        if any(code not in KNOWN_STOP_CODES for code in stop_codes):
            raise ShadowStewardError("stop_codes_invalid")
        intent = _text(wire["intent"], "intent", max_chars=64)
        return cls(SCHEMA_VERSION, request_sha, paths, change_types, risk_flags, stop_codes, intent)


def compile_intake(raw_request: str) -> Intake:
    """Compile untrusted natural language into bounded, non-content facts."""

    raw = _text(raw_request, "raw_request")
    lowered = raw.casefold()
    paths = _safe_paths(raw)
    sensitive_paths = any(_is_sensitive_path(path) for path in paths)
    paths = tuple(path for path in paths if not _is_sensitive_path(path))
    change_types: list[str] = []
    if any(word in lowered for word in ("doc", "documentation", "readme")):
        change_types.append("documentation")
    if any(word in lowered for word in ("test", "fixture", "verification")):
        change_types.append("tests")
    if any(word in lowered for word in ("config", "configuration")):
        change_types.append("configuration")
    if any(word in lowered for word in ("workflow", "ci")):
        change_types.append("workflow")
    if any(word in lowered for word in ("code", "implement", "fix", "refactor", "script")):
        change_types.append("source")
    if not change_types:
        change_types.append("source")

    risk_flags: list[str] = []
    stop_codes: list[str] = []
    if any(marker in lowered for marker in _AUTHORITY_MARKERS):
        risk_flags.append("authority_expansion")
        stop_codes.append("AUTHORITY_REQUIRED")
    if any(marker in lowered for marker in _PRODUCTION_MARKERS) or _matches_high_risk_pattern(
        lowered
    ):
        risk_flags.append("production_or_external_effect")
        stop_codes.append("AUTHORITY_REQUIRED")
    if any(marker in lowered for marker in _DESTRUCTIVE_MARKERS):
        risk_flags.append("destructive_operation")
        stop_codes.append("SAFETY_CONFLICT")
    if any(marker in lowered for marker in _UNKNOWN_MARKERS):
        risk_flags.append("unknown_outcome")
        stop_codes.append("EXTERNAL_OUTCOME_UNKNOWN")
    if any(marker in lowered for marker in _SECRET_MARKERS):
        risk_flags.append("secret_handling")
        stop_codes.append("SAFETY_CONFLICT")
    if any(marker in lowered for marker in _PATH_MARKERS):
        risk_flags.append("private_content")
        stop_codes.append("SAFETY_CONFLICT")
    if sensitive_paths:
        risk_flags.append("private_content")
        stop_codes.append("SAFETY_CONFLICT")
    if any(marker in lowered for marker in _SCOPE_MARKERS) or not paths:
        risk_flags.append("unbounded_or_missing_scope")
        stop_codes.append("SCOPE_EXCEEDED")

    return Intake(
        SCHEMA_VERSION,
        contract.json_sha256({"raw_request": raw}),
        paths,
        tuple(dict.fromkeys(change_types)),
        tuple(dict.fromkeys(risk_flags)),
        tuple(dict.fromkeys(stop_codes)),
        "repository_maintenance" if paths and not stop_codes else "owner_review_required",
    )


@dataclass(frozen=True)
class MissionProposal:
    """A digest-bound shadow proposal, not a Mission or an authority grant."""

    schema_version: str
    proposal_id: str
    source_request_sha256: str
    objective_kind: str
    requested_paths: tuple[str, ...]
    change_types: tuple[str, ...]
    risk_flags: tuple[str, ...]
    stop_codes: tuple[str, ...]
    proposal_sha256: str

    def proposal_wire(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "proposal_id": self.proposal_id,
            "source_request_sha256": self.source_request_sha256,
            "objective_kind": self.objective_kind,
            "requested_paths": list(self.requested_paths),
            "change_types": list(self.change_types),
            "risk_flags": list(self.risk_flags),
            "stop_codes": list(self.stop_codes),
        }

    def to_wire(self) -> dict[str, Any]:
        return {**self.proposal_wire(), "proposal_sha256": self.proposal_sha256}

    @classmethod
    def from_wire(cls, value: object) -> MissionProposal:
        fields = {
            "schema_version",
            "proposal_id",
            "source_request_sha256",
            "objective_kind",
            "requested_paths",
            "change_types",
            "risk_flags",
            "stop_codes",
            "proposal_sha256",
        }
        wire = _mapping(value, fields, fields, "proposal_fields_invalid")
        if wire["schema_version"] != SCHEMA_VERSION:
            raise ShadowStewardError("proposal_schema_unsupported")
        proposal = cls(
            SCHEMA_VERSION,
            _identifier(wire["proposal_id"], "proposal_id"),
            _sha(wire["source_request_sha256"], "source_request_sha256"),
            _text(wire["objective_kind"], "objective_kind", max_chars=64),
            _validated_non_private_paths(wire["requested_paths"], "requested_paths"),
            _items(wire["change_types"], "change_types", allowed=frozenset(SAFE_CHANGE_TYPES)),
            _optional_items(wire["risk_flags"], "risk_flags"),
            _optional_items(wire["stop_codes"], "stop_codes"),
            _sha(wire["proposal_sha256"], "proposal_sha256"),
        )
        if any(code not in KNOWN_STOP_CODES for code in proposal.stop_codes):
            raise ShadowStewardError("stop_codes_invalid")
        if proposal.proposal_sha256 != contract.json_sha256(proposal.proposal_wire()):
            raise ShadowStewardError("proposal_digest_mismatch")
        return proposal


def compile_proposal(raw_request: str) -> MissionProposal:
    """Compile one redacted intake into a deterministic shadow proposal."""

    intake = compile_intake(raw_request)
    wire = {
        "schema_version": SCHEMA_VERSION,
        "proposal_id": "pending",
        "source_request_sha256": intake.request_sha256,
        "objective_kind": intake.intent,
        "requested_paths": list(intake.requested_paths),
        "change_types": list(intake.change_types),
        "risk_flags": list(intake.risk_flags),
        "stop_codes": list(intake.stop_codes),
    }
    proposal_sha = contract.json_sha256({**wire, "proposal_id": "shadow-proposal"})
    proposal_id = f"shadow-proposal-{proposal_sha[:16]}"
    proposal_wire = {**wire, "proposal_id": proposal_id}
    proposal = MissionProposal(
        SCHEMA_VERSION,
        proposal_id,
        intake.request_sha256,
        intake.intent,
        intake.requested_paths,
        intake.change_types,
        intake.risk_flags,
        intake.stop_codes,
        contract.json_sha256(proposal_wire),
    )
    return MissionProposal.from_wire(proposal.to_wire())


@dataclass(frozen=True)
class ProposalDecision:
    schema_version: str
    proposal_sha256: str
    status: str
    mission_id: str | None
    owner_authenticated: bool
    recommendation_active: bool
    authority_consumed: bool
    mutation_allowed: bool
    stop_codes: tuple[str, ...]

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "proposal_sha256": self.proposal_sha256,
            "status": self.status,
            "mission_id": self.mission_id,
            "owner_authenticated": self.owner_authenticated,
            "recommendation_active": self.recommendation_active,
            "authority_consumed": self.authority_consumed,
            "mutation_allowed": self.mutation_allowed,
            "stop_codes": list(self.stop_codes),
        }


def evaluate_proposal(
    proposal: MissionProposal,
    mission: contract.MaintenanceMission,
    owner_approval: contract.OwnerApproval | dict[str, Any] | None,
    *,
    owner_authenticator: OwnerApprovalAuthenticator | None = None,
) -> ProposalDecision:
    """Evaluate exact owner approval without activating or consuming authority."""

    current = _validate_mission(mission)
    if not isinstance(proposal, MissionProposal):
        raise ShadowStewardError("proposal_invalid")
    try:
        model = MissionProposal.from_wire(proposal.to_wire())
    except ShadowStewardError:
        return ProposalDecision(
            SCHEMA_VERSION,
            "0" * 64,
            "REJECTED",
            None,
            False,
            False,
            False,
            False,
            ("SAFETY_CONFLICT",),
        )
    if owner_approval is None:
        return ProposalDecision(
            SCHEMA_VERSION,
            model.proposal_sha256,
            "WAITING_APPROVAL",
            current.mission_id,
            False,
            False,
            False,
            False,
            model.stop_codes,
        )
    try:
        approval_wire = (
            owner_approval.to_wire()
            if isinstance(owner_approval, contract.OwnerApproval)
            else owner_approval
        )
        approval = contract.OwnerApproval.from_wire(approval_wire)
        if approval.owner_identity not in contract.TRUSTED_OWNER_IDENTITIES:
            raise contract.MissionContractError("owner_approval_identity_untrusted")
    except contract.MissionContractError:
        return ProposalDecision(
            SCHEMA_VERSION,
            model.proposal_sha256,
            "REJECTED",
            None,
            False,
            False,
            False,
            False,
            ("AUTHORITY_REQUIRED",),
        )
    if approval.proposal_sha256 != model.proposal_sha256:
        return ProposalDecision(
            SCHEMA_VERSION,
            model.proposal_sha256,
            "REJECTED",
            None,
            False,
            False,
            False,
            False,
            ("SAFETY_CONFLICT",),
        )
    if owner_authenticator is None:
        return ProposalDecision(
            SCHEMA_VERSION,
            model.proposal_sha256,
            "WAITING_AUTHENTICATION",
            current.mission_id,
            False,
            False,
            False,
            False,
            ("AUTHORITY_REQUIRED",),
        )
    try:
        authenticated = owner_authenticator.verify(approval, model.proposal_sha256)
    except Exception:  # an untrusted adapter cannot turn an exception into approval
        authenticated = False
    if authenticated is not True:
        return ProposalDecision(
            SCHEMA_VERSION,
            model.proposal_sha256,
            "REJECTED",
            None,
            False,
            False,
            False,
            False,
            ("AUTHORITY_REQUIRED",),
        )
    if model.stop_codes:
        return ProposalDecision(
            SCHEMA_VERSION,
            model.proposal_sha256,
            "OWNER_STOP",
            current.mission_id,
            True,
            False,
            False,
            False,
            model.stop_codes,
        )
    if (
        not model.requested_paths
        or any(
        not contract.path_in_scope(current.allowed_paths, path.rstrip("/"))
        for path in model.requested_paths
        )
        or any(
            change_type not in current.allowed_change_types
            for change_type in model.change_types
        )
    ):
        return ProposalDecision(
            SCHEMA_VERSION,
            model.proposal_sha256,
            "OWNER_STOP",
            current.mission_id,
            True,
            False,
            False,
            False,
            ("SCOPE_EXCEEDED",),
        )
    return ProposalDecision(
        SCHEMA_VERSION,
        model.proposal_sha256,
        "SHADOW_RECOMMENDATION",
        current.mission_id,
        True,
        True,
        False,
        False,
        (),
    )


@dataclass(frozen=True)
class StopRecommendation:
    schema_version: str
    code: str
    category: str
    pause_owner: bool
    retry_allowed: bool
    reason: str

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "code": self.code,
            "category": self.category,
            "pause_owner": self.pause_owner,
            "retry_allowed": self.retry_allowed,
            "reason": self.reason,
        }


def classify_stop(code: str) -> StopRecommendation:
    """Map a typed failure to routine recovery or an owner pause."""

    if not isinstance(code, str):
        code = "SAFETY_CONFLICT"
    normalized = code.strip().upper()
    if normalized in ROUTINE_FAILURES:
        return StopRecommendation(
            SCHEMA_VERSION,
            normalized,
            "ROUTINE_RECOVERY",
            False,
            True,
            "bounded_failure_recovery",
        )
    if normalized == "BUDGET_EXCEEDED":
        return StopRecommendation(
            SCHEMA_VERSION,
            normalized,
            "PAUSED_FOR_OWNER",
            True,
            False,
            "mission_budget_exhausted",
        )
    if normalized in OWNER_FAILURES or normalized not in ROUTINE_FAILURES:
        known_code = normalized if normalized in OWNER_FAILURES else "SAFETY_CONFLICT"
        reason = {
            "AUTHORITY_REQUIRED": "authority_or_external_effect_requested",
            "SCOPE_EXCEEDED": "requested_scope_is_not_bounded",
            "REQUIREMENT_CONFLICT": "requirements_cannot_be_reconciled",
            "EXTERNAL_OUTCOME_UNKNOWN": "unknown_external_outcome_must_be_reconciled",
            "SAFETY_CONFLICT": "safety_or_input_uncertainty_requires_owner",
        }.get(known_code, "unrecognized_failure_requires_owner")
        return StopRecommendation(
            SCHEMA_VERSION,
            known_code,
            "PAUSED_FOR_OWNER",
            True,
            False,
            reason,
        )
    raise AssertionError("unreachable stop classification")


@dataclass(frozen=True)
class PlanProjection:
    schema_version: str
    proposal_sha256: str
    mission_id: str
    disposition: str
    stage: contract.Stage | None
    workcards: tuple[contract.WorkCard, ...]
    stop: StopRecommendation | None
    replan_count: int
    projection_only: bool = True
    _provenance: object | None = field(default=None, repr=False, compare=False)

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "proposal_sha256": self.proposal_sha256,
            "mission_id": self.mission_id,
            "disposition": self.disposition,
            "stage": self.stage.to_wire() if self.stage is not None else None,
            "workcards": [card.to_wire() for card in self.workcards],
            "stop": self.stop.to_wire() if self.stop is not None else None,
            "replan_count": self.replan_count,
            "projection_only": self.projection_only,
        }


def _waiting_projection(proposal_sha256: str, mission_id: str) -> PlanProjection:
    return PlanProjection(
        SCHEMA_VERSION,
        proposal_sha256,
        mission_id,
        "WAITING_APPROVAL",
        None,
        (),
        None,
        0,
    )


def plan_stage(
    proposal: MissionProposal,
    mission: contract.MaintenanceMission,
    owner_approval: contract.OwnerApproval | dict[str, Any] | None = None,
    *,
    owner_authenticator: OwnerApprovalAuthenticator | None = None,
) -> PlanProjection:
    """Build and validate a deterministic Stage/WorkCard recommendation."""

    current = _validate_mission(mission)
    if not isinstance(proposal, MissionProposal):
        raise ShadowStewardError("proposal_invalid")
    model = MissionProposal.from_wire(proposal.to_wire())
    if model.stop_codes:
        stop = classify_stop(model.stop_codes[0])
        return _paused_projection(model.proposal_sha256, current.mission_id, stop)
    if (
        not model.requested_paths
        or any(
        not contract.path_in_scope(current.allowed_paths, path.rstrip("/"))
        for path in model.requested_paths
        )
        or any(
            change_type not in current.allowed_change_types
            for change_type in model.change_types
        )
    ):
        stop = classify_stop("SCOPE_EXCEEDED")
        return _paused_projection(model.proposal_sha256, current.mission_id, stop)
    decision = evaluate_proposal(
        model,
        current,
        owner_approval,
        owner_authenticator=owner_authenticator,
    )
    if decision.status != "SHADOW_RECOMMENDATION":
        return _waiting_projection(model.proposal_sha256, current.mission_id)
    stage_id = f"shadow-stage-{model.proposal_sha256[:16]}"
    card_id = f"{stage_id}:card"
    stage = contract.Stage(
        stage_id,
        current.mission_id,
        "Provider-free bounded repository maintenance recommendation.",
        current.repository_identity,
        ("focused_checks_required", "full_checks_required", "exact_head_review_required"),
        ("legacy_controller_remains_lifecycle_writer", "no_external_effects"),
        (card_id,),
        current.rollback,
        None,
        None,
    )
    card = contract.WorkCard(
        card_id,
        stage_id,
        model.requested_paths,
        ("outside-approved-scope/",),
        ("Apply only the bounded approved change.",),
        current.quality_checks,
        ("Reject scope, authority, secret, and unknown-outcome expansion.",),
        ("redacted verification receipts", "exact-head review and CI"),
        (),
        model.requested_paths,
        1,
        "T1",
        current.rollback,
        "PENDING",
    )
    try:
        contract.validate_stage(stage, current, (card,))
    except contract.MissionContractError as exc:
        raise ShadowStewardError("stage_projection_invalid") from exc
    return PlanProjection(
        SCHEMA_VERSION,
        model.proposal_sha256,
        current.mission_id,
        "PLANNED",
        stage,
        (card,),
        None,
        0,
        True,
        _PROJECTION_TOKEN,
    )


def replan(plan: PlanProjection, failure_code: str, *, attempt_number: int = 1) -> PlanProjection:
    """Return a recovery or owner-pause projection without changing state."""

    if (
        not isinstance(plan, PlanProjection)
        or not plan.projection_only
        or plan._provenance is not _PROJECTION_TOKEN
    ):
        raise ShadowStewardError("plan_projection_invalid")
    if not isinstance(attempt_number, int) or attempt_number < 1:
        raise ShadowStewardError("attempt_number_invalid")
    stop = classify_stop(failure_code)
    try:
        max_retries = contract.validate_registered_campaign().budget.max_retries
    except contract.MissionContractError as exc:
        raise ShadowStewardError("registered_mission_invalid") from exc
    if stop.pause_owner or attempt_number > max_retries:
        if attempt_number > max_retries and not stop.pause_owner:
            stop = classify_stop("BUDGET_EXCEEDED")
        return replace(
            plan,
            disposition="PAUSED_FOR_OWNER",
            stage=None,
            workcards=(),
            stop=stop,
            replan_count=plan.replan_count + 1,
        )
    cards = tuple(replace(card, result_state="REPLAN_REQUIRED") for card in plan.workcards)
    if plan.stage is not None:
        try:
            contract.validate_stage(plan.stage, contract.campaign_mission(), cards)
        except contract.MissionContractError as exc:
            raise ShadowStewardError("replan_projection_invalid") from exc
    return replace(
        plan,
        disposition="RECOVERY_RECOMMENDED",
        workcards=cards,
        stop=stop,
        replan_count=plan.replan_count + 1,
    )


@dataclass(frozen=True)
class ReplayCase:
    case_id: str
    source: str
    failure_code: str
    evidence_ref: str
    evidence_sha256: str

    def to_wire(self) -> dict[str, str]:
        return {
            "case_id": self.case_id,
            "source": self.source,
            "failure_code": self.failure_code,
            "evidence_ref": self.evidence_ref,
            "evidence_sha256": self.evidence_sha256,
        }

    @staticmethod
    def _evidence_digest(
        case_id: str,
        source: str,
        failure_code: str,
        evidence_ref: str,
    ) -> str:
        return contract.json_sha256(
            {
                "case_id": case_id,
                "source": source,
                "failure_code": failure_code,
                "evidence_ref": evidence_ref,
            }
        )

    @classmethod
    def fixture(
        cls,
        case_id: str,
        source: str,
        failure_code: str,
        evidence_ref: str,
    ) -> ReplayCase:
        failure = failure_code.strip().upper()
        return cls(
            case_id,
            source,
            failure,
            evidence_ref,
            cls._evidence_digest(case_id, source, failure, evidence_ref),
        )

    @classmethod
    def from_wire(cls, value: object) -> ReplayCase:
        fields = {
            "case_id",
            "source",
            "failure_code",
            "evidence_ref",
            "evidence_sha256",
        }
        wire = _mapping(value, fields, fields, "replay_case_fields_invalid")
        case_id = _identifier(wire["case_id"], "case_id")
        source = _text(wire["source"], "source", max_chars=16)
        if source not in KNOWN_SOURCES:
            raise ShadowStewardError("replay_source_invalid")
        failure = _text(wire["failure_code"], "failure_code", max_chars=64).upper()
        if failure not in KNOWN_STOP_CODES:
            raise ShadowStewardError("replay_failure_invalid")
        evidence_ref = _text(wire["evidence_ref"], "evidence_ref", max_chars=128)
        if re.fullmatch(
            r"github:(?:issue|pr|ci|review):[1-9][0-9]{0,11}:[a-z0-9-]+",
            evidence_ref,
        ) is None:
            raise ShadowStewardError("evidence_ref_invalid")
        evidence_sha = _sha(wire["evidence_sha256"], "evidence_sha256")
        if evidence_sha != cls._evidence_digest(
            case_id, source, failure, evidence_ref
        ):
            raise ShadowStewardError("replay_evidence_digest_mismatch")
        expected = HISTORICAL_EVIDENCE.get(evidence_ref)
        if expected is None or expected[:2] != (source, failure):
            raise ShadowStewardError("historical_evidence_binding_invalid")
        return cls(case_id, source, failure, evidence_ref, evidence_sha)


@dataclass(frozen=True)
class ReplayCaseResult:
    case_id: str
    source: str
    failure_code: str
    evidence_ref: str
    evidence_sha256: str
    legacy_action: str
    shadow_action: str
    category: str
    match: bool

    def to_wire(self) -> dict[str, Any]:
        return {
            "case_id": self.case_id,
            "source": self.source,
            "failure_code": self.failure_code,
            "evidence_ref": self.evidence_ref,
            "evidence_sha256": self.evidence_sha256,
            "legacy_action": self.legacy_action,
            "shadow_action": self.shadow_action,
            "category": self.category,
            "match": self.match,
        }


@dataclass(frozen=True)
class ReplayResult:
    schema_version: str
    case_count: int
    ordinary_failure_count: int
    false_pause_count: int
    owner_pause_count: int
    mismatch_count: int
    comparison_sha256: str
    cases: tuple[ReplayCaseResult, ...]

    @property
    def passed(self) -> bool:
        return self.false_pause_count == 0 and self.mismatch_count == 0

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "case_count": self.case_count,
            "ordinary_failure_count": self.ordinary_failure_count,
            "false_pause_count": self.false_pause_count,
            "owner_pause_count": self.owner_pause_count,
            "mismatch_count": self.mismatch_count,
            "comparison_sha256": self.comparison_sha256,
            "cases": [case.to_wire() for case in self.cases],
        }


def _legacy_controller_action(case: ReplayCase) -> str:
    """Reconstruct the legacy disposition from the fixed evidence manifest."""

    return "RECOVER" if HISTORICAL_EVIDENCE[case.evidence_ref][2] == "failed" else "PAUSE"


def historical_failure_fixtures() -> tuple[ReplayCase, ...]:
    """Return sanitized, hash-bound historical Issue/PR/CI/review fixtures."""

    return (
        ReplayCase.fixture(
            "issue-77-worker",
            "issue",
            "WORKER_FAILED",
            "github:issue:77:worker-failed",
        ),
        ReplayCase.fixture(
            "pr-630-review",
            "pr",
            "REVIEW_CHANGES_REQUESTED",
            "github:pr:630:review-changes-requested",
        ),
        ReplayCase.fixture(
            "ci-33108617013-macos",
            "ci",
            "CI_FAILED",
            "github:ci:33108617013:macos-startup-race",
        ),
        ReplayCase.fixture(
            "review-630-unknown",
            "review",
            "EXTERNAL_OUTCOME_UNKNOWN",
            "github:review:630:unknown-outcome",
        ),
        ReplayCase.fixture(
            "issue-77-safety",
            "issue",
            "SAFETY_CONFLICT",
            "github:issue:77:safety-conflict",
        ),
    )


def replay_historical_failures(
    cases: tuple[ReplayCase, ...] | list[ReplayCase] | None = None,
) -> ReplayResult:
    """Compare bounded fixture decisions from the legacy and shadow paths."""

    if cases is None:
        cases = historical_failure_fixtures()
    if not isinstance(cases, (list, tuple)) or not cases or len(cases) > MAX_FAILURES:
        raise ShadowStewardError("replay_cases_invalid")
    normalized = tuple(ReplayCase.from_wire(case.to_wire()) for case in cases)
    if len({case.case_id for case in normalized}) != len(normalized):
        raise ShadowStewardError("replay_case_ids_duplicated")
    results: list[ReplayCaseResult] = []
    ordinary = false_pause = owner_pause = mismatches = 0
    for case in normalized:
        stop = classify_stop(case.failure_code)
        shadow_action = "PAUSE" if stop.pause_owner else "RECOVER"
        if stop.pause_owner:
            owner_pause += 1
        else:
            ordinary += 1
            if shadow_action == "PAUSE":
                false_pause += 1
        legacy_action = _legacy_controller_action(case)
        match = legacy_action == shadow_action
        if not match:
            mismatches += 1
        results.append(
            ReplayCaseResult(
                case.case_id,
                case.source,
                case.failure_code,
                case.evidence_ref,
                case.evidence_sha256,
                legacy_action,
                shadow_action,
                stop.category,
                match,
            )
        )
    comparison_sha = contract.json_sha256(
        {
            "schema_version": SCHEMA_VERSION,
            "cases": [result.to_wire() for result in results],
        }
    )
    return ReplayResult(
        SCHEMA_VERSION,
        len(results),
        ordinary,
        false_pause,
        owner_pause,
        mismatches,
        comparison_sha,
        tuple(results),
    )


@dataclass(frozen=True)
class CompactStatus:
    schema_version: str
    status: str
    mission_id: str
    proposal_sha256: str
    stage_id: str | None
    workcard_count: int
    completed_workcards: int
    stop_code: str | None
    stop_category: str | None
    replay_comparison_sha256: str | None
    replay_case_count: int
    projection_only: bool
    authority_consumed: bool
    mutation_allowed: bool

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "status": self.status,
            "mission_id": self.mission_id,
            "proposal_sha256": self.proposal_sha256,
            "stage_id": self.stage_id,
            "workcard_count": self.workcard_count,
            "completed_workcards": self.completed_workcards,
            "stop_code": self.stop_code,
            "stop_category": self.stop_category,
            "replay_comparison_sha256": self.replay_comparison_sha256,
            "replay_case_count": self.replay_case_count,
            "projection_only": self.projection_only,
            "authority_consumed": self.authority_consumed,
            "mutation_allowed": self.mutation_allowed,
        }


def compact_status(plan: PlanProjection, replay: ReplayResult | None = None) -> CompactStatus:
    """Project only counts, identifiers, digests, and typed dispositions."""

    if not isinstance(plan, PlanProjection) or not plan.projection_only:
        raise ShadowStewardError("plan_projection_invalid")
    completed = sum(card.result_state == "COMPLETE" for card in plan.workcards)
    return CompactStatus(
        SCHEMA_VERSION,
        plan.disposition,
        plan.mission_id,
        plan.proposal_sha256,
        plan.stage.stage_id if plan.stage is not None else None,
        len(plan.workcards),
        completed,
        plan.stop.code if plan.stop is not None else None,
        plan.stop.category if plan.stop is not None else None,
        replay.comparison_sha256 if replay is not None else None,
        replay.case_count if replay is not None else 0,
        True,
        False,
        False,
    )


def shadow_only(plan: PlanProjection) -> bool:
    """Small explicit guard for callers that must not confuse projection with authority."""

    return (
        isinstance(plan, PlanProjection)
        and plan.projection_only
        and plan._provenance is _PROJECTION_TOKEN
        and not plan.to_wire().get("mutation_allowed", False)
    )
