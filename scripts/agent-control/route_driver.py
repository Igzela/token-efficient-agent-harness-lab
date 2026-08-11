"""Promotion compiler and continuous route driver for the Plan lane.

The compiler is the deterministic, provider-free core of route automation:
it parses the checked ``future-route-inventory:v1`` manifest and the eligible
successor's packet sketch from ``docs/FUTURE_ROUTE.md``, proves that exactly
one successor is eligible on the current accepted routing, and compiles the
successor's refreshed twelve-field contract and ``weak-agent-dispatch:v1``
capsule into replace-only updates of ``docs/NEXT_DECISION.md``,
``docs/FUTURE_ROUTE.md``, and ``docs/CURRENT_STATUS.md``.  The compiled
documents are authoritative only after the canonical closeout/promotion PR is
independently reviewed, green on canonical CI, and merged; compilation itself
never executes the successor, never writes a target, and never mints
authority.

The driver entry points reuse the existing Plan Execution Ledger, PR binding,
lifecycle receipt, and merge-state owners; they add no second ledger,
controller, store, scheduler, or routing owner.
"""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from typing import Any

import artifact_contract
import plan_lane


INVENTORY_MARKER = re.compile(
    r"<!--\s*future-route-inventory:v1\s*(\{.*?\})\s*-->", re.DOTALL
)
NEXT_DECISION_MAX_BYTES = 64 * 1024
MAX_SKETCH_FIELD_CHARS = 8 * 1024
MAX_MANIFEST_BYTES = 256 * 1024

PACKET_CLASSES = frozenset({"CONTRACT", "IMPLEMENT", "EFFECT", "CLOSEOUT"})
CLASS_DEFAULT_TIER = {"CONTRACT": "T2", "IMPLEMENT": "T1", "EFFECT": "T3", "CLOSEOUT": "T2"}
CLASS_DEFAULT_RISK = {"EFFECT": "external_effect"}
CLASS_DEFAULT_VERIFICATION = {
    "CONTRACT": "docs_evidence_review",
    "IMPLEMENT": "source_focused_full",
    "EFFECT": "external_effect_evidence",
    "CLOSEOUT": "evidence_review",
}

_PATH_TOKEN = re.compile(r"(?<![A-Za-z0-9])(?:[A-Za-z0-9_.-]+/)+[A-Za-z0-9_.-]+")
_VERIFICATION_COMMANDS = {
    "docs_evidence_review": [
        "PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context",
        "uv run --no-project python scripts/check_agent_handoff.py",
        "git diff --check",
    ],
    "source_focused_full": [
        "PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context "
        "tests.test_agent_plan_lane tests.test_agent_plan_lifecycle tests.test_agent_plan_promotion",
        "uv run --no-project python scripts/check_agent_handoff.py",
        "uv run --no-project python tools/check_security_baseline.py",
        "git diff --check",
    ],
    "evidence_review": [
        "uv run --no-project python scripts/check_agent_handoff.py",
        "git diff --check",
    ],
}
_CLOSEOUT_FIELDS = ("Prerequisite", "Class", "Outcome", "Allowed delta", "Exit", "Stop")


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


def _json_sha256(value: object) -> str:
    encoded = json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _bounded_text(value: str, field: str) -> str:
    if not value or len(value) > MAX_SKETCH_FIELD_CHARS:
        raise RouteDriverError(f"route_{field}_missing_or_invalid")
    return value.strip()


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
    states or durable accepted receipts) and whose class is not ``EFFECT``
    (external-effect packets need a separate fresh T3 operator authority and
    are never auto-promoted by the driver).  The closed packet itself, zero
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
        if profile[1] == "EFFECT":
            continue
        sketch = sketches[packet_id]
        if sketch.packet_class != profile[1]:
            raise RouteDriverError("route_profile_class_mismatch")
        incomplete = [
            prerequisite
            for prerequisite in sketch.prerequisites
            if states.get(prerequisite) != "COMPLETE" and prerequisite not in completed_ids
        ]
        if incomplete:
            continue
        return EligibleSuccessor(packet_id=packet_id, sketch=sketch, profile=profile)
    raise RouteDriverError("no_eligible_successor")


_ACCEPTED_RECEIPT_ROW = re.compile(
    rf"^\|\s*`?(?P<packet>{plan_lane.PACKET_TOKEN})`?\s*\|\s*`?COMPLETE`?\s*\|"
    r"\s*(?P<evidence>[^|]+?)\s*\|\s*$",
    re.MULTILINE,
)


def _accepted_completed_ids(status_document: str) -> frozenset[str]:
    """Return durable accepted-receipt identities from CURRENT_STATUS."""

    receipt_section = re.search(
        r"## Accepted Packet Receipts\s*(?P<body>.*?)(?=^## |\Z)",
        status_document,
        re.MULTILINE | re.DOTALL,
    )
    if receipt_section is None:
        return frozenset()
    ids = {
        match.group("packet")
        for match in _ACCEPTED_RECEIPT_ROW.finditer(receipt_section.group("body"))
    }
    return frozenset(ids)


def _delta_paths(allowed_delta: str) -> list[str]:
    candidates: list[str] = []
    for match in _PATH_TOKEN.finditer(allowed_delta):
        path = match.group(0)
        if not any(
            path.startswith(prefix)
            for prefix in ("scripts/", "tests/", "docs/", "engine/", "sdk/", "tools/", "src/")
        ):
            continue
        if path not in candidates:
            candidates.append(path)
    if not candidates:
        return []
    try:
        return artifact_contract.validate_allowed_paths(candidates)
    except artifact_contract.ArtifactContractError as exc:
        raise RouteDriverError("route_allowed_paths_invalid") from exc


def _compile_capsule(
    successor: EligibleSuccessor, accepted_main_sha: str
) -> dict[str, Any]:
    """Compile the bounded ``weak-agent-dispatch:v1`` capsule for one successor.

    Every field is derived from the checked sketch and manifest profile or
    from a class-level template; no free-form value is invented and every
    derived fact is bounded.  ``EFFECT`` successors are excluded upstream, so
    the capsule always carries the zero-external-effect plan-lane contract.
    """

    packet_id = successor.packet_id
    sketch = successor.sketch
    _packet, packet_class, _tier, _risk, family = successor.profile
    allowed_paths = _delta_paths(sketch.allowed_delta)
    if not allowed_paths:
        raise RouteDriverError("successor_allowed_paths_underived")
    verification = _VERIFICATION_COMMANDS.get(family)
    if not verification:
        raise RouteDriverError("successor_verification_family_underived")
    goal = _bounded_text(sketch.outcome, "goal")
    if len(goal) < 20:
        raise RouteDriverError("successor_goal_not_actionable")
    prerequisites = _bounded_list(list(sketch.prerequisites), "prerequisites")
    forbidden_changes = _bounded_list(
        [
            "Do not create a second Plan Execution Ledger, Issue, store, controller, "
            "scheduler, queue, lease, or routing owner.",
            "Do not give a child merge credentials, GitHub authority, Provider "
            "credentials, or T3 authority.",
            "Do not call a Provider, read credentials, write a target, auto-merge, "
            "release, deploy, execute an EFFECT, or handle T3 automatically.",
            "Do not let model text or self-report advance routing state.",
            "Do not silently edit the future-route inventory manifest or packet "
            "sketches; routing changes are planning diffs only.",
            "Do not change product runtime, schema, evaluator, budget, authority, "
            "or branch protection.",
        ],
        "forbidden_changes",
    )
    forbidden_next_actions = _bounded_list(
        [
            "Do not activate successor execution, EFFECT execution, automatic T3 "
            "handling, or PREFLIGHT from this packet.",
            "Do not bypass Draft/Ready discipline, exact-head review, canonical CI, "
            "or merge eligibility.",
            "Do not treat missing, conflicting, stale, or outcome-unknown routing "
            "or receipts as success.",
        ],
        "forbidden_next_actions",
    )
    pause_gates = _bounded_list(
        [
            "Stop on unprovable or stale routing, missing or conflicting closeout "
            "receipt, or promotion of zero or multiple successors.",
            "Stop on any Provider, target, credential, authority, successor "
            "execution, EFFECT, or T3 path.",
            "Stop on CI failed, queued, or missing; review absent or stale; or "
            "ineligible merge.",
        ],
        "pause_gates",
    )
    ordered_steps = _bounded_list(
        [
            "Refresh accepted main and prove the accepted predecessor receipt and "
            "the checked inventory manifest.",
            f"Execute the {packet_class} packet's accepted contract inside its "
            "allowed paths only.",
            "Run the compiled verification commands and the bounded handoff, "
            "security, and diff checks.",
            "Stop for exact-head review and canonical CI; do not self-report "
            "acceptance.",
        ],
        "ordered_steps",
    )
    expected_artifacts = _bounded_list(
        [
            f"One exact-head reviewed {packet_class} implementation for {packet_id}",
            "Controller-owned receipts bound to the ledger with zero Provider, "
            "target, merge, credential, authority, or T3 effect",
            "Focused and applicable full verification results",
        ],
        "expected_artifacts",
    )
    allowed_outputs = _bounded_list(
        [
            f"{packet_class} packet evidence bound to the exact plan claim with "
            "readback evidence only",
            "Controller-owned lifecycle receipts (CI, review, merge, closeout) on "
            "the Plan Execution Ledger",
        ],
        "allowed_outputs",
    )
    rollback = _bounded_text(
        f"Revert the {packet_class} packet's code and documents; retain ledger "
        "closeout and routing receipts; no external effect is created by this packet.",
        "rollback",
    )
    return {
        "schema_version": "weak_agent_dispatch.v1",
        "packet_id": packet_id,
        "packet_state": "READY_FOR_EXECUTION",
        "dispatch_lane": "provider_free_repository_maintenance",
        "external_effect_limit": 0,
        "authority_consumption_allowed": False,
        "secret_values_allowed": False,
        "private_paths_allowed": False,
        "plan_lane_state": plan_lane.PLAN_LANE_ACTIVE,
        "goal": goal,
        "allowed_paths": list(allowed_paths),
        "prerequisites": list(prerequisites),
        "forbidden_changes": list(forbidden_changes),
        "forbidden_next_actions": list(forbidden_next_actions),
        "pause_gates": list(pause_gates),
        "ordered_steps": list(ordered_steps),
        "expected_artifacts": list(expected_artifacts),
        "allowed_outputs": list(allowed_outputs),
        "read_paths": [
            "START_HERE.md",
            "AGENTS.md",
            "docs/CURRENT_STATUS.md",
            "docs/NEXT_DECISION.md",
            "docs/FUTURE_ROUTE.md",
            "docs/MODULE_MAP.md",
            "docs/ARCHITECTURE_BOOK.md",
            "docs/REAL_WORLD_TESTING_PLAYBOOK.md",
            "scripts/session_context.py",
            "scripts/agent-control/",
        ],
        "verification": list(verification),
        "rollback": rollback,
        "prerequisite_receipts": [],
        "known_store_mutations": [],
    }


def _canonical_spec_digest(capsule: dict[str, Any], accepted_main_sha: str) -> str:
    spec = {
        "schema_version": plan_lane.SCHEMA_VERSION,
        "packet_id": capsule["packet_id"],
        "state": "READY_FOR_EXECUTION",
        "source_main_sha": accepted_main_sha,
        "goal": capsule["goal"],
        "allowed_paths": list(capsule["allowed_paths"]),
        "prerequisites": list(capsule["prerequisites"]),
        "forbidden_changes": list(capsule["forbidden_changes"]),
        "verification": list(capsule["verification"]),
        "rollback": [capsule["rollback"]],
    }
    return plan_lane._canonical_spec(spec)


def _packet_block(
    successor: EligibleSuccessor,
    capsule: dict[str, Any],
    accepted_main_sha: str,
    predecessor_evidence: str,
    manifest_sha256: str,
) -> str:
    packet_id = successor.packet_id
    sketch = successor.sketch
    prerequisites = ", ".join(sketch.prerequisites) or "none"
    capsule_json = json.dumps(capsule, ensure_ascii=False, sort_keys=True)
    return (
        f"## Packet {packet_id}\n\n"
        f"**State:** `READY_FOR_EXECUTION`\n\n"
        f"**Prerequisite:** {prerequisites} — COMPLETE on accepted main "
        f"`{accepted_main_sha}` ({predecessor_evidence}).\n\n"
        f"**Class:** `{sketch.packet_class}`\n\n"
        f"**Outcome:** {sketch.outcome}\n\n"
        f"**Allowed delta:** {sketch.allowed_delta}\n\n"
        f"**Exit:** {sketch.exit_statement}\n\n"
        f"**Stop:** {sketch.stop}\n\n"
        f"### Twelve-field contract\n\n"
        f"1. **Outcome and non-goals.** {sketch.outcome}\n"
        f"2. **Prerequisites and evidence.** {prerequisites} COMPLETE on accepted "
        f"main `{accepted_main_sha}` ({predecessor_evidence}).\n"
        f"3. **Owners and paths.** {sketch.allowed_delta}\n"
        f"4. **Frozen subject identity and invariants.** The compiled successor "
        f"binds `{packet_id}`, the checked inventory manifest SHA "
        f"`{manifest_sha256}`, accepted main `{accepted_main_sha}`, and the "
        f"closed-out predecessor evidence ({predecessor_evidence}). Exactly one "
        f"successor is compiled and promoted at a time; the compiled successor "
        f"is never executed; the manifest is never silently edited; no model "
        f"self-report advances routing; no child receives merge credentials, "
        f"GitHub authority, Provider credentials, or T3 authority; a "
        f"closeout/promotion PR never auto-merges.\n"
        f"5. **Only semantic delta.** {sketch.exit_statement}\n"
        f"6. **Forbidden changes.** {sketch.stop}\n"
        f"7. **Ordered implementation slices.** {sketch.allowed_delta}\n"
        f"8. **Failure, recovery, and stop taxonomy.** Fail closed on unprovable "
        f"or stale routing, missing or conflicting closeout receipt, promotion of "
        f"zero or multiple successors, unknown external outcome, or unavailable "
        f"owner.\n"
        f"9. **Verification.** {'; '.join(capsule['verification'])}\n"
        f"10. **Compatibility, rollback, and retention.** Existing Issue lane, "
        f"activation, and lifecycle behavior unchanged; revert only this "
        f"packet's code/docs; retain ledger closeout and routing receipts; no "
        f"schema migration or new persistence owner.\n"
        f"11. **Exit artifact.** {capsule['expected_artifacts'][0]}\n"
        f"12. **Next action.** After acceptance, the route driver promotes "
        f"exactly one eligible successor against refreshed accepted main; do "
        f"not activate successor execution, EFFECT execution, automatic T3 "
        f"handling, or PREFLIGHT from this packet.\n\n"
        f"### 11. Weak-Agent Dispatch Capsule\n\n"
        f"<!-- weak-agent-dispatch:v1\n{capsule_json}\n-->\n"
    )


def _completed_block(
    closed_packet_id: str, predecessor_evidence: str
) -> str:
    return (
        f"## Completed {closed_packet_id} ({closed_packet_id})\n\n"
        f"**Historical state:** `COMPLETE`\n\n"
        f"**Historical evidence:** {predecessor_evidence}.\n"
    )


def _replace_active_routing(document: str, successor_id: str) -> str:
    routed = list(plan_lane.ACTIVE_ROUTING.finditer(document))
    if not routed:
        raise RouteDriverError("route_active_routing_missing")
    first = routed[0]
    line_end = document.find("\n", first.end())
    if line_end == -1:
        line_end = len(document)
    return (
        document[: first.start()]
        + f"1. `{successor_id}` — `READY_FOR_EXECUTION`"
        + document[line_end:]
    )


def _replace_packet_block(document: str, packet_id: str, replacement: str) -> str:
    headings = list(plan_lane.PACKET_HEADING.finditer(document))
    match = next(
        (heading for heading in headings if heading.group("packet") == packet_id),
        None,
    )
    if match is None:
        raise RouteDriverError("route_closed_packet_block_missing")
    end = _sketch_end(document, match)
    return document[: match.start()] + replacement + document[end:]


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
    closed_packet_id: str, successor_id: str, predecessor_evidence: str
) -> tuple[str, str]:
    closed_row = (
        f"| {closed_packet_id} | `COMPLETE` | "
        f"{predecessor_evidence} |\n"
    )
    successor_row = (
        f"| {successor_id} | `READY_FOR_EXECUTION` | "
        f"Compiled by the route driver after {predecessor_evidence} |\n"
    )
    return closed_row, successor_row


def _with_status_rows(status_document: str, closed_row: str, successor_row: str) -> str:
    marker = "| Repository-maintenance route contract"
    if marker not in status_document:
        raise RouteDriverError("route_status_readiness_table_missing")
    return status_document.replace(
        marker,
        closed_row.rstrip("\n") + "\n" + successor_row.rstrip("\n") + "\n" + marker,
        1,
    )


def compile_successor(
    future_document: str,
    next_document: str,
    status_document: str,
    closed_packet_id: str,
    predecessor_evidence: str,
    accepted_main_sha: str,
) -> CompiledSuccessor:
    """Compile exactly one eligible successor into replace-only document updates.

    The closed packet's block becomes a ``Completed`` block and the compiled
    successor packet (twelve-field contract + capsule) takes its place; the
    promoted sketch is removed from ``docs/FUTURE_ROUTE.md`` and its inventory
    manifest is refreshed; the successor's readiness row is added to
    ``docs/CURRENT_STATUS.md``.  Every derived field is bounded and the
    compiled NEXT_DECISION stays within its byte budget; any violation fails
    closed so a promotion PR is never opened on an unbounded contract.
    """

    successor = eligible_successor(
        future_document, next_document, closed_packet_id,
        completed_ids=_accepted_completed_ids(status_document),
    )
    if not isinstance(accepted_main_sha, str) or not plan_lane.SHA40.fullmatch(accepted_main_sha):
        raise RouteDriverError("route_accepted_main_invalid")
    if not isinstance(predecessor_evidence, str) or not predecessor_evidence.strip():
        raise RouteDriverError("route_predecessor_evidence_missing")
    manifest = inventory_manifest(future_document)
    manifest_sha256 = _json_sha256(manifest)
    capsule = _compile_capsule(successor, accepted_main_sha)
    spec_digest = _canonical_spec_digest(capsule, accepted_main_sha)
    packet_block = _packet_block(
        successor, capsule, accepted_main_sha, predecessor_evidence, manifest_sha256
    )
    completed_block = _completed_block(closed_packet_id, predecessor_evidence)
    next_document = _replace_packet_block(
        next_document, closed_packet_id, completed_block + packet_block
    )
    next_document = _replace_active_routing(next_document, successor.packet_id)
    if len(next_document.encode("utf-8")) > NEXT_DECISION_MAX_BYTES:
        raise RouteDriverError("route_compiled_next_document_too_large")
    future_document = _refresh_future_document(future_document, successor.packet_id)
    closed_row, successor_row = _status_readiness_rows(
        closed_packet_id, successor.packet_id, predecessor_evidence
    )
    status_document = _with_status_rows(
        status_document, closed_row, successor_row
    )
    return CompiledSuccessor(
        packet_id=successor.packet_id,
        capsule=capsule,
        spec_digest=spec_digest,
        manifest_sha256=manifest_sha256,
        branch=f"agent/packet-{successor.packet_id.lower()}",
        next_document=next_document,
        future_document=future_document,
        status_document=status_document,
    )
