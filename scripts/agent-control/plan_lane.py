"""Strict parser and identity contract for plan-derived local candidates.

The canonical document is the only source of plan authority.  A poll result is
bounded transport only; callers must re-read the accepted-main document before
claiming or mutating anything.  Plan subjects consume the accepted
``weak-agent-dispatch:v1`` / session-context representation; no second semantic
plan contract may be invented.
"""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from typing import Any

import artifact_contract


SCHEMA_VERSION = 1
MAX_DOCUMENT_BYTES = 512 * 1024
MAX_FIELD_CHARS = 8 * 1024
PLAN_LANE_ACTIVE = "plan_lane_active"
PLAN_LANE_DEFERRED = "plan_lane_deferred_until_terminal_owners"
KNOWN_PLAN_LANE_STATES = frozenset({PLAN_LANE_ACTIVE, PLAN_LANE_DEFERRED})
PACKET_ID = re.compile(r"^(?:PE[0-9]+|PR[0-9]+|TOOL|CI|PRODUCT)(?:-[A-Z0-9]+)+$")
PACKET_TOKEN = r"(?:PE[0-9]+|PR[0-9]+|TOOL|CI|PRODUCT)(?:-[A-Z0-9]+)+"
SHA40 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
WEAK_DISPATCH_MARKER = re.compile(
    r"<!--\s*weak-agent-dispatch:v1\s*(\{.*?\})\s*-->", re.DOTALL
)
PACKET_HEADING = re.compile(
    rf"^#{{2,3}} Packet (?P<packet>(?:PE[0-9]+|PR[0-9]+|TOOL|CI|PRODUCT)(?:-[A-Z0-9]+)+)\b.*$",
    re.MULTILINE,
)
PACKET_HISTORICAL_HEADING = re.compile(
    rf"^#{{2,3}} (?:Completed|Retained) .*?\((?P<packet>{PACKET_TOKEN})\)\s*$",
    re.MULTILINE,
)
PACKET_STATE = re.compile(r"^\*\*State:\*\* `(?P<state>[A-Z_]+)`", re.MULTILINE)
PACKET_HISTORICAL_STATE = re.compile(
    r"^\*\*Historical state:\*\* `(?P<state>[A-Z_]+)`", re.MULTILINE
)
ACTIVE_ROUTING = re.compile(r"^\s*\d+\.\s+`(?P<packet>[A-Za-z0-9-]+)`", re.MULTILINE)


class PlanLaneError(ValueError):
    """Raised when the canonical plan document cannot authorize a candidate."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


@dataclass(frozen=True)
class PlanCandidate:
    packet_id: str
    source_main_sha: str
    task_spec_sha256: str
    goal: str
    allowed_paths: list[str]
    prerequisites: list[str]
    forbidden_changes: list[str]
    verification: list[str]
    rollback: list[str]

    @property
    def branch(self) -> str:
        return f"agent/packet-{self.packet_id.lower()}"

    def spec_wire(self) -> dict[str, Any]:
        return {
            "schema_version": SCHEMA_VERSION,
            "packet_id": self.packet_id,
            "state": "READY_FOR_EXECUTION",
            "source_main_sha": self.source_main_sha,
            "goal": self.goal,
            "allowed_paths": self.allowed_paths,
            "prerequisites": self.prerequisites,
            "forbidden_changes": self.forbidden_changes,
            "verification": self.verification,
            "rollback": self.rollback,
        }

    def to_wire(self) -> dict[str, Any]:
        return {
            "candidate_kind": "plan",
            "subject_kind": "plan-packet",
            "subject_id": self.packet_id,
            "source_main_sha": self.source_main_sha,
            "task_spec_sha256": self.task_spec_sha256,
            "goal": self.goal,
            "allowed_paths": list(self.allowed_paths),
            "prerequisites": list(self.prerequisites),
            "forbidden_changes": list(self.forbidden_changes),
            "verification": list(self.verification),
            "rollback": list(self.rollback),
            "branch": self.branch,
        }


def _bounded_string(value: Any, field: str) -> str:
    if not isinstance(value, str) or not value.strip() or len(value) > MAX_FIELD_CHARS:
        raise PlanLaneError(f"plan_{field}_missing_or_invalid")
    return value.strip()


def _bounded_strings(value: Any, field: str, *, allow_empty: bool = False) -> list[str]:
    if not isinstance(value, list) or len(value) > 50:
        raise PlanLaneError(f"plan_{field}_missing_or_invalid")
    result = [_bounded_string(item, field) for item in value]
    if not allow_empty and not result:
        raise PlanLaneError(f"plan_{field}_missing_or_invalid")
    if len(result) != len(set(result)):
        raise PlanLaneError(f"plan_{field}_duplicated")
    return result


def _packet_blocks(document: str) -> list[tuple[str, str, int, int, str]]:
    headings = sorted(
        list(PACKET_HEADING.finditer(document))
        + list(PACKET_HISTORICAL_HEADING.finditer(document)),
        key=lambda match: match.start(),
    )
    blocks: list[tuple[str, str, int, int, str]] = []
    for index, heading in enumerate(headings):
        end = headings[index + 1].start() if index + 1 < len(headings) else len(document)
        block = document[heading.start() : end]
        states = PACKET_STATE.findall(block)
        if not states:
            states = PACKET_HISTORICAL_STATE.findall(block)
        if len(states) != 1:
            raise PlanLaneError("plan_packet_state_missing_or_ambiguous")
        blocks.append((heading.group("packet"), states[0], heading.start(), end, block))
    return blocks


def _canonical_spec(payload: dict[str, Any]) -> str:
    return hashlib.sha256(
        json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()


def parse(document: str, accepted_main_sha: str) -> PlanCandidate:
    """Parse exactly one explicit READY packet from accepted canonical prose.

    The candidate is derived from the accepted ``weak-agent-dispatch:v1``
    capsule inside the current packet block — the same representation the
    session entry and handoff checker consume.  ``task_spec_sha256`` remains
    the canonical digest of the derived spec so a mutated capsule can never
    mint a new binding.
    """

    if not isinstance(document, str) or len(document.encode("utf-8")) > MAX_DOCUMENT_BYTES:
        raise PlanLaneError("plan_document_unavailable_or_too_large")
    if not SHA40.fullmatch(accepted_main_sha or ""):
        raise PlanLaneError("plan_accepted_main_invalid")
    blocks = _packet_blocks(document)
    if not blocks:
        raise PlanLaneError("plan_packet_absent")
    ready = [packet for packet, state, _start, _end, _block in blocks if state == "READY_FOR_EXECUTION"]
    if len(ready) == 0:
        raise PlanLaneError("plan_packet_absent")
    if len(ready) != 1:
        raise PlanLaneError("multiple_plan_packets")
    marker_matches = list(WEAK_DISPATCH_MARKER.finditer(document))
    if not marker_matches:
        raise PlanLaneError("plan_packet_fields_missing")
    if len(marker_matches) != 1:
        raise PlanLaneError("multiple_plan_packets")
    marker = marker_matches[0]
    packet_id, structural_state, block_start, block_end, _block = next(
        (item for item in blocks if item[0] == ready[0]), ("", "", -1, -1, "")
    )
    if marker.start() < block_start or marker.end() > block_end:
        raise PlanLaneError("plan_marker_outside_ready_packet")
    try:
        payload = json.loads(marker.group(1))
    except json.JSONDecodeError as exc:
        raise PlanLaneError("plan_packet_fields_invalid") from exc
    if not isinstance(payload, dict):
        raise PlanLaneError("plan_packet_fields_invalid")
    if payload.get("schema_version") != "weak_agent_dispatch.v1":
        raise PlanLaneError("plan_packet_version_unsupported")
    if payload.get("packet_id") != packet_id or not PACKET_ID.fullmatch(packet_id):
        raise PlanLaneError("plan_packet_id_invalid")
    if payload.get("packet_state") != structural_state or structural_state != "READY_FOR_EXECUTION":
        raise PlanLaneError("plan_packet_state_invalid")
    lane_state = payload.get("plan_lane_state")
    if lane_state not in KNOWN_PLAN_LANE_STATES:
        raise PlanLaneError("plan_lane_state_invalid")
    if lane_state != PLAN_LANE_ACTIVE:
        raise PlanLaneError("plan_lane_deferred_until_terminal_owners")
    if type(payload.get("external_effect_limit")) is not int or payload.get("external_effect_limit") != 0:
        raise PlanLaneError("plan_external_effect_not_zero")
    if "goal" not in payload:
        raise PlanLaneError("plan_goal_missing_or_invalid")
    goal = _bounded_string(payload["goal"], "goal")
    if "allowed_paths" not in payload:
        raise PlanLaneError("plan_allowed_paths_missing_or_invalid")
    allowed_paths = _bounded_strings(payload["allowed_paths"], "allowed_paths")
    try:
        allowed_paths = artifact_contract.validate_allowed_paths(allowed_paths)
    except artifact_contract.ArtifactContractError as exc:
        raise PlanLaneError("plan_allowed_paths_invalid") from exc
    prerequisites = _bounded_strings(payload["prerequisites"], "prerequisites", allow_empty=True)
    if any(not PACKET_ID.fullmatch(item) for item in prerequisites):
        raise PlanLaneError("plan_prerequisites_invalid")
    states = {item[0]: item[1] for item in blocks}
    missing = [item for item in prerequisites if states.get(item) != "COMPLETE"]
    if missing:
        raise PlanLaneError("plan_dependencies_not_ready")
    forbidden_changes = _bounded_strings(payload["forbidden_changes"], "forbidden_changes")
    verification = _bounded_strings(payload["verification"], "verification")
    rollback = [_bounded_string(payload["rollback"], "rollback")]
    spec = {
        "schema_version": SCHEMA_VERSION,
        "packet_id": packet_id,
        "state": "READY_FOR_EXECUTION",
        "source_main_sha": accepted_main_sha,
        "goal": goal,
        "allowed_paths": allowed_paths,
        "prerequisites": prerequisites,
        "forbidden_changes": forbidden_changes,
        "verification": verification,
        "rollback": rollback,
    }
    routed = list(ACTIVE_ROUTING.finditer(document))
    if not routed or routed[0].group("packet") != packet_id:
        raise PlanLaneError("plan_packet_is_not_current_route")
    return PlanCandidate(
        packet_id=packet_id,
        source_main_sha=accepted_main_sha,
        task_spec_sha256=_canonical_spec(spec),
        goal=goal,
        allowed_paths=allowed_paths,
        prerequisites=prerequisites,
        forbidden_changes=forbidden_changes,
        verification=verification,
        rollback=rollback,
    )


def terminal_owner_readiness(
    *,
    ledger_issue: object,
    canonical_tests_workflow_present: bool,
    ci_monitor_workflow_present: bool,
    review_owner_present: bool,
    merge_owner_present: bool,
    closeout_owner_present: bool,
) -> tuple[bool, list[str]]:
    """Prove, fail-closed, that each terminal owner can bind a plan subject.

    A plan subject is admitted only when every terminal owner is provably
    usable for that exact packet: the Plan Execution Ledger Issue resolves,
    the canonical CI workflow and CI monitor workflow exist, the review owner
    accepts PR-bound subjects, the repository-maintenance merge owner exists,
    and the canonical closeout owner accepts the packet.  Missing or
    unverifiable owners reject the candidate; no owner may be inferred.
    """

    missing: list[str] = []
    if not isinstance(ledger_issue, int) or ledger_issue <= 0:
        missing.append("plan_execution_ledger")
    if not canonical_tests_workflow_present:
        missing.append("canonical_tests_workflow")
    if not ci_monitor_workflow_present:
        missing.append("ci_monitor_workflow")
    if not review_owner_present:
        missing.append("review_owner")
    if not merge_owner_present:
        missing.append("merge_owner")
    if not closeout_owner_present:
        missing.append("closeout_owner")
    return (not missing, missing)


def parse_optional(document: str, accepted_main_sha: str) -> PlanCandidate | None:
    """Return no candidate only for an actually absent plan lane."""

    try:
        return parse(document, accepted_main_sha)
    except PlanLaneError as exc:
        if exc.reason == "plan_packet_absent":
            return None
        raise


def successor_binding(document: str, closed_packet_id: str, accepted_main_sha: str) -> tuple[str, str]:
    """Return ``(packet_id, capsule_digest)`` of exactly one eligible successor.

    After a plan packet closes out, the live accepted routing must name
    exactly one current ``READY_FOR_EXECUTION`` packet different from the
    closed packet, with a parseable dispatch capsule; the canonical capsule
    digest is the promotion binding.  The closed packet still being current,
    zero candidates, or multiple candidates all fail closed.
    """

    if not isinstance(closed_packet_id, str) or PACKET_ID.fullmatch(closed_packet_id) is None:
        raise PlanLaneError("successor_closed_packet_invalid")
    candidate = parse(document, accepted_main_sha)
    if candidate.packet_id == closed_packet_id:
        raise PlanLaneError("successor_still_current")
    return candidate.packet_id, candidate.task_spec_sha256
