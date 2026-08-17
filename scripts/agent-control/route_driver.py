"""Promotion compiler and continuous route driver for the Plan lane.

The deterministic, provider-free layer parses the checked
``future-route-inventory:v1`` manifest and eligible successor sketch only to
prove route identity, prerequisites, class/profile, manifest integrity, and
predecessor receipt.  A separate bounded promotion planner must then supply
and the deterministic verifier must re-prove all refreshed current-main owner,
caller, test, path, slice, verification, operations, evidence, and decision
facts.  Static ``FUTURE_ROUTE`` paths are hints only and never promotion
authority.  Only then does the compiler create replace-only successor-contract
updates for ``docs/NEXT_DECISION.md``, ``docs/FUTURE_ROUTE.md``, and
``docs/CURRENT_STATUS.md``.  Those documents are authoritative only after the
canonical closeout/promotion PR is independently reviewed, green on canonical
CI, and merged; compilation itself never executes a successor, writes a
target, or mints authority.

The driver entry points reuse the existing Plan Execution Ledger, PR binding,
lifecycle receipt, and merge-state owners; they add no second ledger,
controller, store, scheduler, or routing owner.
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import time
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any
import uuid

import artifact_contract
import plan_lane
import plan_lifecycle


INVENTORY_MARKER = re.compile(
    r"<!--\s*future-route-inventory:v1\s*(\{.*?\})\s*-->", re.DOTALL
)
NEXT_DECISION_MAX_BYTES = 64 * 1024
MAX_SKETCH_FIELD_CHARS = 8 * 1024
MAX_MANIFEST_BYTES = 256 * 1024
MAX_PROMOTION_EVIDENCE_BYTES = 64 * 1024
# The accepted managed-acceptance caller is a single 634 KiB Rust source file;
# keep the proof bounded while allowing that existing owner to be re-read.
MAX_CURRENT_MAIN_SOURCE_BYTES = 768 * 1024
_MAX_PROMOTION_LIST_ITEMS = 50
MAX_T3_RECEIPT_WINDOW = timedelta(minutes=15)
_ROUTE_EVIDENCE_SCHEMA = "route_promotion_evidence.v2"
_DECISION_KINDS = frozenset({"schema", "evaluator", "authority", "recovery"})
_SAFE_PROPOSAL_TOKEN = re.compile(r"^[A-Za-z0-9_./:-]{3,160}$")
_CODE_SYMBOL = re.compile(r"^[A-Za-z_][A-Za-z0-9_]{0,127}$")
_T3_REQUEST_MARKER = re.compile(
    r"<!--\s*route-t3-request:v1\s*(\{.*?\})\s*-->", re.DOTALL
)
_ROUTE_BOOTSTRAP_RECONCILE = re.compile(
    r"<!-- route-bootstrap-reconcile:v1 packet_id=(?P<packet>[A-Z0-9-]+) -->"
)

PACKET_CLASSES = frozenset({"CONTRACT", "IMPLEMENT", "EFFECT", "CLOSEOUT"})
CLASS_DEFAULT_TIER = {"CONTRACT": "T2", "IMPLEMENT": "T1", "EFFECT": "T3", "CLOSEOUT": "T2"}
CLASS_DEFAULT_RISK = {"EFFECT": "external_effect"}
CLASS_DEFAULT_VERIFICATION = {
    "CONTRACT": "docs_evidence_review",
    "IMPLEMENT": "source_focused_full",
    "EFFECT": "external_effect_evidence",
    "CLOSEOUT": "evidence_review",
}
T3_DECISION_SOURCES = frozenset({"human_operator", "local_sol_5_6_max", "gpt_web"})

_CLOSEOUT_FIELDS = ("Prerequisite", "Class", "Outcome", "Allowed delta", "Exit", "Stop")
_ROUTE_CLOSEOUT_PACKET_DETAIL = re.compile(
    rf"^route closeout packet `(?P<packet>{plan_lane.PACKET_TOKEN})`$"
)


class RouteDriverError(ValueError):
    """Raised when the checked routing cannot authorize a compiled successor."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


@dataclass(frozen=True)
class PacketSketch:
    packet_id: str
    prerequisites: tuple[str, ...]
    packet_class: str
    outcome: str
    allowed_delta: str
    exit_statement: str
    stop: str


@dataclass(frozen=True)
class EligibleSuccessor:
    packet_id: str
    sketch: PacketSketch
    profile: tuple[str, str, str, str, str]


@dataclass(frozen=True)
class CompiledSuccessor:
    packet_id: str
    capsule: dict[str, Any]
    spec_digest: str
    manifest_sha256: str
    branch: str
    next_document: str
    future_document: str
    status_document: str
    packet_state: str = "READY_FOR_EXECUTION"
    t3_request: T3Request | None = None


def _json_sha256(value: object) -> str:
    encoded = json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _bounded_text(value: str, field: str) -> str:
    if not value or len(value) > MAX_SKETCH_FIELD_CHARS:
        raise RouteDriverError(f"route_{field}_missing_or_invalid")
    return value.strip()


def _canonical_route_attempt(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    try:
        parsed = uuid.UUID(value)
    except ValueError:
        return None
    return value if value == str(parsed) else None


def _bounded_list(value: list[str], field: str) -> list[str]:
    result = [_bounded_text(item, field) for item in value]
    if len(result) > 50:
        raise RouteDriverError(f"route_{field}_too_large")
    if len(result) != len(set(result)):
        raise RouteDriverError(f"route_{field}_duplicated")
    return result


def inventory_manifest(future_document: str) -> dict[str, Any]:
    """Return the checked FUTURE_ROUTE inventory manifest, failing closed.

    The manifest must be the single ``future-route-inventory:v1`` marker and
    must agree with the prose packet order, dependency graph, profile rows,
    and digest fields exactly as the handoff checker derives them.  Any
    disagreement (missing marker, stale digest, mismatched profile, duplicate
    identity) is a routing failure, never a silent correction.
    """

    if not isinstance(future_document, str) or not future_document:
        raise RouteDriverError("route_future_route_unavailable")
    if len(future_document.encode("utf-8")) > plan_lane.MAX_DOCUMENT_BYTES:
        raise RouteDriverError("route_future_route_too_large")
    markers = list(INVENTORY_MARKER.finditer(future_document))
    if not markers:
        raise RouteDriverError("route_inventory_manifest_missing")
    if len(markers) != 1:
        raise RouteDriverError("route_inventory_manifest_duplicated")
    if len(markers[0].group(1).encode("utf-8")) > MAX_MANIFEST_BYTES:
        raise RouteDriverError("route_inventory_manifest_too_large")
    try:
        observed = json.loads(markers[0].group(1))
    except json.JSONDecodeError as exc:
        raise RouteDriverError("route_inventory_manifest_invalid") from exc
    if not isinstance(observed, dict):
        raise RouteDriverError("route_inventory_manifest_invalid")
    if observed.get("schema_version") != "future_route_inventory.v1":
        raise RouteDriverError("route_inventory_schema_unsupported")
    derived = _derive_inventory_payload(future_document)
    if observed.get("packet_count") != derived["packet_count"]:
        raise RouteDriverError("route_inventory_count_stale")
    if observed.get("ordered_packet_ids") != derived["ordered_packet_ids"]:
        raise RouteDriverError("route_inventory_order_stale")
    if observed.get("ordered_packet_ids_sha256") != derived["ordered_packet_ids_sha256"]:
        raise RouteDriverError("route_inventory_ids_sha_stale")
    if observed.get("dependency_graph_sha256") != derived["dependency_graph_sha256"]:
        raise RouteDriverError("route_inventory_graph_sha_stale")
    if observed.get("profiles") != derived["profiles"]:
        raise RouteDriverError("route_inventory_profiles_stale")
    if observed.get("profiles_sha256") != derived["profiles_sha256"]:
        raise RouteDriverError("route_inventory_profiles_sha_stale")
    return dict(observed)


def _derive_inventory_payload(future_document: str) -> dict[str, object]:
    """Derive the canonical inventory payload from the prose packet order.

    Shared by the manifest validator and the promotion compiler's manifest
    refresh so both agree on exactly one derivation of the ordered packet
    ids, dependency graph, and profile rows.
    """

    ordered: list[str] = []
    dependency_graph: list[dict[str, object]] = []
    profile_rows: list[list[object]] = []
    for match in plan_lane.PACKET_HEADING.finditer(future_document):
        packet_id = match.group("packet")
        ordered.append(packet_id)
        block = future_document[match.start() : _sketch_end(future_document, match)]
        prerequisite = re.search(r"^\*\*Prerequisite:\*\*\s*(?P<value>.+)$", block, re.MULTILINE)
        prerequisites = (
            re.findall(plan_lane.PACKET_TOKEN, prerequisite.group("value"))
            if prerequisite
            else []
        )
        prerequisites = list(
            dict.fromkeys(item for item in prerequisites if item != packet_id)
        )
        dependency_graph.append({"packet_id": packet_id, "prerequisites": prerequisites})
        row = _profile_row(block, packet_id)
        if row is None:
            raise RouteDriverError("route_profile_row_missing")
        profile_rows.append(row)
    return {
        "schema_version": "future_route_inventory.v1",
        "packet_count": len(ordered),
        "ordered_packet_ids": ordered,
        "ordered_packet_ids_sha256": _json_sha256(ordered),
        "dependency_graph_sha256": _json_sha256(dependency_graph),
        "profiles_sha256": _json_sha256(profile_rows),
        "profiles": profile_rows,
    }


def _sketch_end(future_document: str, heading: re.Match[str]) -> int:
    following = plan_lane.PACKET_HEADING.finditer(future_document, heading.end())
    try:
        return next(following).start()
    except StopIteration:
        return len(future_document)


def _profile_row(block: str, packet_id: str) -> list[object] | None:
    packet_class = re.search(
        r"^\*\*Class:\*\*\s*`?(?P<value>[A-Z]+)`?\s*$", block, re.MULTILINE
    )
    if packet_class is None or packet_class.group("value") not in PACKET_CLASSES:
        return None
    return [
        packet_id,
        packet_class.group("value"),
        CLASS_DEFAULT_TIER[packet_class.group("value")],
        CLASS_DEFAULT_RISK.get(packet_class.group("value"), "none"),
        CLASS_DEFAULT_VERIFICATION[packet_class.group("value")],
    ]


def packet_sketches(future_document: str) -> dict[str, PacketSketch]:
    """Parse exactly one bounded sketch per FUTURE_ROUTE packet, failing closed."""

    headings = list(plan_lane.PACKET_HEADING.finditer(future_document))
    sketches: dict[str, PacketSketch] = {}
    for index, match in enumerate(headings):
        packet_id = match.group("packet")
        end = headings[index + 1].start() if index + 1 < len(headings) else len(future_document)
        block = future_document[match.start() : end]
        values: dict[str, str] = {}
        for label in _CLOSEOUT_FIELDS:
            found = list(
                re.finditer(
                    rf"^\*\*{re.escape(label)}:\*\*\s*(?P<value>\S.*)$",
                    block,
                    re.MULTILINE,
                )
            )
            if len(found) != 1:
                raise RouteDriverError(f"route_sketch_field_missing_or_ambiguous:{label}")
            values[label] = _bounded_text(found[0].group("value").strip(), label)
        prerequisites = tuple(
            dict.fromkeys(
                item
                for item in re.findall(plan_lane.PACKET_TOKEN, values["Prerequisite"])
                if item != packet_id
            )
        )
        if any(not plan_lane.PACKET_ID.fullmatch(item) for item in prerequisites):
            raise RouteDriverError("route_sketch_prerequisites_invalid")
        packet_class = values["Class"].strip("` ").upper()
        if packet_class not in PACKET_CLASSES:
            raise RouteDriverError("route_sketch_class_invalid")
        if packet_id in sketches:
            raise RouteDriverError("route_sketch_duplicated")
        sketches[packet_id] = PacketSketch(
            packet_id=packet_id,
            prerequisites=prerequisites,
            packet_class=packet_class,
            outcome=values["Outcome"],
            allowed_delta=values["Allowed delta"],
            exit_statement=values["Exit"],
            stop=values["Stop"],
        )
    return sketches


def _packet_states(next_document: str) -> dict[str, str]:
    try:
        blocks = plan_lane._packet_blocks(next_document)
    except plan_lane.PlanLaneError as exc:
        raise RouteDriverError(f"route_next_document_invalid:{exc.reason}") from exc
    return {packet_id: state for packet_id, state, _start, _end, _block in blocks}


def eligible_successor(
    future_document: str,
    next_document: str,
    closed_packet_id: str,
    *,
    completed_ids: frozenset[str] = frozenset(),
) -> EligibleSuccessor:
    """Return exactly one eligible successor from the checked inventory.

    The successor is the first ordered packet whose prerequisites are all
    COMPLETE on the current accepted routing (structural ``COMPLETE`` packet
    states or durable accepted receipts). ``EFFECT`` is deliberately returned
    too: the evidence-backed planner turns it into a typed T3 pause rather
    than silently skipping a route node. The closed packet itself, zero
    eligible successors, an inconsistent manifest, or an ambiguous sketch
    inventory all fail closed.
    """

    if not isinstance(closed_packet_id, str) or plan_lane.PACKET_ID.fullmatch(closed_packet_id) is None:
        raise RouteDriverError("successor_closed_packet_invalid")
    manifest = inventory_manifest(future_document)
    sketches = packet_sketches(future_document)
    ordered = manifest["ordered_packet_ids"]
    if not isinstance(ordered, list) or set(ordered) != set(sketches):
        raise RouteDriverError("route_inventory_sketch_mismatch")
    rows = manifest["profiles"]
    if not isinstance(rows, list) or len(rows) != len(ordered):
        raise RouteDriverError("route_inventory_sketch_mismatch")
    states = _packet_states(next_document)
    for index, packet_id in enumerate(ordered):
        if packet_id == closed_packet_id:
            continue
        if not isinstance(rows[index], list) or len(rows[index]) != 5 or rows[index][0] != packet_id:
            raise RouteDriverError("route_inventory_sketch_mismatch")
        profile = tuple(str(item) for item in rows[index])
        if profile[1] not in PACKET_CLASSES:
            raise RouteDriverError("route_profile_class_invalid")
        sketch = sketches[packet_id]
        if sketch.packet_class != profile[1]:
            raise RouteDriverError("route_profile_class_mismatch")
        incomplete = [
            prerequisite
            for prerequisite in sketch.prerequisites
            if (
                prerequisite != closed_packet_id
                and states.get(prerequisite) != "COMPLETE"
                and prerequisite not in completed_ids
            )
        ]
        if incomplete:
            continue
        return EligibleSuccessor(packet_id=packet_id, sketch=sketch, profile=profile)
    raise RouteDriverError("no_eligible_successor")


def _accepted_completed_ids(status_document: str) -> frozenset[str]:
    """Return durable accepted-receipt identities from CURRENT_STATUS."""

    try:
        return plan_lane.accepted_completed_packet_ids(status_document)
    except plan_lane.PlanLaneError as exc:
        raise RouteDriverError(f"route_status_receipt_index_invalid:{exc.reason}") from exc


def accepted_complete_receipt(status_document: str, packet_id: str) -> str:
    """Return one bounded, merge-backed accepted receipt for a bootstrap.

    A route normally reaches promotion through the Plan Execution Ledger.  The
    one narrow migration case is a packet whose implementation was accepted
    before the route driver itself could create a ledger generation.  That
    packet may be reconciled only from the canonical status receipt, never
    from a branch, a model message, or a caller-supplied packet id.
    """

    if not isinstance(packet_id, str) or plan_lane.PACKET_ID.fullmatch(packet_id) is None:
        raise RouteDriverError("route_bootstrap_packet_invalid")
    _accepted_completed_ids(status_document)
    section = re.search(
        r"^## Accepted Packet Receipts\s*(?P<body>.*?)(?=^## |\Z)",
        status_document,
        re.MULTILINE | re.DOTALL,
    )
    if section is None:
        raise RouteDriverError("route_status_receipt_index_invalid:plan_status_receipt_index_missing")
    receipt_rows = [
        match.group(0).strip()
        for match in plan_lane._ACCEPTED_RECEIPT_ROW.finditer(section.group("body"))
        if match.group("packet") == packet_id
    ]
    if len(receipt_rows) != 1:
        raise RouteDriverError("route_bootstrap_receipt_missing_or_ambiguous")
    receipt_match = re.fullmatch(
        rf"\|\s*`?{re.escape(packet_id)}`?\s*\|\s*`?COMPLETE`?\s*\|\s*(?P<evidence>[^|]+?)\s*\|",
        receipt_rows[0],
    )
    if receipt_match is None:
        raise RouteDriverError("route_bootstrap_receipt_invalid")
    receipt = receipt_match.group("evidence").strip()
    match = plan_lifecycle.canonical_closeout_reference_match(receipt)
    if match is None:
        raise RouteDriverError("route_bootstrap_receipt_not_merge_backed")
    return match.group("canonical")


def _status_with_bound_receipt(
    status_document: str, packet_id: str, predecessor_receipt: str
) -> str | None:
    """Add one transient, packet-bound receipt row for an already-proved route."""

    try:
        accepted_complete_receipt(status_document, packet_id)
        return None
    except RouteDriverError as exc:
        if exc.reason != "route_bootstrap_receipt_missing_or_ambiguous":
            return None
    match = plan_lifecycle.canonical_closeout_reference_match(predecessor_receipt)
    detail = (
        _ROUTE_CLOSEOUT_PACKET_DETAIL.fullmatch(match.group("detail") or "")
        if match is not None
        else None
    )
    if detail is None or detail.group("packet") != packet_id:
        return None
    section = re.search(
        r"^## Accepted Packet Receipts\s*(?P<body>.*?)(?=^## |\Z)",
        status_document,
        re.MULTILINE | re.DOTALL,
    )
    if section is None:
        return None
    row = f"| `{packet_id}` | `COMPLETE` | {match.group('canonical')} |\n"
    end = section.end("body")
    return status_document[:end] + row + status_document[end:]


def _merge_is_ancestor(
    merge_sha: str, accepted_main_sha: str, repo_path: Path | None = None
) -> bool:
    """Prove a predecessor merge is reachable from accepted main."""

    try:
        result = subprocess.run(
            [
                "git",
                "-C",
                str(repo_path or Path(__file__).resolve().parents[2]),
                "merge-base",
                "--is-ancestor",
                merge_sha,
                accepted_main_sha,
            ],
            capture_output=True,
            timeout=20,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return False
    return result.returncode == 0


def route_bound_closeout_reference(packet_id: str, closeout_reference: object) -> str:
    """Bind a ledger-proved closeout to its packet during the status-row gap.

    This annotation is only an in-memory routing bridge.  Once the promotion
    PR writes the accepted status row, that row's packet column is the durable
    identity binding and downstream readers retain only the canonical receipt.
    """

    if not isinstance(packet_id, str) or plan_lane.PACKET_ID.fullmatch(packet_id) is None:
        raise RouteDriverError("promotion_closed_packet_invalid")
    match = plan_lifecycle.canonical_closeout_reference_match(closeout_reference)
    if match is None:
        raise RouteDriverError("promotion_predecessor_receipt_unproved")
    return f"{match.group('canonical')}; route closeout packet `{packet_id}`"


def _require_ancestor_receipt(
    receipt: str, accepted_main_sha: str, repo_path: Path | None = None
) -> str:
    """Require a canonical receipt merge to be reachable from accepted main."""

    match = plan_lifecycle.canonical_closeout_reference_match(receipt)
    if match is None or not _merge_is_ancestor(
        match.group("merge"), accepted_main_sha, repo_path
    ):
        raise RouteDriverError("promotion_predecessor_receipt_unproved")
    return match.group("canonical")


def verified_predecessor_receipt(
    status_document: str,
    closed_packet_id: str,
    predecessor_receipt: str,
    accepted_main_sha: str,
    repo_path: Path | None = None,
) -> str:
    """Prove the just-closed prerequisite before it enters a candidate."""

    if not isinstance(predecessor_receipt, str) or not predecessor_receipt.strip():
        raise RouteDriverError("promotion_predecessor_receipt_missing")
    if plan_lane.SHA40.fullmatch(accepted_main_sha) is None:
        raise RouteDriverError("promotion_accepted_main_invalid")
    receipt = predecessor_receipt.strip()
    match = plan_lifecycle.canonical_closeout_reference_match(receipt)
    if match is None:
        raise RouteDriverError("promotion_predecessor_receipt_unproved")
    canonical = match.group("canonical")
    if isinstance(status_document, str) and status_document:
        try:
            accepted = accepted_complete_receipt(status_document, closed_packet_id)
        except RouteDriverError as exc:
            if exc.reason != "route_bootstrap_receipt_missing_or_ambiguous":
                raise
        else:
            if accepted != canonical:
                raise RouteDriverError("promotion_predecessor_receipt_mismatch")
            return _require_ancestor_receipt(canonical, accepted_main_sha, repo_path)
    detail = match.group("detail")
    packet_detail = (
        _ROUTE_CLOSEOUT_PACKET_DETAIL.fullmatch(detail)
        if isinstance(detail, str)
        else None
    )
    if (
        match.group("merge") != accepted_main_sha
        or packet_detail is None
        or packet_detail.group("packet") != closed_packet_id
    ):
        raise RouteDriverError("promotion_predecessor_receipt_unproved")
    return canonical


def bound_prerequisite_receipts(
    successor: EligibleSuccessor,
    closed_packet_id: str,
    predecessor_receipt: str,
    status_document: str,
    accepted_main_sha: str,
    repo_path: Path | None = None,
) -> tuple[str, ...]:
    """Bind every prerequisite to its exact accepted receipt.

    The just-closed packet has a closeout receipt before this promotion's
    document update can add its durable status row.  Every other prerequisite
    must already have one exact accepted-current-status receipt.  This keeps a
    multi-prerequisite candidate from carrying a plausible but incomplete
    receipt list.
    """

    if not isinstance(closed_packet_id, str) or plan_lane.PACKET_ID.fullmatch(closed_packet_id) is None:
        raise RouteDriverError("promotion_closed_packet_invalid")
    if not isinstance(predecessor_receipt, str) or not predecessor_receipt.strip():
        raise RouteDriverError("promotion_predecessor_receipt_missing")
    if not successor.sketch.prerequisites:
        raise RouteDriverError("promotion_prerequisites_missing")
    if closed_packet_id not in successor.sketch.prerequisites:
        raise RouteDriverError("promotion_closed_packet_not_prerequisite")
    if not isinstance(status_document, str):
        raise RouteDriverError("promotion_prerequisite_receipts_missing_or_invalid")
    receipts: list[str] = []
    for prerequisite in successor.sketch.prerequisites:
        if prerequisite == closed_packet_id:
            receipts.append(
                verified_predecessor_receipt(
                    status_document,
                    closed_packet_id,
                    predecessor_receipt,
                    accepted_main_sha,
                    repo_path,
                )
            )
        else:
            receipts.append(
                _require_ancestor_receipt(
                    accepted_complete_receipt(status_document, prerequisite),
                    accepted_main_sha,
                    repo_path,
                )
            )
    return tuple(receipts)


def bootstrap_reconcile_marked(document: str, packet_id: str) -> bool:
    """Recognize the one explicit bridge from a pre-ledger merge to promotion."""

    matches = _ROUTE_BOOTSTRAP_RECONCILE.findall(document)
    return (
        len(matches) == 1
        and matches[0] == packet_id
        and plan_lane.PACKET_ID.fullmatch(packet_id) is not None
    )


def _refresh_future_document(future_document: str, promoted_id: str) -> str:
    headings = list(plan_lane.PACKET_HEADING.finditer(future_document))
    match = next(
        (heading for heading in headings if heading.group("packet") == promoted_id),
        None,
    )
    if match is None:
        raise RouteDriverError("route_promoted_sketch_missing")
    block_start = match.start()
    block_end = _sketch_end(future_document, match)
    remainder = future_document[:block_start] + future_document[block_end:]
    payload = _derive_inventory_payload(remainder)
    manifest_json = json.dumps(payload, ensure_ascii=False, sort_keys=True)
    marker = INVENTORY_MARKER.search(remainder)
    if marker is None:
        raise RouteDriverError("route_inventory_manifest_missing")
    return (
        remainder[: marker.start()]
        + f"<!-- future-route-inventory:v1\n{manifest_json}\n-->"
        + remainder[marker.end() :]
    )


def _status_readiness_rows(
    closed_packet_id: str,
    successor_id: str,
    predecessor_evidence: str,
    state: str,
    *,
    closed_packet_state: str = "COMPLETE",
) -> tuple[str, str]:
    if closed_packet_state not in {"COMPLETE", "IN_PROGRESS"}:
        raise RouteDriverError("route_closed_packet_state_invalid")
    if closed_packet_state == "IN_PROGRESS":
        return "", ""
    return (
        f"| `{closed_packet_id}` | `COMPLETE` | {predecessor_evidence} |\n",
        "",
    )


def _with_status_rows(status_document: str, closed_row: str, successor_row: str) -> str:
    if successor_row:
        raise RouteDriverError("route_successor_status_row_forbidden")
    if not closed_row:
        return status_document
    section = re.search(
        r"^## Accepted Packet Receipts\s*(?P<body>.*?)(?=^## |\Z)",
        status_document,
        re.MULTILINE | re.DOTALL,
    )
    if section is None:
        raise RouteDriverError("route_status_receipt_index_missing")
    packet = re.search(r"\|\s*`?(?P<packet>[^`|\s]+)`?\s*\|", closed_row)
    if packet is None or plan_lane.PACKET_ID.fullmatch(packet.group("packet")) is None:
        raise RouteDriverError("route_status_receipt_row_invalid")
    existing = [
        match.group(0)
        for match in plan_lane._ACCEPTED_RECEIPT_ROW.finditer(section.group("body"))
        if match.group("packet") == packet.group("packet")
    ]
    if existing:
        if len(existing) == 1 and existing[0].strip() == closed_row.strip():
            return status_document
        raise RouteDriverError("route_status_receipt_conflict")
    return status_document[:section.end()] + closed_row + status_document[section.end():]


def compile_successor(
    future_document: str,
    next_document: str,
    status_document: str,
    closed_packet_id: str,
    predecessor_evidence: str,
    accepted_main_sha: str,
    evidence: CurrentMainEvidence | None = None,
    *,
    closed_packet_state: str = "COMPLETE",
    retained_t3_request: T3Request | None = None,
    retained_t3_receipt: T3Receipt | None = None,
) -> CompiledSuccessor:
    """Compile one evidence-backed non-EFFECT successor into document updates.

    The former compiler derived paths and verification from FUTURE_ROUTE prose.
    This entry point now accepts only an already validated current-main evidence
    object; an absent or incomplete object is a hard ``DECISION_REQUIRED``.
    """

    successor = eligible_successor(
        future_document, next_document, closed_packet_id,
        completed_ids=_accepted_completed_ids(status_document),
    )
    if closed_packet_state == "IN_PROGRESS":
        if (
            retained_t3_request is None
            or retained_t3_receipt is None
            or retained_t3_request.packet_id != closed_packet_id
            or retained_t3_receipt.packet_id != closed_packet_id
            or retained_t3_receipt.accepted_main_sha != accepted_main_sha
            or retained_t3_receipt.candidate_digest != retained_t3_request.candidate_digest
            or successor.profile[1] != "CLOSEOUT"
            or successor.sketch.prerequisites != (closed_packet_id,)
        ):
            raise RouteDriverError("route_effect_closeout_bridge_invalid")
    # Validate the accepted source inventory before selecting a successor, then
    # bind the active candidate to the inventory that will coexist with it.
    # Promotion removes exactly that candidate from FUTURE_ROUTE, so retaining
    # the source hash in the active dispatch capsule would make the accepted
    # plan impossible to reproduce from its own canonical documents.
    inventory_manifest(future_document)
    refreshed_future_document = _refresh_future_document(
        future_document, successor.packet_id
    )
    manifest_sha256 = _json_sha256(
        inventory_manifest(refreshed_future_document)
    )
    if closed_packet_state == "COMPLETE":
        predecessor_match = plan_lifecycle.canonical_closeout_reference_match(
            predecessor_evidence
        )
        if predecessor_match is None or not _merge_is_ancestor(
            predecessor_match.group("merge"), accepted_main_sha
        ):
            raise RouteDriverError("promotion_predecessor_receipt_unproved")
    predecessor_status_document = _status_with_bound_receipt(
        status_document, closed_packet_id, predecessor_evidence
    )
    if predecessor_status_document is not None:
        predecessor_match = plan_lifecycle.canonical_closeout_reference_match(
            predecessor_evidence
        )
        if predecessor_match is None or not _merge_is_ancestor(
            predecessor_match.group("merge"), accepted_main_sha
        ):
            raise RouteDriverError("promotion_predecessor_receipt_unproved")
    planned = RoutePromotionPlanner().plan(
        successor,
        accepted_main_sha,
        predecessor_evidence,
        evidence,
        manifest_sha256,
        closed_packet_id=closed_packet_id,
        status_document=status_document,
        predecessor_status_document=predecessor_status_document,
        retained_t3_request=retained_t3_request,
        retained_t3_receipt=retained_t3_receipt,
    )
    if planned.state not in {"READY_FOR_EXECUTION", "T3_REQUIRED"} or planned.candidate is None:
        raise RouteDriverError(planned.reason)
    candidate = planned.candidate
    contract = candidate.contract
    packet_state = planned.state
    durable_predecessor_evidence = predecessor_evidence.strip()
    if closed_packet_state == "COMPLETE":
        predecessor_match = plan_lifecycle.canonical_closeout_reference_match(
            durable_predecessor_evidence
        )
        if predecessor_match is None:
            raise RouteDriverError("promotion_predecessor_receipt_unproved")
        durable_predecessor_evidence = predecessor_match.group("canonical")
    capsule = dict(candidate.capsule)
    capsule["packet_state"] = packet_state
    if planned.t3_request is not None:
        capsule["t3_request_digest"] = planned.t3_request.candidate_digest
    capsule_json = json.dumps(capsule, ensure_ascii=False, sort_keys=True)
    t3_marker = ""
    if planned.t3_request is not None:
        t3_marker = _t3_request_marker(planned.t3_request)
    predecessor_state = (
        "COMPLETE" if closed_packet_state == "COMPLETE"
        else "IN_PROGRESS pending this packet's independent outcome validation"
    )
    packet_block = (
        f"## Packet {successor.packet_id}\n\n"
        f"**State:** `{packet_state}`\n\n"
        f"**Prerequisite:** {', '.join(successor.sketch.prerequisites) or 'none'} — "
        f"{predecessor_state} on accepted main `{accepted_main_sha}` ({durable_predecessor_evidence}).\n\n"
        f"**Class:** `{successor.sketch.packet_class}`\n\n"
        f"**Outcome:** {successor.sketch.outcome}\n\n"
        f"**Allowed delta:** {', '.join(contract['allowed_paths'])}.\n\n"
        f"**Exit:** {successor.sketch.exit_statement}\n\n"
        f"**Stop:** {successor.sketch.stop}\n\n"
        "### Twelve-field contract\n\n"
        f"1. **Outcome and non-goals.** {successor.sketch.outcome}\n"
        f"2. **Prerequisites and evidence.** Accepted main `{accepted_main_sha}`; checked route manifest SHA `{candidate.manifest_sha256}`; predecessor receipt {durable_predecessor_evidence}; current-main evidence SHA `{candidate.evidence_sha256}`.\n"
        f"3. **Owners and paths.** Owners: {', '.join(contract['owner_paths'])}; callers: {', '.join(contract['caller_paths'])}; tests: {', '.join(contract['test_paths'])}.\n"
        f"4. **Frozen invariants.** Packet identity, route manifest SHA `{candidate.manifest_sha256}`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.\n"
        "5. **Only semantic delta.** Execute only the independently reviewed candidate contract.\n"
        "6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.\n"
        f"7. **Ordered implementation slices.** {'; '.join(contract['ordered_slices'])}\n"
        f"8. **Failure, recovery, and stop taxonomy.** Cleanup: {contract['cleanup']}; retention: {contract['retention']}; decisions: {'; '.join(contract['decisions'])}.\n"
        f"9. **Verification.** {'; '.join(contract['verification'])}\n"
        f"10. **Compatibility, rollback, and retention.** {contract['rollback']}\n"
        f"11. **Exit artifact.** Evidence destinations: {', '.join(contract['evidence_destinations'])}.\n"
        "12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.\n\n"
        "### 11. Bounded Autonomous Worker Dispatch Capsule\n\n"
        f"<!-- weak-agent-dispatch:v1\n{capsule_json}\n-->{t3_marker}"
    )
    next_document = compact_next_window(
        next_document,
        closed_packet_id=closed_packet_id,
        predecessor_receipt=durable_predecessor_evidence,
        active_packet_block=packet_block,
        active_state=packet_state,
        closed_packet_state=closed_packet_state,
        retained_marker=(
            _t3_request_marker(retained_t3_request)
            if retained_t3_request is not None
            else ""
        ),
        active_risk_class=successor.profile[3],
    )
    future_document = refreshed_future_document
    closed_row, successor_row = _status_readiness_rows(
        closed_packet_id, successor.packet_id, durable_predecessor_evidence, packet_state,
        closed_packet_state=closed_packet_state,
    )
    status_document = _with_status_rows(
        status_document, closed_row, successor_row
    )
    return CompiledSuccessor(
        packet_id=successor.packet_id,
        capsule=candidate.capsule,
        spec_digest=candidate.spec_digest,
        manifest_sha256=manifest_sha256,
        branch=f"agent/packet-{successor.packet_id.lower()}",
        next_document=next_document,
        future_document=future_document,
        status_document=status_document,
        packet_state=packet_state,
        t3_request=planned.t3_request,
    )


@dataclass(frozen=True)
class CurrentMainEvidence:
    """The bounded, current-main proof required before successor promotion.

    ``FUTURE_ROUTE`` may describe an intended seam, but it never supplies
    these fields.  They are supplied by the bounded promotion planner after
    inspecting the accepted checkout's MODULE_MAP, source callers, and tests.
    """

    packet_id: str
    accepted_main_sha: str
    status_document_sha256: str
    owner_paths: tuple[str, ...]
    caller_paths: tuple[str, ...]
    test_paths: tuple[str, ...]
    allowed_paths: tuple[str, ...]
    read_paths: tuple[str, ...]
    ordered_slices: tuple[str, ...]
    verification: tuple[str, ...]
    rollback: str
    cleanup: str
    retention: str
    evidence_destinations: tuple[str, ...]
    decisions: tuple[str, ...]


@dataclass(frozen=True)
class PromotionCandidate:
    """One independently reviewable successor contract, never an execution grant."""

    packet_id: str
    accepted_main_sha: str
    predecessor_receipt: str
    evidence_sha256: str
    manifest_sha256: str
    spec_digest: str
    capsule: dict[str, Any]
    contract: dict[str, object]


@dataclass(frozen=True)
class T3Request:
    """The minimum typed source-authoritative request for a prepared EFFECT node."""

    packet_id: str
    accepted_main_sha: str
    candidate_digest: str
    action_digest: str
    scope_digest: str
    authority_owner_digest: str
    requested_action: str


def t3_decision_digest(
    request: T3Request,
    decision_source: str,
    decision_evidence_digest: str,
    disposition: str,
) -> str:
    """Hash the redacted decision evidence into one exact finite disposition.

    The evidence digest is the retention-safe commitment to the source's
    conclusion; this binding prevents a transport from reusing it for another
    packet, scope, source, or disposition. It is not an effect grant.
    """

    return _json_sha256({
        "schema_version": "route_t3_decision.v1",
        "packet_id": request.packet_id,
        "accepted_main_sha": request.accepted_main_sha,
        "candidate_digest": request.candidate_digest,
        "action_digest": request.action_digest,
        "scope_digest": request.scope_digest,
        "authority_owner_digest": request.authority_owner_digest,
        "decision_source": decision_source,
        "decision_evidence_digest": decision_evidence_digest,
        "disposition": disposition,
    })


def _t3_request_marker(request: T3Request) -> str:
    """Serialize a typed request for an active or retained route boundary."""

    return (
        "\n<!-- route-t3-request:v1\n"
        + json.dumps(
            {
                "schema_version": "route_t3_request.v1",
                "packet_id": request.packet_id,
                "accepted_main_sha": request.accepted_main_sha,
                "candidate_digest": request.candidate_digest,
                "action_digest": request.action_digest,
                "scope_digest": request.scope_digest,
                "authority_owner_digest": request.authority_owner_digest,
                "requested_action": request.requested_action,
            },
            ensure_ascii=False,
            sort_keys=True,
        )
        + "\n-->\n"
    )


@dataclass(frozen=True)
class T3Receipt:
    """A hostile-input-validated finite source-authoritative decision receipt.

    The authenticated transport binds an allowlisted decision source to the
    finite request; it does not execute the effect.  The existing product
    effect owner remains outside this repository-maintenance controller.  The
    redacted outcome digest is only a handoff binding: the routed CLOSEOUT
    packet must independently validate the owner-held evidence before it can
    make a stronger claim.
    """

    packet_id: str
    accepted_main_sha: str
    candidate_digest: str
    action_digest: str
    scope_digest: str
    authority_receipt_digest: str
    outcome_receipt_digest: str
    authority_owner_digest: str
    operator: str
    decision_source: str
    decision_evidence_digest: str
    decision_digest: str
    issued_at: str
    expires_at: str
    disposition: str


def t3_closeout_reference(receipt: T3Receipt) -> str:
    """Return the only predecessor reference permitted after an EFFECT."""

    return (
        f"T3 operator authority `{receipt.authority_receipt_digest}`; redacted effect outcome "
        f"`{receipt.outcome_receipt_digest}`"
    )


def _t3_receipt_wire(receipt: T3Receipt) -> dict[str, object]:
    """Serialize a typed receipt for the same hostile-input validation path."""

    return {
        "schema_version": "route_t3_receipt.v1",
        "packet_id": receipt.packet_id,
        "accepted_main_sha": receipt.accepted_main_sha,
        "candidate_digest": receipt.candidate_digest,
        "action_digest": receipt.action_digest,
        "scope_digest": receipt.scope_digest,
        "authority_receipt_digest": receipt.authority_receipt_digest,
        "outcome_receipt_digest": receipt.outcome_receipt_digest,
        "authority_owner_digest": receipt.authority_owner_digest,
        "operator": receipt.operator,
        "decision_source": receipt.decision_source,
        "decision_evidence_digest": receipt.decision_evidence_digest,
        "decision_digest": receipt.decision_digest,
        "issued_at": receipt.issued_at,
        "expires_at": receipt.expires_at,
        "disposition": receipt.disposition,
    }


def validate_t3_receipt(
    raw: object,
    request: T3Request,
    *,
    now: datetime | None = None,
) -> tuple[T3Receipt | None, str]:
    """Validate one finite receipt without issuing or consuming authority.

    The route never writes these receipts.  An existing authenticated
    transport must put a typed, hash-bound record on the authoritative ledger
    for one allowlisted decision source; this function only rejects malformed,
    stale, or conflicting values.  A valid GO permits the route to compile the
    next provider-free CLOSEOUT packet; it never causes this controller to
    execute an EFFECT.
    """

    required = {
        "schema_version", "packet_id", "accepted_main_sha", "candidate_digest",
        "action_digest", "scope_digest", "authority_receipt_digest", "outcome_receipt_digest",
        "authority_owner_digest", "operator", "decision_source", "decision_evidence_digest",
        "decision_digest",
        "issued_at", "expires_at", "disposition",
    }
    if not isinstance(raw, dict) or set(raw) != required:
        return None, "t3_receipt_invalid"
    if (
        raw.get("schema_version") != "route_t3_receipt.v1"
        or raw.get("packet_id") != request.packet_id
        or raw.get("accepted_main_sha") != request.accepted_main_sha
        or raw.get("candidate_digest") != request.candidate_digest
        or raw.get("action_digest") != request.action_digest
        or raw.get("scope_digest") != request.scope_digest
        or raw.get("authority_owner_digest") != request.authority_owner_digest
    ):
        return None, "t3_receipt_binding_mismatch"
    digest_fields = (
        "action_digest", "scope_digest", "authority_receipt_digest",
        "outcome_receipt_digest", "authority_owner_digest",
    )
    if any(not isinstance(raw.get(field), str) or plan_lane.SHA256.fullmatch(raw[field]) is None for field in digest_fields):
        return None, "t3_receipt_digest_invalid"
    operator = raw.get("operator")
    if (
        not isinstance(operator, str)
        or _SAFE_PROPOSAL_TOKEN.fullmatch(operator) is None
    ):
        return None, "t3_receipt_operator_invalid"
    decision_source = raw.get("decision_source")
    decision_evidence_digest = raw.get("decision_evidence_digest")
    decision_digest = raw.get("decision_digest")
    if (
        decision_source not in T3_DECISION_SOURCES
        or not isinstance(decision_evidence_digest, str)
        or plan_lane.SHA256.fullmatch(decision_evidence_digest) is None
        or not isinstance(decision_digest, str)
        or plan_lane.SHA256.fullmatch(decision_digest) is None
    ):
        return None, "t3_receipt_decision_source_invalid"
    disposition = raw.get("disposition")
    if disposition not in {"GO", "NO_GO", "DEFER"}:
        return None, "t3_receipt_disposition_invalid"
    if decision_digest != t3_decision_digest(
        request, decision_source, decision_evidence_digest, disposition
    ):
        return None, "t3_receipt_decision_binding_invalid"
    try:
        issued = datetime.fromisoformat(str(raw["issued_at"]).replace("Z", "+00:00"))
        expires = datetime.fromisoformat(str(raw["expires_at"]).replace("Z", "+00:00"))
    except ValueError:
        return None, "t3_receipt_time_invalid"
    if issued.tzinfo is None or expires.tzinfo is None or expires <= issued:
        return None, "t3_receipt_time_invalid"
    observed = now or datetime.now(timezone.utc)
    if observed.tzinfo is None:
        observed = observed.replace(tzinfo=timezone.utc)
    if issued > observed:
        return None, "t3_receipt_issued_in_future"
    if expires - issued > MAX_T3_RECEIPT_WINDOW:
        return None, "t3_receipt_window_exceeded"
    if observed > expires:
        return None, "t3_receipt_expired"
    receipt = T3Receipt(
        packet_id=request.packet_id,
        accepted_main_sha=request.accepted_main_sha,
        candidate_digest=request.candidate_digest,
        action_digest=raw["action_digest"],
        scope_digest=raw["scope_digest"],
        authority_receipt_digest=raw["authority_receipt_digest"],
        outcome_receipt_digest=raw["outcome_receipt_digest"],
        authority_owner_digest=raw["authority_owner_digest"],
        operator=operator,
        decision_source=decision_source,
        decision_evidence_digest=decision_evidence_digest,
        decision_digest=decision_digest,
        issued_at=raw["issued_at"],
        expires_at=raw["expires_at"],
        disposition=disposition,
    )
    return receipt, "t3_receipt_valid"


def current_t3_request(document: str, accepted_main_sha: str) -> T3Request | None:
    """Return the one current typed EFFECT pause, never an execution grant."""

    if not plan_lane.SHA40.fullmatch(accepted_main_sha):
        raise RouteDriverError("route_t3_main_invalid")
    routed = list(plan_lane.ACTIVE_ROUTING.finditer(document))
    if not routed:
        return None
    current = routed[0].group("packet")
    headings = list(plan_lane.PACKET_HEADING.finditer(document))
    index = next((number for number, heading in enumerate(headings) if heading.group("packet") == current), None)
    if index is None:
        return None
    start = headings[index].start()
    end = headings[index + 1].start() if index + 1 < len(headings) else len(document)
    block = document[start:end]
    if not re.search(r"^\*\*State:\*\* `T3_REQUIRED`\s*$", block, re.MULTILINE):
        return None
    markers = list(_T3_REQUEST_MARKER.finditer(block))
    if len(markers) != 1:
        raise RouteDriverError("route_t3_request_missing_or_ambiguous")
    try:
        payload = json.loads(markers[0].group(1))
    except json.JSONDecodeError as exc:
        raise RouteDriverError("route_t3_request_invalid") from exc
    required = {
        "schema_version", "packet_id", "accepted_main_sha", "candidate_digest", "action_digest",
        "scope_digest", "authority_owner_digest", "requested_action"
    }
    if (
        not isinstance(payload, dict)
        or set(payload) != required
        or payload.get("schema_version") != "route_t3_request.v1"
        or payload.get("packet_id") != current
        or payload.get("accepted_main_sha") != accepted_main_sha
        or not isinstance(payload.get("candidate_digest"), str)
        or plan_lane.SHA256.fullmatch(payload["candidate_digest"]) is None
        or not isinstance(payload.get("action_digest"), str)
        or plan_lane.SHA256.fullmatch(payload["action_digest"]) is None
        or not isinstance(payload.get("scope_digest"), str)
        or plan_lane.SHA256.fullmatch(payload["scope_digest"]) is None
        or not isinstance(payload.get("authority_owner_digest"), str)
        or plan_lane.SHA256.fullmatch(payload["authority_owner_digest"]) is None
        or not isinstance(payload.get("requested_action"), str)
        or not payload["requested_action"].strip()
        or len(payload["requested_action"]) > MAX_SKETCH_FIELD_CHARS
    ):
        raise RouteDriverError("route_t3_request_invalid")
    return T3Request(
        packet_id=current,
        accepted_main_sha=accepted_main_sha,
        candidate_digest=payload["candidate_digest"],
        action_digest=payload["action_digest"],
        scope_digest=payload["scope_digest"],
        authority_owner_digest=payload["authority_owner_digest"],
        requested_action=payload["requested_action"].strip(),
    )


def retained_t3_request(document: str) -> T3Request | None:
    """Read the one retained EFFECT request that gates a direct CLOSEOUT."""

    retained = list(re.finditer(
        rf"^#{{2,3}} Retained .*?\((?P<packet>{plan_lane.PACKET_TOKEN})\)\s*$",
        document,
        re.MULTILINE,
    ))
    if not retained:
        return None
    if len(retained) != 1:
        raise RouteDriverError("route_retained_t3_request_ambiguous")
    heading = retained[0]
    next_heading = re.search(r"^#{2,3} ", document[heading.end():], re.MULTILINE)
    end = heading.end() + next_heading.start() if next_heading is not None else len(document)
    block = document[heading.start():end]
    markers = list(_T3_REQUEST_MARKER.finditer(block))
    if len(markers) != 1:
        raise RouteDriverError("route_retained_t3_request_missing_or_ambiguous")
    try:
        payload = json.loads(markers[0].group(1))
    except json.JSONDecodeError as exc:
        raise RouteDriverError("route_retained_t3_request_invalid") from exc
    accepted_main_sha = payload.get("accepted_main_sha") if isinstance(payload, dict) else None
    if not isinstance(accepted_main_sha, str) or plan_lane.SHA40.fullmatch(accepted_main_sha) is None:
        raise RouteDriverError("route_retained_t3_request_invalid")
    synthetic = (
        f"## Active Routing\n\n1. `{heading.group('packet')}` — `T3_REQUIRED`\n\n"
        f"## Packet {heading.group('packet')}\n\n**State:** `T3_REQUIRED`\n\n"
        f"<!-- route-t3-request:v1\n{markers[0].group(1)}\n-->\n"
    )
    return current_t3_request(synthetic, accepted_main_sha)


def direct_effect_closeout_request(
    document: str,
    closeout_packet_id: str,
    source_main_sha: str,
) -> T3Request | None:
    """Recover the immutable EFFECT binding for its direct CLOSEOUT packet.

    The closeout's plan claim binds ``source_main_sha`` before its PR can
    change the current window.  Re-reading that source prevents a completed
    closeout from deleting its retained marker and thereby bypassing the
    independent existing-owner outcome proof required before later promotion.
    """

    if (
        not isinstance(closeout_packet_id, str)
        or plan_lane.PACKET_ID.fullmatch(closeout_packet_id) is None
        or not isinstance(source_main_sha, str)
        or plan_lane.SHA40.fullmatch(source_main_sha) is None
    ):
        raise RouteDriverError("route_effect_closeout_source_invalid")
    request = retained_t3_request(document)
    block = re.search(
        rf"^## Packet {re.escape(closeout_packet_id)}\s*(?P<body>.*?)(?=^## |\Z)",
        document,
        re.MULTILINE | re.DOTALL,
    )
    if block is None or not re.search(
        r"^\*\*Class:\*\*\s*`CLOSEOUT`\s*$", block.group("body"), re.MULTILINE
    ):
        return None
    active = re.search(
        r"^## Active Routing\s*(?P<body>.*?)(?=^## |\Z)",
        document,
        re.MULTILINE | re.DOTALL,
    )
    if active is None or len(re.findall(
        rf"^1\. `{re.escape(closeout_packet_id)}` — `READY_FOR_EXECUTION`\s*$",
        active.group("body"),
        re.MULTILINE,
    )) != 1:
        raise RouteDriverError("route_effect_closeout_source_invalid")
    prerequisite = re.search(
        r"^\*\*Prerequisite:\*\*\s*(?P<value>.+)$",
        block.group("body"),
        re.MULTILINE,
    )
    if request is None:
        # An ordinary CLOSEOUT may have no retained T3 marker.  A direct
        # EFFECT closeout is distinguishable by its still-IN_PROGRESS
        # prerequisite; a missing marker there is source-window tampering and
        # must not downgrade to ordinary routing.
        if prerequisite is not None and re.search(r"\bIN_PROGRESS\b", prerequisite.group("value")):
            raise RouteDriverError("route_effect_closeout_source_invalid")
        return None
    if request.accepted_main_sha != source_main_sha:
        raise RouteDriverError("route_effect_closeout_source_invalid")
    packet_ids = (
        tuple(dict.fromkeys(re.findall(plan_lane.PACKET_TOKEN, prerequisite.group("value"))))
        if prerequisite is not None
        else ()
    )
    if packet_ids != (request.packet_id,):
        raise RouteDriverError("route_effect_closeout_source_invalid")
    return request


def validate_recorded_t3_receipt(
    raw: object, request: T3Request
) -> tuple[T3Receipt | None, str]:
    """Validate an already-recorded receipt without requiring it to stay fresh.

    Freshness is enforced before the bridge is opened. Once that accepted
    bridge exists, later CLOSEOUT verification proves the immutable binding at
    issuance time rather than treating ordinary PR/CI latency as a new T3
    expiry.
    """

    issued_at = raw.get("issued_at") if isinstance(raw, dict) else None
    expires_at = raw.get("expires_at") if isinstance(raw, dict) else None
    if not isinstance(issued_at, str) or not isinstance(expires_at, str):
        return None, "t3_receipt_time_invalid"
    try:
        issued = datetime.fromisoformat(issued_at.replace("Z", "+00:00"))
        expires = datetime.fromisoformat(expires_at.replace("Z", "+00:00"))
    except ValueError:
        return None, "t3_receipt_time_invalid"
    if issued.tzinfo is None or expires.tzinfo is None:
        return None, "t3_receipt_time_invalid"
    observed = datetime.now(timezone.utc)
    if issued > observed:
        return None, "t3_receipt_issued_in_future"
    return validate_t3_receipt(raw, request, now=min(observed, expires))


_OWNER_ACTOR = re.compile(r"^[A-Za-z0-9](?:[A-Za-z0-9-]{0,37})$")


def owner_outcome_receipt_digest(
    packet_id: str,
    accepted_main_sha: str,
    candidate_digest: str,
    outcome_receipt_digest: str,
    owner_actor: str,
    owner_evidence_digest: str,
) -> str:
    """Derive the stable binding for an authenticated existing-owner receipt."""

    payload = {
        "schema_version": "route_t3_owner_outcome.v1",
        "packet_id": packet_id,
        "accepted_main_sha": accepted_main_sha,
        "candidate_digest": candidate_digest,
        "outcome_receipt_digest": outcome_receipt_digest,
        "owner_actor": owner_actor,
        "owner_evidence_digest": owner_evidence_digest,
    }
    return hashlib.sha256(
        json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()


def owner_outcome_receipt_proved(
    status_document: str,
    request: T3Request,
    receipt: T3Receipt,
    owner_receipt: object,
) -> bool:
    """Require an independent, authenticated owner receipt, not status prose.

    The accepted status row is only the closeout index.  The authority is the
    separate trusted ledger state written by the existing product owner
    transport, bound to this exact request and outcome digest.
    """

    if not isinstance(owner_receipt, dict):
        return False
    if (
        owner_receipt.get("action") != "route-t3-owner-outcome"
        or owner_receipt.get("status") != "validated"
    ):
        return False
    details = owner_receipt.get("details")
    if not isinstance(details, dict):
        return False
    owner_actor = details.get("owner_actor")
    owner_evidence_digest = details.get("owner_evidence_digest")
    if (
        details.get("schema_version") != "route_t3_owner_outcome.v1"
        or details.get("packet_id") != request.packet_id
        or details.get("accepted_main_sha") != request.accepted_main_sha
        or details.get("candidate_digest") != request.candidate_digest
        or details.get("outcome_receipt_digest") != receipt.outcome_receipt_digest
        or not isinstance(owner_actor, str)
        or _OWNER_ACTOR.fullmatch(owner_actor) is None
        or owner_actor.endswith("[bot]")
        or not isinstance(owner_evidence_digest, str)
        or plan_lane.SHA256.fullmatch(owner_evidence_digest) is None
    ):
        return False
    expected_digest = owner_outcome_receipt_digest(
        request.packet_id,
        request.accepted_main_sha,
        request.candidate_digest,
        receipt.outcome_receipt_digest,
        owner_actor,
        owner_evidence_digest,
    )
    return details.get("owner_receipt_digest") == expected_digest


@dataclass(frozen=True)
class PromotionPlanResult:
    state: str
    reason: str
    candidate: PromotionCandidate | None = None
    t3_request: T3Request | None = None
    evidence: CurrentMainEvidence | None = None


def _evidence_payload(evidence: CurrentMainEvidence) -> dict[str, object]:
    return {
        "packet_id": evidence.packet_id,
        "accepted_main_sha": evidence.accepted_main_sha,
        "status_document_sha256": evidence.status_document_sha256,
        "owner_paths": list(evidence.owner_paths),
        "caller_paths": list(evidence.caller_paths),
        "test_paths": list(evidence.test_paths),
        "allowed_paths": list(evidence.allowed_paths),
        "read_paths": list(evidence.read_paths),
        "ordered_slices": list(evidence.ordered_slices),
        "verification": list(evidence.verification),
        "rollback": evidence.rollback,
        "cleanup": evidence.cleanup,
        "retention": evidence.retention,
        "evidence_destinations": list(evidence.evidence_destinations),
        "decisions": list(evidence.decisions),
    }


def _evidence_problem(
    successor: EligibleSuccessor,
    accepted_main_sha: str,
    evidence: CurrentMainEvidence | None,
) -> str | None:
    if evidence is None:
        return "promotion_current_main_evidence_missing"
    if evidence.packet_id != successor.packet_id:
        return "promotion_evidence_packet_mismatch"
    if evidence.accepted_main_sha != accepted_main_sha:
        return "promotion_evidence_main_mismatch"
    if not plan_lane.SHA40.fullmatch(accepted_main_sha):
        return "promotion_accepted_main_invalid"
    if plan_lane.SHA256.fullmatch(evidence.status_document_sha256) is None:
        return "promotion_status_document_binding_invalid"
    fields = (
        evidence.owner_paths,
        evidence.caller_paths,
        evidence.test_paths,
        evidence.allowed_paths,
        evidence.read_paths,
        evidence.ordered_slices,
        evidence.verification,
        evidence.evidence_destinations,
        evidence.decisions,
    )
    if any(not value for value in fields):
        return "promotion_refresh_fact_missing"
    scalar_fields = (evidence.rollback, evidence.cleanup, evidence.retention)
    if any(not isinstance(value, str) or not value.strip() for value in scalar_fields):
        return "promotion_refresh_fact_missing"
    try:
        allowed = artifact_contract.validate_allowed_paths(list(evidence.allowed_paths))
    except artifact_contract.ArtifactContractError:
        return "promotion_allowed_paths_invalid"
    if tuple(allowed) != evidence.allowed_paths:
        return "promotion_allowed_paths_noncanonical"
    try:
        read_paths = tuple(artifact_contract.validate_allowed_paths(list(evidence.read_paths)))
    except artifact_contract.ArtifactContractError:
        return "promotion_read_paths_invalid"
    if read_paths != evidence.read_paths:
        return "promotion_read_paths_noncanonical"
    allowed_set = set(evidence.allowed_paths)
    read_set = set(evidence.read_paths)
    if not allowed_set.issubset(read_set):
        return "promotion_allowed_paths_outside_read_paths"
    closure = set(evidence.owner_paths + evidence.caller_paths + evidence.test_paths)
    if not closure.issubset(read_set):
        return "promotion_owner_caller_test_outside_read_paths"
    if len(allowed_set) != len(evidence.allowed_paths):
        return "promotion_allowed_paths_duplicated"
    if len(read_set) != len(evidence.read_paths):
        return "promotion_read_paths_duplicated"
    return None


def promotion_planner_prompt(
    successor: EligibleSuccessor,
    accepted_main_sha: str,
    predecessor_receipt: str,
) -> str:
    """Build the bounded, read-only request made to a weak promotion worker.

    The prompt contains stable inventory facts only.  In particular, it does
    not include FUTURE_ROUTE's ``Allowed delta`` prose: the worker must inspect
    the exact accepted tree and return a proposal which the deterministic
    verifier re-proves before it can become a candidate.
    """

    if not plan_lane.SHA40.fullmatch(accepted_main_sha):
        raise RouteDriverError("promotion_accepted_main_invalid")
    if not predecessor_receipt.strip():
        raise RouteDriverError("promotion_predecessor_receipt_missing")
    return f"""You are a read-only bounded promotion planner. Do not modify files,
run GitHub commands, create a PR, call an external target, or issue authority.

Stable routing facts (not edit authority):
- packet_id: {successor.packet_id}
- class: {successor.sketch.packet_class}
- prerequisites: {json.dumps(list(successor.sketch.prerequisites))}
- accepted_main_sha: {accepted_main_sha}
- predecessor receipt: {predecessor_receipt.strip()[:200]}

Inspect only the exact accepted tree with commands such as
`git show {accepted_main_sha}:docs/MODULE_MAP.md` and
`git show {accepted_main_sha}:<path>`.  Do not use FUTURE_ROUTE's Allowed delta
prose as path authority.  Resolve exact current owners, direct callers,
tests, allowed-path closure, ordered slices, precise allowlisted verification,
rollback, cleanup, retention, evidence destinations, and the schema,
evaluator, authority, and recovery decisions.

Treat `allowed_paths` as the closed, machine-validated edit list.
Each `allowed_paths` entry must be a literal repository-relative path:
- Return a non-empty list of at most {_MAX_PROMOTION_LIST_ITEMS} entries, with no duplicates and no
  glob characters.
- Every entry must name an existing regular, non-symlink accepted-tree file.
  Prove each with `git ls-tree {accepted_main_sha} -- <path>` before
  returning it.
- Never use placeholders such as `...` or `exact/file`, a directory, an
  absolute path, or a path containing `..`.
- The verifier hard-rejects workflow and hidden paths: never include any
  path under `.github/workflows/`, `.github/actions/`, `.git/`, or `.codex/`.
- Every mutable path (ordered slice, operation, destination, or decision) must
  appear in `allowed_paths`; do not use the list to smuggle a static route hint.

Treat `read_paths` as the closed, machine-validated read-only evidence scope.
It must contain every `allowed_paths` entry plus every owner, caller, and test
path. The verifier reads those paths from the exact accepted tree, but the
worker may not edit them unless they are also in `allowed_paths`.

Each `caller_evidence` row must be independently machine-verifiable:
- `owner_path` must be in both `owner_evidence` and `read_paths`.
- `caller_path` must be in `read_paths`.
- For `.py` paths, `symbol` must match a Python `def` or `class` declaration in `owner_path`
  and a verifier-recognized `symbol(` reference in `caller_path`.
- For `.rs` paths, `symbol` must match a Rust `fn`, `struct`, `enum`, `trait`,
  `type`, `mod`, `const`, or `static` declaration in `owner_path` and a
  verifier-recognized function call, lower-case associated constructor/call
  path, or lower-case `symbol!` macro reference in
  `caller_path`.
- Prove Python symbols against the exact accepted tree with
  `git grep -nE '^[[:space:]]*(def|class)[[:space:]]+<symbol>' {accepted_main_sha} -- <owner_path>`
  and `git grep -nF '<symbol>(' {accepted_main_sha} -- <caller_path>`.
  For Rust, use equivalent language-appropriate `git grep -nE` checks for the
  declaration and `git grep -nE '<symbol>[[:space:]]*(\\(|::|!)'` for the
  reference. Rust comments and literals are not evidence. Return the row only when both commands prove it.
  Declaration and consumption must both be
  provable; row whose owner/caller/symbol cannot be proven is rejected.
Do not include a speculative caller row; return the bounded decision object
instead when no such exact caller fact can be proved.

Return exactly one compact JSON object and no Markdown.  If any fact cannot
be proved, return exactly:
{{"schema_version":"{_ROUTE_EVIDENCE_SCHEMA}","state":"DECISION_REQUIRED","reason":"short_safe_reason"}}

Otherwise return this schema (no extra keys):
{{
 "schema_version":"{_ROUTE_EVIDENCE_SCHEMA}", "packet_id":"...",
 "accepted_main_sha":"...", "owner_evidence":[{{"path":"...","module_map_token":"..."}}],
 "caller_evidence":[{{"owner_path":"...","caller_path":"...","symbol":"..."}}],
 "test_evidence":[{{"target_path":"...","test_path":"...","symbol":"..."}}],
 "allowed_paths":["docs/MODULE_MAP.md"], "read_paths":["docs/MODULE_MAP.md"],
 "ordered_slices":[{{"paths":["docs/MODULE_MAP.md"],"description":"precise bounded slice"}}],
 "verification":["one repository allowlisted command"],
 "operations":{{
   "rollback":{{"source_path":"...","needle":"...","description":"..."}},
   "cleanup":{{"source_path":"...","needle":"...","description":"..."}},
   "retention":{{"source_path":"...","needle":"...","description":"..."}}
 }},
 "evidence_destinations":[{{"source_path":"...","needle":"...","description":"..."}}],
 "decisions":{{
   "schema":{{"state":"UNCHANGED","source_path":"...","needle":"..."}},
   "evaluator":{{"state":"UNCHANGED","source_path":"...","needle":"..."}},
   "authority":{{"state":"UNCHANGED","source_path":"...","needle":"..."}},
   "recovery":{{"state":"UNCHANGED","source_path":"...","needle":"..."}}
 }}
}}"""


class CurrentMainEvidenceVerifier:
    """Turn a bounded autonomous worker proposal into verified current-main evidence.

    This is deliberately a small deep module: model text can suggest a finite
    closure, but this module is the sole place that parses it, reads the exact
    accepted tree, proves each claimed edge, and either emits a complete
    ``CurrentMainEvidence`` object or a typed decision.  It never reads a
    future-route path hint and never writes durable state.
    """

    def __init__(self, repo_path: Path, accepted_main_sha: str) -> None:
        if not plan_lane.SHA40.fullmatch(accepted_main_sha):
            raise RouteDriverError("promotion_accepted_main_invalid")
        self.repo_path = Path(repo_path)
        self.accepted_main_sha = accepted_main_sha
        self._cache: dict[str, str] = {}

    def _source(self, path: object) -> str:
        if not isinstance(path, str) or not path or path.startswith("/") or ".." in Path(path).parts:
            raise RouteDriverError("promotion_evidence_path_invalid")
        if path in self._cache:
            return self._cache[path]
        try:
            mode = subprocess.run(
                ["git", "-C", str(self.repo_path), "ls-tree", self.accepted_main_sha, "--", path],
                capture_output=True,
                text=True,
                timeout=20,
                check=False,
            )
            if mode.returncode != 0 or not mode.stdout.startswith("100") or "\t" not in mode.stdout:
                raise RouteDriverError("promotion_evidence_path_missing")
            if mode.stdout.startswith("120000"):
                raise RouteDriverError("promotion_evidence_path_symlink")
            result = subprocess.run(
                ["git", "-C", str(self.repo_path), "show", f"{self.accepted_main_sha}:{path}"],
                capture_output=True,
                text=True,
                timeout=20,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise RouteDriverError("promotion_evidence_source_unavailable") from exc
        if result.returncode != 0 or len(result.stdout.encode("utf-8")) > MAX_CURRENT_MAIN_SOURCE_BYTES:
            raise RouteDriverError("promotion_evidence_source_unavailable")
        self._cache[path] = result.stdout
        return result.stdout

    def _status_with_ancestor_receipt(
        self, status_document: str, packet_id: str, predecessor_receipt: str
    ) -> str | None:
        """Return a transient prerequisite index when an accepted row lagged main.

        The receipt is still required to be canonical and its merge must be an
        ancestor of accepted main.  The synthetic row is never persisted and is
        used only by the existing prerequisite validator.
        """

        try:
            accepted_complete_receipt(status_document, packet_id)
            return None
        except RouteDriverError as exc:
            if exc.reason != "route_bootstrap_receipt_missing_or_ambiguous":
                return None
        match = plan_lifecycle.canonical_closeout_reference_match(predecessor_receipt)
        if match is None:
            return None
        detail = _ROUTE_CLOSEOUT_PACKET_DETAIL.fullmatch(match.group("detail") or "")
        if detail is None or detail.group("packet") != packet_id:
            return None
        if not _merge_is_ancestor(
            match.group("merge"), self.accepted_main_sha, self.repo_path
        ):
            return None
        return _status_with_bound_receipt(status_document, packet_id, predecessor_receipt)

    @staticmethod
    def _text(value: object, reason: str, *, maximum: int = 512) -> str:
        if not isinstance(value, str) or not value.strip() or len(value) > maximum:
            raise RouteDriverError(reason)
        return value.strip()

    @classmethod
    def _token(cls, value: object, reason: str) -> str:
        token = cls._text(value, reason, maximum=160)
        if _SAFE_PROPOSAL_TOKEN.fullmatch(token) is None:
            raise RouteDriverError(reason)
        return token

    @classmethod
    def _symbol(cls, value: object, reason: str) -> str:
        symbol = cls._text(value, reason, maximum=128)
        if _CODE_SYMBOL.fullmatch(symbol) is None:
            raise RouteDriverError(reason)
        return symbol

    @staticmethod
    def _language(path: str) -> str:
        if path.endswith(".py"):
            return "python"
        if path.endswith(".rs"):
            return "rust"
        return "unknown"

    @staticmethod
    def _module_map_owns(module_map: str, path: str, token: str) -> bool:
        """Prove a path/token pairing from one current MODULE_MAP row.

        A token occurring anywhere in the map is not ownership evidence for
        an arbitrary file.  The current map must put both the declared token
        and the exact path on the same table row.  Basename matching is
        deliberately forbidden because two owners may contain the same file
        name.
        """

        return any(
            line.startswith("|") and token in line and path in line
            for line in module_map.splitlines()
        )

    @staticmethod
    def _rust_code(source: str) -> str:
        """Remove Rust comments and literals while preserving line boundaries."""

        output: list[str] = []
        index = 0
        block_depth = 0
        length = len(source)
        while index < length:
            if block_depth:
                if source.startswith("/*", index):
                    block_depth += 1
                    output.extend("  ")
                    index += 2
                elif source.startswith("*/", index):
                    block_depth -= 1
                    output.extend("  ")
                    index += 2
                else:
                    output.append("\n" if source[index] == "\n" else " ")
                    index += 1
                continue
            if source.startswith("//", index):
                newline = source.find("\n", index)
                if newline == -1:
                    output.extend(" " * (length - index))
                    break
                output.extend(" " * (newline - index))
                output.append("\n")
                index = newline + 1
                continue
            if source.startswith("/*", index):
                block_depth = 1
                output.extend("  ")
                index += 2
                continue
            raw = re.match(r"(?:br|r)(#+)?\"", source[index:])
            if raw is not None:
                marker = raw.group(1) or ""
                start = index + len(raw.group(0))
                closing = '"' + marker
                end = source.find(closing, start)
                if end == -1:
                    output.extend("\n" if char == "\n" else " " for char in source[index:])
                    break
                literal = source[index : end + len(closing)]
                output.extend("\n" if char == "\n" else " " for char in literal)
                index = end + len(closing)
                continue
            if source[index] == "'" and not (
                index + 2 < length
                and (source[index + 2] == "'" or source[index + 1] == "\\")
            ):
                output.append(source[index])
                index += 1
                continue
            if source[index] in {'"', "'"} or (
                source[index] in {"b", "c"}
                and index + 1 < length
                and source[index + 1] in {'"', "'"}
            ):
                quote_index = index + 1 if source[index] in {"b", "c"} else index
                quote = source[quote_index]
                cursor = quote_index + 1
                while cursor < length:
                    if source[cursor] == "\\":
                        cursor += 2
                        continue
                    if source[cursor] == quote:
                        cursor += 1
                        break
                    cursor += 1
                literal = source[index:cursor]
                output.extend("\n" if char == "\n" else " " for char in literal)
                index = cursor
                continue
            output.append(source[index])
            index += 1
        return "".join(output)

    @classmethod
    def _declares_symbol(cls, source: str, symbol: str, language: str) -> bool:
        escaped = re.escape(symbol)
        if language == "python":
            pattern = rf"\b(?:def|class)\s+{escaped}\b"
        elif language == "rust":
            pattern = (
                rf"(?m)^\s*(?:(?:pub(?:\([^)]*\))?|async|const|unsafe|"
                rf"extern(?:\s+\"[^\"]+\")?)\s+)*"
                rf"(?:fn|struct|enum|trait|type|mod|const|static)\s+{escaped}\b"
            )
            source = cls._rust_code(source)
        else:
            return False
        return re.search(pattern, source) is not None

    @staticmethod
    def _rust_type_context(prefix: str) -> bool:
        """Reject Rust paths that occur in a type/signature context."""

        boundary = max(prefix.rfind(";"), prefix.rfind("{"), prefix.rfind("}"))
        segment = prefix[boundary + 1 :]
        arrow = segment.rfind("->")
        if arrow >= 0 and "=" not in segment[arrow + 2 :]:
            return True
        if re.search(r"\b(?:enum|struct|trait)\b[^{;]*\{\s*$", prefix):
            return True
        if re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:type|use)\b", segment):
            return True
        if re.match(r"\s*(?:fn|extern\b)", segment) and segment.count("(") > segment.count(")"):
            return True
        if re.match(r"\s*(?:let|const|static)\b", segment) and "=" not in segment:
            return True
        if re.search(r":\s*(?:&\s*(?:'[A-Za-z_][A-Za-z0-9_]*\s*)?(?:mut\s*)?)?$", segment):
            return True
        match_pos = prefix.rfind("match")
        opening = prefix.rfind("{")
        if (
            match_pos > prefix.rfind("}")
            and opening > match_pos
            and "=>" not in prefix[opening + 1 :]
        ):
            match_segment = prefix[opening + 1 :]
            if not re.search(r"\bif\b", match_segment) or re.search(
                r"\bif\s+let\b", match_segment
            ):
                return True
        if re.search(r"\b(?:if|while|for)\s+let\b[^{};]*$", prefix):
            return True
        if re.search(r"\bfor\b", segment) and " in " not in segment:
            return True
        has_open_closure = segment.count("|") % 2 == 1 and re.search(
            r"(?:^|=\s*(?:(?:async\s+)?(?:move\s+)?)?|[,(]\s*)\|", segment
        )
        if has_open_closure:
            return True
        macro_rules = prefix.rfind("macro_rules!")
        if macro_rules > max(prefix.rfind("}"), prefix.rfind(";")):
            return True
        return False

    @classmethod
    def _consumes_symbol(cls, source: str, symbol: str, language: str) -> bool:
        escaped = re.escape(symbol)
        if language == "python":
            return re.search(
                rf"(?<!def )(?<!class )\b{escaped}\s*\(", source
            ) is not None
        if language == "rust":
            code = cls._rust_code(source)
            patterns = [
                re.compile(
                    rf"\b{escaped}\s*::\s*[a-z_][A-Za-z0-9_]*\s*"
                    rf"(?:\(|!\s*(?:\(|\[|\{{))"
                )
            ]
            if symbol[:1].islower():
                patterns.extend(
                    (
                        re.compile(rf"\b{escaped}\s*\("),
                        re.compile(rf"\b{escaped}\s*!\s*(?:\(|\[|\{{)"),
                    )
                )
            for pattern in patterns:
                for match in pattern.finditer(code):
                    prefix = code[: match.start()]
                    if cls._rust_type_context(prefix) or re.search(
                        r"\b(?:fn|struct|enum|trait|type|mod|const|static)\s*$",
                        prefix,
                    ) or re.search(
                        r"\b(?:impl|for|macro_rules!)\s*(?:<[^>\n]*>)?\s*$",
                        prefix,
                    ):
                        continue
                    return True
            return False
        return False

    @staticmethod
    def _list(
        value: object,
        reason: str,
        *,
        maximum: int = _MAX_PROMOTION_LIST_ITEMS,
    ) -> list[object]:
        if not isinstance(value, list) or not value or len(value) > maximum:
            raise RouteDriverError(reason)
        return value

    @staticmethod
    def _object(value: object, keys: set[str], reason: str) -> dict[str, object]:
        if not isinstance(value, dict) or set(value) != keys:
            raise RouteDriverError(reason)
        return value

    def _proposal(self, raw: str, successor: EligibleSuccessor) -> dict[str, object] | PromotionPlanResult:
        if not isinstance(raw, str) or not raw or len(raw.encode("utf-8")) > MAX_PROMOTION_EVIDENCE_BYTES:
            return PromotionPlanResult("DECISION_REQUIRED", "promotion_planner_output_invalid")
        try:
            value = json.loads(raw)
        except json.JSONDecodeError:
            return PromotionPlanResult("DECISION_REQUIRED", "promotion_planner_output_invalid")
        if not isinstance(value, dict) or value.get("schema_version") != _ROUTE_EVIDENCE_SCHEMA:
            return PromotionPlanResult("DECISION_REQUIRED", "promotion_planner_output_invalid")
        if value.get("state") == "DECISION_REQUIRED":
            if set(value) != {"schema_version", "state", "reason"}:
                return PromotionPlanResult("DECISION_REQUIRED", "promotion_planner_output_invalid")
            try:
                reason = self._token(value["reason"], "promotion_planner_output_invalid")
            except RouteDriverError:
                return PromotionPlanResult("DECISION_REQUIRED", "promotion_planner_output_invalid")
            return PromotionPlanResult("DECISION_REQUIRED", f"promotion_planner:{reason}")
        required = {
            "schema_version", "packet_id", "accepted_main_sha", "owner_evidence",
            "caller_evidence", "test_evidence", "allowed_paths", "read_paths", "ordered_slices",
            "verification", "operations", "evidence_destinations", "decisions",
        }
        if set(value) != required:
            return PromotionPlanResult("DECISION_REQUIRED", "promotion_planner_output_invalid")
        if value.get("packet_id") != successor.packet_id or value.get("accepted_main_sha") != self.accepted_main_sha:
            return PromotionPlanResult("DECISION_REQUIRED", "promotion_planner_binding_mismatch")
        return value

    def verify(
        self,
        raw: str,
        successor: EligibleSuccessor,
        predecessor_receipt: str,
        closed_packet_id: str | None = None,
        retained_t3_request: T3Request | None = None,
        retained_t3_receipt: T3Receipt | None = None,
    ) -> PromotionPlanResult:
        proposal = self._proposal(raw, successor)
        if isinstance(proposal, PromotionPlanResult):
            return proposal
        try:
            return self._verify_proposal(
                proposal,
                successor,
                predecessor_receipt,
                closed_packet_id,
                retained_t3_request,
                retained_t3_receipt,
            )
        except RouteDriverError as exc:
            return PromotionPlanResult("DECISION_REQUIRED", exc.reason)

    def _verify_proposal(
        self,
        proposal: dict[str, object],
        successor: EligibleSuccessor,
        predecessor_receipt: str,
        closed_packet_id: str | None,
        retained_t3_request: T3Request | None,
        retained_t3_receipt: T3Receipt | None,
    ) -> PromotionPlanResult:
        raw_allowed = self._list(proposal["allowed_paths"], "promotion_allowed_paths_invalid")
        if any(not isinstance(path, str) for path in raw_allowed):
            raise RouteDriverError("promotion_allowed_paths_invalid")
        try:
            allowed_paths = tuple(
                sorted(artifact_contract.validate_allowed_paths(raw_allowed))
            )
        except artifact_contract.ArtifactContractError as exc:
            raise RouteDriverError("promotion_allowed_paths_invalid") from exc
        if len(set(allowed_paths)) != len(allowed_paths):
            raise RouteDriverError("promotion_allowed_paths_noncanonical")
        raw_read = self._list(proposal["read_paths"], "promotion_read_paths_invalid")
        if any(not isinstance(path, str) for path in raw_read):
            raise RouteDriverError("promotion_read_paths_invalid")
        try:
            read_paths = tuple(sorted(artifact_contract.validate_allowed_paths(raw_read)))
        except artifact_contract.ArtifactContractError as exc:
            raise RouteDriverError("promotion_read_paths_invalid") from exc
        if len(set(read_paths)) != len(read_paths):
            raise RouteDriverError("promotion_read_paths_noncanonical")
        if not set(allowed_paths).issubset(read_paths):
            raise RouteDriverError("promotion_allowed_paths_outside_read_paths")
        required_documents = {
            "docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md", "docs/CURRENT_STATUS.md",
        }
        if not required_documents.issubset(allowed_paths):
            raise RouteDriverError("promotion_allowed_paths_missing_canonical_documents")
        if successor.sketch.packet_class == "IMPLEMENT":
            prod_allowed = [
                p for p in allowed_paths
                if not (
                    p.startswith("docs/")
                    or p.endswith(".md")
                    or p in {"START_HERE.md", "AGENTS.md", "README.md", "CLAUDE.md", "LICENSE"}
                    or "tests" in p.split("/")
                    or "fixtures" in p.split("/")
                    or p.split("/")[-1].startswith("test_")
                    or p.split("/")[-1].endswith(("_test.rs", ".test.ts", ".test.js", ".test.mjs", ".spec.ts"))
                )
            ]
            test_allowed = [
                p for p in allowed_paths
                if (
                    "tests" in p.split("/")
                    or "fixtures" in p.split("/")
                    or p.split("/")[-1].startswith("test_")
                    or p.split("/")[-1].endswith(("_test.rs", ".test.ts", ".test.js", ".test.mjs", ".spec.ts"))
                )
            ]
            if not prod_allowed:
                if test_allowed:
                    raise RouteDriverError("promotion_implement_allowed_paths_test_only")
                raise RouteDriverError("promotion_implement_allowed_paths_lack_source")
            if successor.packet_id.startswith(("PE7-AC", "PE7-HE", "PE7-CWS", "PE7-MEMORY", "PE7-SKILL", "PE7-RWE")):
                engine_prod = [
                    p for p in prod_allowed
                    if p.startswith("engine/src/") or p == "engine/src" or p.startswith("engine/")
                ]
                if not engine_prod:
                    raise RouteDriverError("promotion_implement_allowed_paths_mismatched_target")
            elif successor.packet_id.startswith(("PE7-ROUTE-", "PE7-PLAN-", "PE7-CTRL-", "TOOL-")):
                route_prod = [
                    p for p in prod_allowed
                    if p.startswith(("scripts/", "tools/"))
                ]
                if not route_prod:
                    raise RouteDriverError("promotion_implement_allowed_paths_mismatched_target")
        for path in allowed_paths:
            self._source(path)
        for path in read_paths:
            self._source(path)

        module_map = self._source("docs/MODULE_MAP.md")
        owner_paths: list[str] = []
        for item in self._list(proposal["owner_evidence"], "promotion_owner_evidence_invalid"):
            entry = self._object(item, {"path", "module_map_token"}, "promotion_owner_evidence_invalid")
            path = self._text(entry["path"], "promotion_owner_evidence_invalid")
            token = self._token(entry["module_map_token"], "promotion_owner_evidence_invalid")
            source = self._source(path)
            if (
                path not in read_paths
                or not self._module_map_owns(module_map, path, token)
                or token not in source and token not in path
            ):
                raise RouteDriverError("promotion_owner_not_proved")
            owner_paths.append(path)
        if len(owner_paths) != len(set(owner_paths)):
            raise RouteDriverError("promotion_owner_evidence_invalid")

        caller_paths: list[str] = []
        for item in self._list(proposal["caller_evidence"], "promotion_caller_evidence_invalid"):
            entry = self._object(item, {"owner_path", "caller_path", "symbol"}, "promotion_caller_evidence_invalid")
            owner = self._text(entry["owner_path"], "promotion_caller_evidence_invalid")
            caller = self._text(entry["caller_path"], "promotion_caller_evidence_invalid")
            symbol = self._symbol(entry["symbol"], "promotion_caller_evidence_invalid")
            if owner not in owner_paths or caller not in read_paths:
                raise RouteDriverError("promotion_caller_not_proved")
            if not self._declares_symbol(self._source(owner), symbol, self._language(owner)) or not self._consumes_symbol(
                self._source(caller), symbol, self._language(caller)
            ):
                raise RouteDriverError("promotion_caller_not_proved")
            caller_paths.append(caller)
        if len(caller_paths) != len(set(caller_paths)):
            raise RouteDriverError("promotion_caller_evidence_invalid")

        test_paths: list[str] = []
        for item in self._list(proposal["test_evidence"], "promotion_test_evidence_invalid"):
            entry = self._object(item, {"target_path", "test_path", "symbol"}, "promotion_test_evidence_invalid")
            target = self._text(entry["target_path"], "promotion_test_evidence_invalid")
            test = self._text(entry["test_path"], "promotion_test_evidence_invalid")
            symbol = self._symbol(entry["symbol"], "promotion_test_evidence_invalid")
            if target not in set(owner_paths + caller_paths) or test not in read_paths:
                raise RouteDriverError("promotion_test_not_proved")
            if not self._declares_symbol(self._source(target), symbol, self._language(target)) or not self._consumes_symbol(
                self._source(test), symbol, self._language(test)
            ):
                raise RouteDriverError("promotion_test_not_proved")
            test_paths.append(test)
        if len(test_paths) != len(set(test_paths)):
            raise RouteDriverError("promotion_test_evidence_invalid")

        ordered_slices: list[str] = []
        for item in self._list(proposal["ordered_slices"], "promotion_slices_invalid"):
            entry = self._object(item, {"paths", "description"}, "promotion_slices_invalid")
            paths = self._list(entry["paths"], "promotion_slices_invalid")
            if any(not isinstance(path, str) or path not in allowed_paths for path in paths):
                raise RouteDriverError("promotion_slices_invalid")
            description = self._text(entry["description"], "promotion_slices_invalid")
            ordered_slices.append(f"{', '.join(paths)}: {description}")

        import local_verification

        verification = tuple(
            self._text(item, "promotion_verification_invalid")
            for item in self._list(proposal["verification"], "promotion_verification_invalid")
        )
        if len(set(verification)) != len(verification) or any(
            local_verification.allowlisted_command(command) is None for command in verification
        ):
            raise RouteDriverError("promotion_verification_invalid")

        operations = self._object(
            proposal["operations"], {"rollback", "cleanup", "retention"}, "promotion_operations_invalid"
        )
        operation_text: dict[str, str] = {}
        provenance: dict[str, object] = {"owners": proposal["owner_evidence"], "callers": proposal["caller_evidence"], "tests": proposal["test_evidence"]}
        for name in ("rollback", "cleanup", "retention"):
            entry = self._object(
                operations[name], {"source_path", "needle", "description"}, "promotion_operations_invalid"
            )
            path = self._text(entry["source_path"], "promotion_operations_invalid")
            needle = self._token(entry["needle"], "promotion_operations_invalid")
            description = self._text(entry["description"], "promotion_operations_invalid")
            if path not in allowed_paths or needle not in self._source(path):
                raise RouteDriverError(f"promotion_{name}_not_proved")
            operation_text[name] = f"{description} (proved by {path}:{needle})"
            provenance[name] = entry

        destinations: list[str] = []
        destination_proofs: list[dict[str, object]] = []
        for item in self._list(proposal["evidence_destinations"], "promotion_destinations_invalid"):
            entry = self._object(item, {"source_path", "needle", "description"}, "promotion_destinations_invalid")
            path = self._text(entry["source_path"], "promotion_destinations_invalid")
            needle = self._token(entry["needle"], "promotion_destinations_invalid")
            description = self._text(entry["description"], "promotion_destinations_invalid")
            if path not in allowed_paths or needle not in self._source(path):
                raise RouteDriverError("promotion_destination_not_proved")
            destinations.append(f"{description} ({path}:{needle})")
            destination_proofs.append(entry)
        if len(destinations) != len(set(destinations)):
            raise RouteDriverError("promotion_destinations_invalid")
        provenance["destinations"] = destination_proofs

        decisions = self._object(proposal["decisions"], set(_DECISION_KINDS), "promotion_decisions_invalid")
        decision_text: list[str] = []
        decision_proofs: dict[str, object] = {}
        for kind in sorted(_DECISION_KINDS):
            entry = self._object(decisions[kind], {"state", "source_path", "needle"}, "promotion_decisions_invalid")
            state = self._text(entry["state"], "promotion_decisions_invalid")
            if state == "DECISION_REQUIRED":
                return PromotionPlanResult("DECISION_REQUIRED", f"promotion_{kind}_decision_required")
            path = self._text(entry["source_path"], "promotion_decisions_invalid")
            needle = self._token(entry["needle"], "promotion_decisions_invalid")
            if state != "UNCHANGED" or path not in allowed_paths or needle not in self._source(path):
                raise RouteDriverError(f"promotion_{kind}_decision_unproved")
            decision_text.append(f"{kind} unchanged ({path}:{needle})")
            decision_proofs[kind] = entry
        provenance["decisions"] = decision_proofs

        status_document = self._source("docs/CURRENT_STATUS.md")
        evidence = CurrentMainEvidence(
            packet_id=successor.packet_id,
            accepted_main_sha=self.accepted_main_sha,
            status_document_sha256=hashlib.sha256(
                status_document.encode("utf-8")
            ).hexdigest(),
            owner_paths=tuple(owner_paths),
            caller_paths=tuple(caller_paths),
            test_paths=tuple(test_paths),
            allowed_paths=allowed_paths,
            read_paths=read_paths,
            ordered_slices=tuple(ordered_slices),
            verification=verification,
            rollback=operation_text["rollback"],
            cleanup=operation_text["cleanup"],
            retention=operation_text["retention"],
            evidence_destinations=tuple(destinations),
            decisions=tuple(decision_text),
        )
        manifest_sha256 = _json_sha256(inventory_manifest(self._source("docs/FUTURE_ROUTE.md")))
        if closed_packet_id is None:
            if len(successor.sketch.prerequisites) != 1:
                raise RouteDriverError("promotion_prerequisite_receipts_missing_or_invalid")
            closed_packet_id = successor.sketch.prerequisites[0]
        predecessor_status_document = self._status_with_ancestor_receipt(
            status_document, closed_packet_id, predecessor_receipt
        )
        return RoutePromotionPlanner().plan(
            successor,
            self.accepted_main_sha,
            predecessor_receipt,
            evidence,
            manifest_sha256,
            closed_packet_id=closed_packet_id,
            status_document=status_document,
            predecessor_status_document=predecessor_status_document,
            repo_path=self.repo_path,
            retained_t3_request=retained_t3_request,
            retained_t3_receipt=retained_t3_receipt,
        )


class RoutePromotionPlanner:
    """Deep promotion boundary: validate current-main evidence into one contract.

    The interface intentionally accepts a small evidence object instead of
    reading FUTURE_ROUTE's ``Allowed delta`` prose.  An upstream bounded
    planner may propose evidence, but this validator makes a generic or
    incomplete proposal a typed ``DECISION_REQUIRED`` result.
    """

    def plan(
        self,
        successor: EligibleSuccessor,
        accepted_main_sha: str,
        predecessor_receipt: str,
        evidence: CurrentMainEvidence | None,
        manifest_sha256: str | None,
        *,
        closed_packet_id: str | None = None,
        status_document: str | None = None,
        retained_t3_request: T3Request | None = None,
        retained_t3_receipt: T3Receipt | None = None,
        predecessor_status_document: str | None = None,
        repo_path: Path | None = None,
    ) -> PromotionPlanResult:
        if not isinstance(predecessor_receipt, str) or not predecessor_receipt.strip():
            return PromotionPlanResult("DECISION_REQUIRED", "promotion_predecessor_receipt_missing")
        if not isinstance(manifest_sha256, str) or plan_lane.SHA256.fullmatch(manifest_sha256) is None:
            return PromotionPlanResult("DECISION_REQUIRED", "promotion_manifest_missing_or_invalid")
        problem = _evidence_problem(successor, accepted_main_sha, evidence)
        if problem is not None:
            return PromotionPlanResult("DECISION_REQUIRED", problem)
        assert evidence is not None
        if status_document is not None and evidence.status_document_sha256 != hashlib.sha256(
            status_document.encode("utf-8")
        ).hexdigest():
            return PromotionPlanResult(
                "DECISION_REQUIRED", "promotion_status_document_binding_invalid"
            )
        if len(successor.sketch.outcome.strip()) < 20:
            return PromotionPlanResult("DECISION_REQUIRED", "promotion_goal_too_short")
        if len(evidence.rollback.strip()) < 20:
            return PromotionPlanResult("DECISION_REQUIRED", "promotion_rollback_too_short")
        if not successor.sketch.prerequisites:
            return PromotionPlanResult("DECISION_REQUIRED", "promotion_prerequisites_missing")
        if retained_t3_request is not None or retained_t3_receipt is not None:
            validated_t3_receipt = (
                None
                if retained_t3_request is None or retained_t3_receipt is None
                else validate_recorded_t3_receipt(
                    _t3_receipt_wire(retained_t3_receipt), retained_t3_request
                )[0]
            )
            if (
                closed_packet_id is None
                or retained_t3_request is None
                or retained_t3_receipt is None
                or validated_t3_receipt != retained_t3_receipt
                or retained_t3_receipt.disposition != "GO"
                or successor.sketch.prerequisites != (closed_packet_id,)
                or retained_t3_request.packet_id != closed_packet_id
                or retained_t3_receipt.packet_id != closed_packet_id
                or retained_t3_request.accepted_main_sha != accepted_main_sha
                or retained_t3_receipt.accepted_main_sha != accepted_main_sha
                or retained_t3_receipt.candidate_digest != retained_t3_request.candidate_digest
                or retained_t3_receipt.action_digest != retained_t3_request.action_digest
                or retained_t3_receipt.scope_digest != retained_t3_request.scope_digest
                or retained_t3_receipt.authority_owner_digest
                != retained_t3_request.authority_owner_digest
                or predecessor_receipt.strip() != t3_closeout_reference(retained_t3_receipt)
            ):
                return PromotionPlanResult(
                    "DECISION_REQUIRED", "promotion_t3_closeout_receipt_invalid"
                )
            prerequisite_receipts = (predecessor_receipt.strip(),)
        elif closed_packet_id is None:
            if len(successor.sketch.prerequisites) != 1:
                return PromotionPlanResult(
                    "DECISION_REQUIRED",
                    "promotion_prerequisite_receipts_missing_or_invalid",
                )
            try:
                prerequisite_receipts = (
                    verified_predecessor_receipt(
                        status_document or "",
                        successor.sketch.prerequisites[0],
                        predecessor_receipt,
                        accepted_main_sha,
                        repo_path,
                    ),
                )
            except RouteDriverError as exc:
                return PromotionPlanResult("DECISION_REQUIRED", exc.reason)
        else:
            try:
                prerequisite_receipts = bound_prerequisite_receipts(
                    successor,
                    closed_packet_id,
                    predecessor_receipt,
                    predecessor_status_document
                    if predecessor_status_document is not None
                    else status_document,
                    accepted_main_sha,
                    repo_path,
                )
            except RouteDriverError as exc:
                if len(successor.sketch.prerequisites) > 1 or exc.reason in {
                    "promotion_predecessor_receipt_unproved",
                    "promotion_predecessor_receipt_mismatch",
                }:
                    return PromotionPlanResult(
                        "DECISION_REQUIRED",
                        "promotion_prerequisite_receipts_missing_or_invalid",
                    )
                return PromotionPlanResult("DECISION_REQUIRED", exc.reason)
        evidence_sha256 = _json_sha256(_evidence_payload(evidence))
        packet_id, packet_class, worker_tier, risk_class, verification_family = successor.profile
        contract = {
            "manifest_sha256": manifest_sha256,
            "owner_paths": list(evidence.owner_paths),
            "caller_paths": list(evidence.caller_paths),
            "test_paths": list(evidence.test_paths),
            "allowed_paths": list(evidence.allowed_paths),
            "read_paths": list(evidence.read_paths),
            "ordered_slices": list(evidence.ordered_slices),
            "verification": list(evidence.verification),
            "rollback": evidence.rollback,
            "cleanup": evidence.cleanup,
            "retention": evidence.retention,
            "evidence_destinations": list(evidence.evidence_destinations),
            "decisions": list(evidence.decisions),
        }
        forbidden_changes = [
            "Do not use FUTURE_ROUTE static paths as current-main authority.",
            "Do not create a second controller, ledger, queue, lease, store, or workflow owner.",
            "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target.",
        ]
        pause_gates = [
            "Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.",
            "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.",
            "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.",
            "Stop before a Provider, target, automatic merge, authority consumption, or external effect.",
            "Do not retry a possibly executed external effect whose outcome is unknown.",
        ]
        forbidden_next_actions = [
            "Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.",
            "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.",
            "Do not start a successor whose promotion candidate has not been independently accepted.",
            *forbidden_changes,
        ]
        capsule = {
            "schema_version": "weak_agent_dispatch.v1",
            "packet_id": packet_id,
            "packet_state": "READY_FOR_EXECUTION",
            "dispatch_lane": "provider_free_repository_maintenance",
            "external_effect_limit": 0,
            "authority_consumption_allowed": False,
            "secret_values_allowed": False,
            "private_paths_allowed": False,
            "plan_lane_state": plan_lane.PLAN_LANE_ACTIVE,
            "goal": successor.sketch.outcome,
            "allowed_paths": list(evidence.allowed_paths),
            "read_paths": list(evidence.read_paths),
            "allowed_outputs": [
                "A provider-free change limited to the independently proved current-main allowed paths.",
                "Exact-head verification and review evidence through the existing lifecycle owners.",
            ],
            "prerequisites": list(successor.sketch.prerequisites),
            "prerequisite_receipts": list(prerequisite_receipts),
            "forbidden_changes": forbidden_changes,
            "ordered_steps": list(evidence.ordered_slices),
            "verification": list(evidence.verification),
            "rollback": evidence.rollback,
            "pause_gates": pause_gates,
            "expected_artifacts": list(evidence.evidence_destinations),
            "forbidden_next_actions": forbidden_next_actions,
            "promotion_evidence_sha256": evidence_sha256,
            "route_manifest_sha256": manifest_sha256,
            "verification_family": verification_family,
            "worker_tier": worker_tier,
            "risk_class": risk_class,
        }
        spec_digest = _json_sha256({
            "packet_id": packet_id,
            "accepted_main_sha": accepted_main_sha,
            "predecessor_receipt": predecessor_receipt.strip(),
            "manifest_sha256": manifest_sha256,
            "evidence_sha256": evidence_sha256,
            "capsule": capsule,
            "contract": contract,
        })
        candidate = PromotionCandidate(
            packet_id=packet_id,
            accepted_main_sha=accepted_main_sha,
            predecessor_receipt=predecessor_receipt.strip(),
            evidence_sha256=evidence_sha256,
            manifest_sha256=manifest_sha256,
            spec_digest=spec_digest,
            capsule=capsule,
            contract=contract,
        )
        if packet_class == "EFFECT":
            return PromotionPlanResult(
                "T3_REQUIRED",
                "effect_prepared_t3_receipt_required",
                candidate=candidate,
                t3_request=T3Request(
                    packet_id=packet_id,
                    accepted_main_sha=accepted_main_sha,
                    candidate_digest=spec_digest,
                    action_digest=_json_sha256({
                        "packet_id": packet_id,
                        "packet_class": packet_class,
                        "outcome": successor.sketch.outcome,
                    }),
                    scope_digest=_json_sha256({
                        "allowed_paths": contract["allowed_paths"],
                        "ordered_slices": contract["ordered_slices"],
                        "verification": contract["verification"],
                    }),
                    authority_owner_digest=_json_sha256({
                        "owner_paths": list(evidence.owner_paths),
                    }),
                    requested_action=successor.sketch.outcome,
                ),
                evidence=evidence,
            )
        return PromotionPlanResult(
            "READY_FOR_EXECUTION", "promotion_candidate_valid", candidate=candidate, evidence=evidence
        )


def _refresh_forward_order_window(
    document: str,
    *,
    active_id: str,
    active_state: str,
    risk_class: str,
) -> str:
    """Refresh the Authoritative Forward Order window projection.

    The block is routing prose, but its first line names the current window;
    leaving it stale while the Active Routing section advances would make the
    compiled NEXT_DECISION self-contradictory.  The replacement is
    deterministic: the exact active packet id, its machine state, and a
    provider-free detail when the risk class is ``none``.  The first hop line
    (the just-promoted successor) is consumed, and any block shape that
    cannot be proven is left untouched so an unexpected document fails closed
    instead of being silently rewritten.
    """

    heading = "## Authoritative Forward Order"
    index = document.find(heading)
    if index < 0:
        return document
    section_start = index + len(heading)
    section_end = document.find("\n## ", section_start)
    if section_end < 0:
        section_end = len(document)
    section = document[section_start:section_end]
    if "[window:" not in section:
        return document
    window_line = re.search(r"(?m)^\[window: [^\]]*\]\s*$", section)
    if window_line is None:
        return document
    detail = "provider-free" if risk_class == "none" else risk_class
    replacement = f"[window: {active_id} — {active_state}, {detail}]"
    hop_line = re.search(r"(?m)^→ .*$\n?", section[window_line.end():])
    refreshed = section[: window_line.start()] + replacement + "\n"
    if hop_line is not None:
        hop_start = window_line.end() + hop_line.start()
        hop_end = window_line.end() + hop_line.end()
        refreshed += section[window_line.end(): hop_start] + section[hop_end:]
    else:
        refreshed += section[window_line.end():]
    return document[:section_start] + refreshed + document[section_end:]


def compact_next_window(
    document: str,
    *,
    closed_packet_id: str,
    predecessor_receipt: str,
    active_packet_block: str,
    active_state: str = "READY_FOR_EXECUTION",
    closed_packet_state: str = "COMPLETE",
    retained_marker: str = "",
    active_risk_class: str = "none",
) -> str:
    """Replace routing history with one active window and one short binding.

    Completed packet receipts are durable in CURRENT_STATUS and Git/ledger
    history.  Keeping them in NEXT_DECISION would grow linearly with the
    route, so this operation deliberately retains only the immediately
    preceding binding.
    """

    if not plan_lane.PACKET_ID.fullmatch(closed_packet_id):
        raise RouteDriverError("route_closed_packet_invalid")
    if not isinstance(predecessor_receipt, str) or not predecessor_receipt.strip():
        raise RouteDriverError("route_predecessor_receipt_missing")
    if not isinstance(active_packet_block, str) or not active_packet_block.startswith("## Packet "):
        raise RouteDriverError("route_active_packet_block_invalid")
    active_match = plan_lane.PACKET_HEADING.search(active_packet_block)
    if active_match is None or not plan_lane.PACKET_ID.fullmatch(active_match.group("packet")):
        raise RouteDriverError("route_active_packet_block_invalid")
    if active_state not in {"READY_FOR_EXECUTION", "T3_REQUIRED"}:
        raise RouteDriverError("route_active_packet_state_invalid")
    if closed_packet_state not in {"COMPLETE", "IN_PROGRESS"}:
        raise RouteDriverError("route_closed_packet_state_invalid")
    if not isinstance(retained_marker, str) or len(retained_marker.encode("utf-8")) > 4 * 1024:
        raise RouteDriverError("route_retained_marker_invalid")
    if closed_packet_state == "COMPLETE" and retained_marker:
        raise RouteDriverError("route_completed_packet_retained_marker_forbidden")
    if not isinstance(active_risk_class, str) or not active_risk_class.strip():
        raise RouteDriverError("route_active_risk_class_invalid")
    common_marker = "## Common Execution Protocol"
    common_index = document.find(common_marker)
    routing_marker = "## Active Routing"
    routing_index = document.find(routing_marker)
    if routing_index >= 0:
        prefix = document[:routing_index].rstrip() + "\n\n"
    elif common_index >= 0:
        prefix = document[:common_index].rstrip() + "\n\n"
    else:
        prefix = document.rstrip() + "\n\n"
    prefix = _refresh_forward_order_window(
        prefix,
        active_id=active_match.group("packet"),
        active_state=active_state,
        risk_class=active_risk_class,
    )
    suffix = document[common_index:].lstrip() if common_index >= 0 else ""
    active_id = active_match.group("packet")
    historical_heading = "Completed" if closed_packet_state == "COMPLETE" else "Retained"
    compact = (
        f"{prefix}## Active Routing\n\n"
        f"1. `{active_id}` — `{active_state}`\n\n"
        f"## {historical_heading} ({closed_packet_id})\n\n"
        f"**Historical state:** `{closed_packet_state}`\n\n"
        f"**Historical evidence:** {predecessor_receipt.strip()}.{retained_marker}\n"
        f"{active_packet_block.strip()}\n\n{suffix}"
    )
    if len(compact.encode("utf-8")) > NEXT_DECISION_MAX_BYTES:
        raise RouteDriverError("route_compacted_next_document_too_large")
    return compact


@dataclass(frozen=True)
class RouteRunResult:
    """Wire result of the caller-independent repository route command."""

    state: str
    reason: str
    packet_id: str | None = None
    transitions: int = 0

    def to_wire(self) -> dict[str, object]:
        return {
            "kind": "repo-agent-route-run.v1",
            "state": self.state,
            "reason": self.reason,
            "packet_id": self.packet_id,
            "transitions": self.transitions,
        }


class RepositoryRouteRunner:
    """Refresh and drive the current route without accepting a packet selector.

    This adapter owns no durable state.  It obtains the current packet from
    accepted ``main`` on each turn and delegates packet work to the existing
    LocalRunOnce/lifecycle owners.  Its only terminal results are the typed
    route outcomes; ordinary worker/CI/review/restart drift statuses are
    retried through those same owners until the caller's explicit bounded
    transition limit detects unavailable infrastructure.
    """

    RECOVERABLE = frozenset({
        "failed", "worker_failed", "canonical_ci_repair_pending",
        "review_repair_pending", "claim_unavailable", "claim_rejected",
        "stale_checkout", "promotion_pr", "promotion_pending", "handed_off",
        "promotion_review_pending", "promotion_ready_pending", "promotion_ci_pending",
        "in_flight", "successor_current", "unavailable",
    })

    def __init__(
        self,
        *,
        repository: str,
        repo_path: Path,
        max_transitions: int = 128,
        github: object | None = None,
        runner: object | None = None,
        attempt_factory: Any = uuid.uuid4,
        t3_receipt_reader: Any = None,
        poll_interval_seconds: float = 5.0,
        recovery_timeout_seconds: int = 900,
        sleeper: Any = time.sleep,
        clock: Any = time.monotonic,
    ) -> None:
        if not isinstance(max_transitions, int) or max_transitions < 1 or max_transitions > 256:
            raise ValueError("route max_transitions must be between 1 and 256")
        if (
            isinstance(poll_interval_seconds, bool)
            or not isinstance(poll_interval_seconds, (int, float))
            or not 1 <= float(poll_interval_seconds) <= 60
            or not callable(sleeper)
            or not isinstance(recovery_timeout_seconds, int)
            or isinstance(recovery_timeout_seconds, bool)
            or not 60 <= recovery_timeout_seconds <= 3600
            or not callable(clock)
        ):
            raise ValueError("route recovery configuration is invalid")
        self.repository = repository
        self.repo_path = Path(repo_path)
        self.max_transitions = max_transitions
        self._github = github
        self._runner = runner
        self._attempt_factory = attempt_factory
        self._current_t3_request: T3Request | None = None
        self._current_complete_receipt: str | None = None
        self._t3_receipt_reader = t3_receipt_reader
        # Tests inject a runner and must remain deterministic.  A real route
        # process polls recoverable controller state instead of burning its
        # bounded transition budget in a tight loop while CI/review advances.
        self._poll_recoverable = runner is None
        self._poll_interval_seconds = float(poll_interval_seconds)
        self._recovery_timeout_seconds = recovery_timeout_seconds
        self._sleeper = sleeper
        self._clock = clock

    def _wait_for_recovery(self) -> None:
        if self._poll_recoverable:
            self._sleeper(self._poll_interval_seconds)

    def _read_t3_receipt(self, request: T3Request) -> object | None:
        """Read only an existing ledger-backed source-authoritative receipt.

        The concrete GitHub ledger adapter is deliberately injected by the
        existing controller transport. The route driver never writes this
        record and cannot synthesize a decision source conclusion.
        """

        if self._t3_receipt_reader is not None:
            return self._t3_receipt_reader(request)
        try:
            import local_loop
            import state_manager

            github = self._github
            if github is None:
                github = local_loop.GitHubAdapter(self.repository)
            ledger_issue = github.plan_ledger_issue()
            state = state_manager.read_dispatch_state(
                ledger_issue,
                f"route-t3:{request.packet_id}:{request.candidate_digest}",
                self.repository,
            )
        except (
            AttributeError,
            OSError,
            ValueError,
            state_manager.StateUnavailableError,
            local_loop.LoopUnavailable,
        ):
            return None
        if (
            isinstance(state, dict)
            and state.get("action") == "route-t3-receipt"
            and state.get("status") == "authorized"
            and isinstance(state.get("details"), dict)
        ):
            return state["details"]
        return None

    def _current_packet(self) -> tuple[str | None, str]:
        # Imports stay lazy to keep the pure planner usable in deterministic
        # unit tests and to avoid inventing a second GitHub transport.
        import local_loop

        self._current_t3_request = None
        self._current_complete_receipt = None
        github = self._github or local_loop.GitHubAdapter(self.repository)
        metadata = github.repository_metadata()
        branch = metadata.get("default_branch")
        if not isinstance(branch, str) or not branch:
            raise RouteDriverError("route_default_branch_unavailable")
        # Every transition refreshes exactly the accepted default-branch ref
        # before it reads routing documents or asks the lifecycle adapter to
        # bind a plan.  This is local fetch-only state, not a checkout, merge,
        # or remote write.
        try:
            local_loop.GitAdapter().refresh_origin_main(self.repo_path, branch)
        except local_loop.LoopUnavailable as exc:
            raise RouteDriverError("route_origin_main_refresh_unavailable") from exc
        accepted_main_sha = github.accepted_main_sha(branch)
        document = github.accepted_plan_document(accepted_main_sha)
        status_document = github.accepted_status_document(accepted_main_sha)
        # Handle an authenticated EFFECT pause before asking the ordinary plan
        # parser for its deliberately READY-only execution candidate.
        t3_request = current_t3_request(document, accepted_main_sha)
        if t3_request is not None:
            self._current_t3_request = t3_request
            return t3_request.packet_id, accepted_main_sha
        try:
            completed_ids = _accepted_completed_ids(status_document)
            candidate = plan_lane.parse(
                document,
                accepted_main_sha,
                completed_packet_ids=completed_ids,
            )
        except plan_lane.PlanLaneError as exc:
            if exc.reason == "plan_allowed_paths_invalid":
                # A pre-ledger merge may have an accepted workflow path in its
                # historical contract.  It is readable only to bind the
                # completed receipt into promotion; the ordinary parser and
                # every patch artifact remain workflow-write-denying.
                try:
                    candidate = plan_lane.parse_bootstrap(
                        document,
                        accepted_main_sha,
                        completed_packet_ids=completed_ids,
                    )
                except plan_lane.PlanLaneError:
                    raise exc
                if (
                    candidate.packet_id not in completed_ids
                    or not bootstrap_reconcile_marked(document, candidate.packet_id)
                ):
                    raise exc
            elif exc.reason != "plan_packet_absent":
                raise
            else:
                route_document = github.accepted_route_document(accepted_main_sha)
                manifest = inventory_manifest(route_document)
                if manifest.get("packet_count") == 0:
                    return None, accepted_main_sha
                raise RouteDriverError("route_current_window_missing") from exc
        if candidate.packet_id in completed_ids:
            if not bootstrap_reconcile_marked(document, candidate.packet_id):
                raise RouteDriverError("route_bootstrap_marker_missing_or_invalid")
            self._current_complete_receipt = accepted_complete_receipt(
                status_document, candidate.packet_id
            )
        return candidate.packet_id, accepted_main_sha

    def _runner_instance(self) -> object:
        if self._runner is not None:
            return self._runner
        import local_run_once

        self._runner = local_run_once.LocalRunOnce(
            repository=self.repository, repo_path=self.repo_path
        )
        return self._runner

    def run(self) -> dict[str, object]:
        transitions = 0
        last_packet: str | None = None
        last_recovery_marker: tuple[str, str, str] | None = None
        unavailable_since: float | None = None
        recovery_stop_reason = "route_transition_limit_exhausted"

        def recover(
            packet_id: str,
            status: object,
            details: object,
            *,
            stable_poll: bool = True,
        ) -> bool:
            """Poll a stable production wait without exhausting transition budget.

            CI and review may legitimately outlast a fixed number of
            five-second polls. The budget therefore limits distinct recovery
            transitions, not repeated observations of one unchanged state.
            Injected test runners retain count-on-every-call behavior so a
            deterministic fake cannot spin indefinitely.
            """

            nonlocal transitions, last_recovery_marker, unavailable_since, recovery_stop_reason
            reason = (
                str(details.get("reason", ""))
                if isinstance(details, dict)
                else ""
            )
            marker = (packet_id, str(status), reason)
            if status == "unavailable" and self._poll_recoverable:
                observed = float(self._clock())
                if unavailable_since is None:
                    unavailable_since = observed
                elif observed - unavailable_since >= self._recovery_timeout_seconds:
                    recovery_stop_reason = "route_controller_unavailable_timeout"
                    return False
            else:
                unavailable_since = None
            if (
                not self._poll_recoverable
                or not stable_poll
                or marker != last_recovery_marker
            ):
                transitions += 1
                last_recovery_marker = marker
            self._wait_for_recovery()
            if transitions >= self.max_transitions:
                recovery_stop_reason = "route_transition_limit_exhausted"
            return transitions < self.max_transitions

        while transitions < self.max_transitions:
            try:
                packet_id, _accepted_main_sha = self._current_packet()
            except (RouteDriverError, plan_lane.PlanLaneError, OSError, ValueError) as exc:
                return RouteRunResult(
                    "UNRECOVERABLE_INFRASTRUCTURE_FAILURE", str(exc)[:200], last_packet, transitions
                ).to_wire()
            if packet_id is None:
                return RouteRunResult(
                    "ROUTE_EXHAUSTED", "accepted_inventory_empty", last_packet, transitions
                ).to_wire()
            last_packet = packet_id
            if self._current_t3_request is not None:
                raw_receipt = self._read_t3_receipt(self._current_t3_request)
                if raw_receipt is not None:
                    receipt, receipt_reason = validate_t3_receipt(
                        raw_receipt, self._current_t3_request
                    )
                    if receipt is None:
                        return RouteRunResult(
                            "DECISION_REQUIRED", receipt_reason, packet_id, transitions
                        ).to_wire()
                    if receipt.disposition != "GO":
                        return RouteRunResult(
                            "DECISION_REQUIRED", "route_t3_non_go_requires_canonical_rewrite",
                            packet_id, transitions,
                        ).to_wire()
                    try:
                        route_runner = self._runner_instance()
                        resume = getattr(route_runner, "run_effect_route_once", None)
                        if not callable(resume):
                            return RouteRunResult(
                                "DECISION_REQUIRED", "route_effect_resume_owner_unavailable",
                                packet_id, transitions,
                            ).to_wire()
                        effect_result = resume(self._current_t3_request, receipt)
                        effect_status = getattr(effect_result, "status", None)
                        effect_details = getattr(effect_result, "details", {})
                        if effect_status == "outcome_unknown":
                            return RouteRunResult(
                                "OUTCOME_UNKNOWN",
                                str(effect_details.get("reason", "route_effect_outcome_unknown")),
                                packet_id,
                                transitions,
                            ).to_wire()
                        if effect_status in self.RECOVERABLE:
                            if not recover(packet_id, effect_status, effect_details):
                                return RouteRunResult(
                                    "UNRECOVERABLE_INFRASTRUCTURE_FAILURE",
                                    recovery_stop_reason,
                                    packet_id,
                                    transitions,
                                ).to_wire()
                            continue
                        resumed = "RESUMED" if effect_status in {
                            "promoted", "successor_current", "t3_required",
                        } else "UNPROVED"
                    except (OSError, ValueError) as exc:
                        return RouteRunResult(
                            "UNRECOVERABLE_INFRASTRUCTURE_FAILURE",
                            str(exc)[:200],
                            packet_id,
                            transitions,
                        ).to_wire()
                    if resumed == "RESUMED":
                        transitions += 1
                        last_recovery_marker = None
                        continue
                    return RouteRunResult(
                        "DECISION_REQUIRED", "route_effect_resume_unproved", packet_id, transitions
                    ).to_wire()
                return RouteRunResult(
                    "T3_REQUIRED",
                    "effect_prepared_t3_receipt_required",
                    packet_id,
                    transitions,
                ).to_wire()
            runner = self._runner_instance()
            try:
                if self._current_complete_receipt is not None:
                    bootstrap = getattr(runner, "bootstrap_route_once", None)
                    if not callable(bootstrap):
                        return RouteRunResult(
                            "DECISION_REQUIRED", "route_bootstrap_owner_unavailable", packet_id, transitions
                        ).to_wire()
                    result = bootstrap(packet_id, self._current_complete_receipt)
                else:
                    reconcile = getattr(type(runner), "reconcile_plan", None)
                    result = runner.reconcile_plan(packet_id) if callable(reconcile) else None
                    if result is None:
                        attempt = str(self._attempt_factory())
                        result = runner.run_plan_once(packet_id, attempt)
            except (OSError, ValueError) as exc:
                if not recover(
                    packet_id,
                    "route_adapter_exception",
                    {"reason": str(exc)[:200]},
                    stable_poll=False,
                ):
                    return RouteRunResult(
                        "UNRECOVERABLE_INFRASTRUCTURE_FAILURE", str(exc)[:200], packet_id, transitions
                    ).to_wire()
                continue
            status = getattr(result, "status", None)
            details = getattr(result, "details", {})
            worker_failure_reason = (
                details.get("worker_failure_reason")
                if isinstance(details, dict)
                else None
            )
            if status == "failed" and worker_failure_reason in {
                "authentication_failure",
                "usage_or_credit_exhaustion",
            }:
                return RouteRunResult(
                    "UNRECOVERABLE_INFRASTRUCTURE_FAILURE",
                    f"route_worker_{worker_failure_reason}",
                    packet_id,
                    transitions,
                ).to_wire()
            if status == "outcome_unknown":
                return RouteRunResult("OUTCOME_UNKNOWN", str(details.get("reason", "outcome_unknown")), packet_id, transitions).to_wire()
            if status == "failed_unknown_output":
                return RouteRunResult(
                    "OUTCOME_UNKNOWN",
                    str(details.get("reason", "failed_unknown_output")),
                    packet_id,
                    transitions,
                ).to_wire()
            if status == "terminal":
                claim_status = details.get("claim_status") if isinstance(details, dict) else None
                if claim_status in {"failed_unknown_output", "outcome_unknown"}:
                    return RouteRunResult(
                        "OUTCOME_UNKNOWN",
                        str(details.get("reason", "plan_terminal_outcome_unknown")),
                        packet_id,
                        transitions,
                    ).to_wire()
                if claim_status == "closed_out":
                    status = "closed_out"
                elif claim_status == "failed":
                    status = "failed"
                else:
                    return RouteRunResult(
                        "DECISION_REQUIRED",
                        str(details.get("reason", "plan_terminal_unproved")),
                        packet_id,
                        transitions,
                    ).to_wire()
            if status in {"control_stopped", "rejected"}:
                return RouteRunResult("DECISION_REQUIRED", str(details.get("reason", status)), packet_id, transitions).to_wire()
            if status == "closed_out":
                terminal_state = details.get("terminal_packet_state")
                if isinstance(terminal_state, str) and terminal_state.casefold() in {
                    "no_go", "no-go", "defer", "decline", "insufficient", "harm",
                }:
                    return RouteRunResult(
                        "DECISION_REQUIRED",
                        "route_no_go_requires_canonical_rewrite",
                        packet_id,
                        transitions,
                    ).to_wire()
                # The closeout/promotion adapter is still an internal primitive:
                # it receives the closed packet only from this verified result,
                # never from the route-run caller.
                closed_attempt = getattr(result, "attempt_id", None)
                if _canonical_route_attempt(closed_attempt) is None:
                    return RouteRunResult(
                        "DECISION_REQUIRED", "route_closeout_attempt_unproved", packet_id, transitions
                    ).to_wire()
                promoted = runner.run_route_once(packet_id, closed_attempt)
                promotion_status = getattr(promoted, "status", None)
                promotion_details = getattr(promoted, "details", {})
                if promotion_status == "outcome_unknown":
                    return RouteRunResult("OUTCOME_UNKNOWN", str(promotion_details.get("reason", "promotion_outcome_unknown")), packet_id, transitions).to_wire()
                if promotion_status == "t3_required":
                    return RouteRunResult(
                        "T3_REQUIRED",
                        "effect_prepared_t3_receipt_required",
                        packet_id,
                        transitions,
                    ).to_wire()
                if promotion_status == "promoted":
                    transitions += 1
                    last_recovery_marker = None
                    continue
                if promotion_status == "bounded_pause":
                    return RouteRunResult(
                        "DECISION_REQUIRED",
                        str(promotion_details.get("reason", "promotion_evidence_unproved")),
                        packet_id,
                        transitions,
                    ).to_wire()
                if promotion_status in self.RECOVERABLE:
                    if not recover(packet_id, promotion_status, promotion_details):
                        return RouteRunResult(
                            "UNRECOVERABLE_INFRASTRUCTURE_FAILURE",
                            recovery_stop_reason,
                            packet_id,
                            transitions,
                        ).to_wire()
                    continue
                return RouteRunResult("DECISION_REQUIRED", str(promotion_details.get("reason", promotion_status)), packet_id, transitions).to_wire()
            if status in self.RECOVERABLE:
                reason = (
                    str(details.get("reason", ""))
                    if isinstance(details, dict)
                    else ""
                )
                if (
                    status in {"in_flight", "claim_unavailable"}
                    and last_recovery_marker == (packet_id, str(status), reason)
                ):
                    return RouteRunResult(
                        "OUTCOME_UNKNOWN",
                        reason or "in_flight_unbounded",
                        packet_id,
                        transitions,
                    ).to_wire()
                if not recover(packet_id, status, details):
                    return RouteRunResult(
                        "UNRECOVERABLE_INFRASTRUCTURE_FAILURE",
                        recovery_stop_reason,
                        packet_id,
                        transitions,
                    ).to_wire()
                continue
            return RouteRunResult("DECISION_REQUIRED", str(details.get("reason", status or "route_state_unknown")), packet_id, transitions).to_wire()
        return RouteRunResult(
            "UNRECOVERABLE_INFRASTRUCTURE_FAILURE",
            "route_transition_limit_exhausted",
            last_packet,
            transitions,
        ).to_wire()
