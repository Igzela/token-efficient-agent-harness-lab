#!/usr/bin/env python3
"""Validate canonical navigation, handoff, and active-document contracts."""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]

REQUIRED_TEXT = {
    "START_HERE.md": [
        "# Start Here",
        "## Quality Order",
        "## Source-of-Truth Hierarchy",
        "## Establish the Leading Valid Frontier",
        "## Role Routes",
        "scripts/project_context.py",
        "## Automation Boundary",
        "## End-of-Work Handoff",
        "## Documentation Discipline",
    ],
    "AGENTS.md": [
        "Read `START_HERE.md` first",
        "## Quality and Frontier Rule",
        "leading valid frontier",
        "Autonomous Operating Model",
        "Execution-Ready Task Packets",
        "READY_FOR_EXECUTION",
        "DECISION_REQUIRED",
        "Full Agent Autonomy Mode",
        "Autonomous Advancement Loop",
        "Documentation Maintenance Rule",
        "resolve bounded design gaps",
        "do not commit real secrets",
        "do not falsify test or CI evidence",
        "do not intentionally hide failures",
        "do not remove rollback paths without a tested replacement",
        "do not perform irreversible external destruction without a recovery path",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "CLAUDE.md": [
        "# Claude Code Adapter",
        "START_HERE.md",
        "AGENTS.md",
        "scripts/project_context.py",
        "docs/CURRENT_STATUS.md",
        "docs/NEXT_DECISION.md",
    ],
    "README.md": [
        "START_HERE.md",
        "docs/CURRENT_STATUS.md",
        "AGENTS.md",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "docs/ARCHITECTURE_BOOK.md": [
        "# Architecture Book",
        "Current version: v",
        "Product Boundary",
        "Dashboard Boundary",
    ],
    "docs/CURRENT_STATUS.md": [
        "# Current Status",
        "Last updated:",
        "## Verified Repository State",
        "## Capability Status",
        "## Confirmed Integration Gaps",
        "## Maintenance Boundary",
    ],
    "docs/NEXT_DECISION.md": [
        "# Next Decision",
        "## Current Direction",
        "## Active Routing",
        "## Common Execution Protocol",
        "READY_FOR_EXECUTION",
        "DECISION_REQUIRED",
        "Hard Stops",
    ],
    "docs/FUTURE_ROUTE.md": [
        "# Future Route",
        "routing-only",
        "BLOCKED_PREREQUISITE",
        "docs/NEXT_DECISION.md",
        "DECISION_REQUIRED",
    ],
    "docs/MODULE_MAP.md": [
        "# Module Map",
        "## Core Ownership",
        "`scripts/check_wire_codegen_drift.sh`",
    ],
    "docs/REAL_WORLD_TESTING_PLAYBOOK.md": [
        "# Real-World Testing Playbook",
        "Action Permission Matrix",
        "New architecture/authority/recovery decision",
        "docs/archive/",
        "## Review Convergence Protocol",
        "MAX_SUBSTANTIVE_REVIEW_ROUNDS",
        "MAX_AUTONOMOUS_REPAIR_BATCHES",
        "DECISION_REQUIRED",
    ],
    "docs/RUNBOOK.md": [
        "# Agent Control Plane",
        "Operator procedures",
    ],
    "scripts/project_context.py": [
        "project_context.v1",
        "accepted_baseline",
        "canonical_document_source",
        "active_frontier",
        "missing_required",
        "exact_head_review",
        "next_permitted_action",
        "--offline",
        "review_state_projection",
    ],
    "scripts/agent-control/review_convergence.py": [
        "REVIEW_PROTOCOL_VERSION",
        "MAX_SUBSTANTIVE_REVIEW_ROUNDS = 2",
        "MAX_AUTONOMOUS_REPAIR_BATCHES = 1",
        "MAX_DEFERRED_NOTES",
        "MAX_NOTE_LEN",
        "class ReviewDecision",
        "class ReviewFinding",
        "def apply_r2_decision",
        "def derive_next_review_attempt",
        "def project_capsule_fields",
    ],
    "scripts/agent-control/validate_review.py": [
        "review_convergence",
        "convergence_cross_field_invalid",
    ],
    "START_HERE.md": [
        "review_protocol_version",
        "review_round",
    ],
    "scripts/verify_rust_typescript_stack.sh": [
        "bash scripts/check_wire_codegen_drift.sh",
    ],
    ".github/workflows/tests.yml": [
        "run: bash scripts/check_wire_codegen_drift.sh",
    ],
}

MODEL_AGNOSTIC_FILES = [
    "START_HERE.md",
    "AGENTS.md",
    "docs/CURRENT_STATUS.md",
    "docs/NEXT_DECISION.md",
    "docs/FUTURE_ROUTE.md",
    "docs/MODULE_MAP.md",
    "docs/REAL_WORLD_TESTING_PLAYBOOK.md",
]

FORBIDDEN_MODEL_LOCK_MARKERS = [
    "gpt-5.6-terra",
    "READY_FOR_TERRA",
    "model_profile_mismatch",
    "Mandatory Codex Execution Profile",
    "Mandatory Executor Profile",
]

FORBIDDEN_ADAPTER_HEADINGS = {
    "CLAUDE.md": [
        "## Current State",
        "## Authority and Safety",
        "## Autonomous Advancement Protocol",
        "## Documentation Maintenance",
        "## Test Strategy",
    ],
}

PACKET_ID_PATTERN = r"(?:PE\d+|PR\d+|TOOL|CI|PRODUCT)(?:-[A-Z0-9]+)+"
PACKET_HEADING_RE = re.compile(
    rf"^#{{2,3}} Packet (?P<packet>{PACKET_ID_PATTERN})\b.*$", re.MULTILINE
)
PACKET_STATE_RE = re.compile(
    r"^\*\*State:\*\* `(?P<state>[A-Z_]+)`(?:[ \t]+.*)?$", re.MULTILINE
)
STAGE_ROW_RE = re.compile(
    r"^\|\s*(?P<stage>PE-\d+)\s*\|[^|]*\|[^|]*\|\s*(?P<summary>[^|]+?)\s*\|$",
    re.MULTILINE,
)
VALID_PACKET_STATES = {
    "READY_FOR_EXECUTION",
    "BLOCKED_PREREQUISITE",
    "DECISION_REQUIRED",
    "IN_PROGRESS",
    "COMPLETE",
}

ACCEPTED_PACKET_RECEIPT_RE = re.compile(
    rf"^\|\s*`?(?P<packet>{PACKET_ID_PATTERN})`?\s*\|\s*`?COMPLETE`?\s*\|",
    re.MULTILINE,
)
FUTURE_ROUTE_REQUIRED_SECTIONS = (
    "## Weak-Agent Full-Course Contract",
    "## Worker Tiers",
    "## Cheap-Agent Dispatch Protocol",
    "## Known Planned-Seam Gaps",
    "## Promotion Contract",
    "## Stop and Resume Protocol",
    "## Execution Profile Field Contract",
)
FUTURE_PACKET_PROFILE_FIELDS = (
    "Execution profile",
    "Worker tier",
    "Owner/seam",
    "Allowed paths at promotion",
    "Ordered work",
    "Verification",
    "Rollback/recovery",
    "Human/effect gate",
    "Consolidation boundary",
    "Negative-result route",
)


def read(relative_path: str) -> str:
    path = ROOT / relative_path
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def run_guard(command: list[str], label: str, failures: list[str]) -> None:
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    if result.returncode != 0:
        output = (result.stdout or result.stderr).strip()
        failures.append(f"{label} failed: {output}")


def check_required_text(failures: list[str]) -> None:
    for relative_path, snippets in REQUIRED_TEXT.items():
        path = ROOT / relative_path
        if not path.exists():
            failures.append(f"missing required handoff file: {relative_path}")
            continue
        text = read(relative_path)
        for snippet in snippets:
            if snippet not in text:
                failures.append(f"{relative_path} is missing required text: {snippet!r}")


def check_entrypoint_roles(failures: list[str]) -> None:
    start = read("START_HERE.md")
    if re.search(r"\bPR #\d+\b", start) or "Last updated:" in start:
        failures.append(
            "START_HERE.md must remain stable navigation and must not own current PR/status facts"
        )

    for relative_path, headings in FORBIDDEN_ADAPTER_HEADINGS.items():
        text = read(relative_path)
        for heading in headings:
            if heading in text:
                failures.append(
                    f"{relative_path} duplicates canonical policy section {heading!r}"
                )

    canonical_paths = [
        "docs/CURRENT_STATUS.md",
        "docs/NEXT_DECISION.md",
        "docs/FUTURE_ROUTE.md",
        "docs/MODULE_MAP.md",
        "docs/ARCHITECTURE_BOOK.md",
        "docs/REAL_WORLD_TESTING_PLAYBOOK.md",
        "docs/RUNBOOK.md",
        "AGENTS.md",
        "README.md",
        "CLAUDE.md",
    ]
    for relative_path in canonical_paths:
        if f"`{relative_path}`" not in start:
            failures.append(f"START_HERE.md does not route to {relative_path}")
        if not (ROOT / relative_path).exists():
            failures.append(f"START_HERE.md routes to missing path {relative_path}")


def check_model_agnostic_governance(failures: list[str]) -> None:
    for relative_path in MODEL_AGNOSTIC_FILES:
        text = read(relative_path)
        for marker in FORBIDDEN_MODEL_LOCK_MARKERS:
            if marker in text:
                failures.append(
                    f"{relative_path} must remain model-agnostic; found stale marker {marker!r}"
                )


def check_schema_document_drift(failures: list[str]) -> None:
    schema = read("engine/src/storage/local_product_store/schema.rs")
    migrations = read("engine/src/storage/local_product_store/migrations.rs")
    architecture = read("docs/ARCHITECTURE_BOOK.md")
    version = re.search(r"CURRENT_SQLITE_SCHEMA_VERSION\s*:\s*i64\s*=\s*(\d+)", schema)
    if not version:
        failures.append("Cannot parse CURRENT_SQLITE_SCHEMA_VERSION from schema.rs")
        return
    if "CURRENT_SCHEMA_VERSION" not in migrations:
        failures.append("migrations.rs is missing CURRENT_SCHEMA_VERSION constant")
    documented = re.search(r"Current version:\s*v(\d+)", architecture)
    if not documented:
        failures.append("ARCHITECTURE_BOOK.md is missing 'Current version: vN'")
    elif documented.group(1) != version.group(1):
        failures.append(
            f"Schema version mismatch: schema.rs has v{version.group(1)}, "
            f"ARCHITECTURE_BOOK.md has v{documented.group(1)}"
        )


def section(text: str, heading: str) -> str:
    start = text.find(heading)
    if start < 0:
        return ""
    start += len(heading)
    end = text.find("\n## ", start)
    return text[start:] if end < 0 else text[start:end]


def _packet_stage(packet_id: str) -> str:
    match = re.match(r"PE(\d+)-", packet_id)
    return f"PE-{match.group(1)}" if match else ""


def parse_packet_contracts(
    text: str, failures: list[str]
) -> dict[str, dict[str, object]]:
    headings = list(PACKET_HEADING_RE.finditer(text))
    packets: dict[str, dict[str, object]] = {}
    for index, match in enumerate(headings):
        packet = match.group("packet")
        end = headings[index + 1].start() if index + 1 < len(headings) else len(text)
        block = text[match.start() : end]
        states = PACKET_STATE_RE.findall(block)
        if len(states) != 1:
            failures.append(
                f"{packet} must have exactly one structural State field; found {states}"
            )
            continue
        if packet in packets:
            failures.append(f"{packet} is represented more than once")
            continue
        prerequisite = re.search(
            r"^\*\*Prerequisite:\*\* (?P<value>.+)$", block, re.MULTILINE
        )
        packets[packet] = {
            "state": states[0],
            "prerequisites": (
                re.findall(PACKET_ID_PATTERN, prerequisite.group("value"))
                if prerequisite
                else []
            ),
        }
    return packets


def accepted_packet_receipts(status_text: str) -> set[str]:
    """Return packet identities whose accepted completion is durable status truth."""

    return {
        match.group("packet")
        for match in ACCEPTED_PACKET_RECEIPT_RE.finditer(status_text)
    }


def future_route_contract_failures(future_text: str) -> list[str]:
    """Validate the weak-agent execution dossier attached to every future packet."""

    failures: list[str] = []
    for heading in FUTURE_ROUTE_REQUIRED_SECTIONS:
        if heading not in future_text:
            failures.append(f"FUTURE_ROUTE is missing future-route section {heading!r}")

    headings = list(PACKET_HEADING_RE.finditer(future_text))
    seen_profiles: dict[str, str] = {}
    placeholder_values = {"TBD", "TODO", "FIXME", "UNKNOWN", "N/A", "TO BE DETERMINED"}
    for index, match in enumerate(headings):
        packet_id = match.group("packet")
        end = headings[index + 1].start() if index + 1 < len(headings) else len(future_text)
        block = future_text[match.start() : end]
        for label in FUTURE_PACKET_PROFILE_FIELDS:
            values = re.findall(
                rf"^\*\*{re.escape(label)}:\*\*\s*(?P<value>\S.*)$",
                block,
                re.MULTILINE,
            )
            if not values:
                failures.append(f"{packet_id} is missing {label}")
            elif len(values) != 1:
                failures.append(f"{packet_id} must have exactly one {label} field")
            else:
                value = values[0].strip()
                normalized = value.strip("` .").upper()
                if label not in {"Execution profile", "Worker tier"} and (
                    normalized in placeholder_values
                ):
                    failures.append(f"{packet_id} has placeholder {label}: {value!r}")
                if label == "Execution profile":
                    profile_id = value.strip("`")
                    if profile_id in seen_profiles:
                        failures.append(
                            f"duplicate Execution profile {profile_id!r}: "
                            f"{seen_profiles[profile_id]} and {packet_id}"
                        )
                    else:
                        seen_profiles[profile_id] = packet_id

        packet_class = re.search(
            r"^\*\*Class:\*\*\s*`?(?P<value>[A-Z]+)`?\s*$",
            block,
            re.MULTILINE,
        )
        tier = re.search(
            r"^\*\*Worker tier:\*\*\s*`?(?P<value>T[0-3])`?\b",
            block,
            re.MULTILINE,
        )
        if not packet_class:
            failures.append(f"{packet_id} is missing or has invalid Class")
        elif packet_class.group("value") not in {
            "CONTRACT",
            "IMPLEMENT",
            "EFFECT",
            "CLOSEOUT",
        }:
            failures.append(f"{packet_id} has unsupported Class")
        if not tier:
            failures.append(f"{packet_id} is missing or has invalid Worker tier")
        if tier and packet_class and packet_class.group("value") == "EFFECT":
            if tier.group("value") != "T3":
                failures.append(f"{packet_id} EFFECT work must use Worker tier T3")
    return failures


def _packet_dependency_cycle(
    packets: dict[str, dict[str, object]],
) -> list[str] | None:
    graph = {
        packet_id: [
            prerequisite
            for prerequisite in packet["prerequisites"]
            if prerequisite in packets
        ]
        for packet_id, packet in packets.items()
    }
    visiting: set[str] = set()
    visited: set[str] = set()
    path: list[str] = []

    def visit(packet_id: str) -> list[str] | None:
        if packet_id in visiting:
            start = path.index(packet_id)
            return path[start:] + [packet_id]
        if packet_id in visited:
            return None
        visiting.add(packet_id)
        path.append(packet_id)
        for prerequisite in graph[packet_id]:
            cycle = visit(prerequisite)
            if cycle:
                return cycle
        path.pop()
        visiting.remove(packet_id)
        visited.add(packet_id)
        return None

    for packet_id in sorted(graph):
        cycle = visit(packet_id)
        if cycle:
            return cycle
    return None


def active_state_failures(
    status_text: str, next_text: str, future_text: str = ""
) -> list[str]:
    failures: list[str] = []
    current_packets = parse_packet_contracts(next_text, failures)
    future_packets = parse_packet_contracts(future_text, failures)
    duplicate_packets = sorted(set(current_packets) & set(future_packets))
    for packet_id in duplicate_packets:
        failures.append(
            f"{packet_id} is duplicated between NEXT_DECISION and FUTURE_ROUTE"
        )
    packets = {**future_packets, **current_packets}
    accepted_packets = accepted_packet_receipts(status_text)

    for packet_id in sorted(accepted_packets & set(packets)):
        if packets[packet_id]["state"] != "COMPLETE":
            failures.append(
                f"{packet_id} is COMPLETE in accepted receipts but active as "
                f"{packets[packet_id]['state']}"
            )

    if future_text:
        failures.extend(future_route_contract_failures(future_text))
        if len(current_packets) != 1:
            failures.append(
                "NEXT_DECISION must contain exactly one expanded current packet; "
                f"found {len(current_packets)}"
            )
        for packet_id, packet in future_packets.items():
            if packet["state"] != "BLOCKED_PREREQUISITE":
                failures.append(
                    f"FUTURE_ROUTE packet {packet_id} must remain BLOCKED_PREREQUISITE"
                )
        for packet_id, packet in future_packets.items():
            unknown = [
                prerequisite
                for prerequisite in packet["prerequisites"]
                if prerequisite not in packets and prerequisite not in accepted_packets
            ]
            if unknown:
                failures.append(
                    f"{packet_id} references unknown prerequisites: {unknown}"
                )
        cycle = _packet_dependency_cycle(packets)
        if cycle:
            failures.append("packet dependency cycle: " + " -> ".join(cycle))

    for packet_id, packet in current_packets.items():
        state = str(packet["state"])
        if state not in VALID_PACKET_STATES:
            failures.append(f"{packet_id} has unknown state {state!r}")
        if state in {"READY_FOR_EXECUTION", "IN_PROGRESS"}:
            incomplete = [
                prerequisite
                for prerequisite in packet["prerequisites"]
                if prerequisite not in accepted_packets
                and (
                    prerequisite not in packets
                    or packets[prerequisite]["state"] != "COMPLETE"
                )
            ]
            if incomplete:
                failures.append(
                    f"{packet_id} is {state} while prerequisites are not complete: {incomplete}"
                )

    routing = section(next_text, "## Active Routing") or section(
        next_text, "## Current Direction"
    )
    routed_packets = re.findall(PACKET_ID_PATTERN, routing)
    terminal_routing = bool(re.search(r"\bterminal objective\b", routing, re.IGNORECASE))
    if not routed_packets and not terminal_routing:
        failures.append("Active Routing must name at least one packet")
    for packet_id in routed_packets:
        if packet_id not in packets:
            failures.append(f"Active Routing references unknown packet {packet_id}")
        elif packet_id not in current_packets:
            failures.append(
                f"Active Routing references routing-only FUTURE_ROUTE packet {packet_id}"
            )
        elif packets[packet_id]["state"] == "COMPLETE" and not terminal_routing:
            failures.append(f"Active Routing points to completed packet {packet_id}")
    if terminal_routing:
        incomplete = [
            packet_id
            for packet_id, packet in packets.items()
            if packet["state"] != "COMPLETE"
        ]
        if incomplete:
            failures.append(
                "terminal objective routing requires every packet to be complete: "
                + ",".join(sorted(incomplete))
            )
    if routed_packets and routed_packets[0] in packets:
        first = packets[routed_packets[0]]
        incomplete = [
            prerequisite
            for prerequisite in first["prerequisites"]
            if prerequisite not in accepted_packets
            and (
                prerequisite not in packets
                or packets[prerequisite]["state"] != "COMPLETE"
            )
        ]
        if incomplete:
            failures.append(
                f"next routed packet {routed_packets[0]} has incomplete prerequisites: {incomplete}"
            )

    next_stages = {
        match.group("stage"): match.group("summary").strip()
        for match in STAGE_ROW_RE.finditer(next_text)
    }
    status_stages = {
        match.group("stage"): match.group("summary").strip()
        for match in STAGE_ROW_RE.finditer(status_text)
    }
    packet_states: dict[str, list[str]] = {}
    for packet_id, packet in current_packets.items():
        packet_states.setdefault(_packet_stage(packet_id), []).append(str(packet["state"]))

    for stage, summary in next_stages.items():
        states = packet_states.get(stage, [])
        lowered = summary.lower()
        if "complete" in lowered and any(state != "COMPLETE" for state in states):
            failures.append(f"{stage} summary says complete while packet states are {states}")
        if "in progress" in lowered and "IN_PROGRESS" not in states:
            failures.append(f"{stage} summary says in progress but no packet is IN_PROGRESS")
        if "not started" in lowered and any(
            state in {"IN_PROGRESS", "COMPLETE"} for state in states
        ):
            failures.append(
                f"{stage} summary says not started while packet states are {states}"
            )

    for stage in sorted(set(next_stages) & set(status_stages)):
        next_complete = "complete" in next_stages[stage].lower()
        status_complete = "complete" in status_stages[stage].lower()
        if next_complete != status_complete:
            failures.append(
                f"{stage} completion summary contradicts between CURRENT_STATUS and NEXT_DECISION"
            )

    active_status = section(status_text, "## Active Tracks") + section(
        status_text, "## Current Gaps"
    )
    for stage, summary in status_stages.items():
        if "complete" not in summary.lower():
            continue
        if re.search(
            rf"{re.escape(stage)}[^\n]*(pending|next|in progress|not yet|has not)",
            active_status,
            re.IGNORECASE,
        ):
            failures.append(
                f"{stage} is complete in the stage table but still described as pending in active status"
            )

    return failures


def check_active_state_consistency(failures: list[str]) -> None:
    status = read("docs/CURRENT_STATUS.md")
    next_text = read("docs/NEXT_DECISION.md")
    future_text = read("docs/FUTURE_ROUTE.md")
    failures.extend(active_state_failures(status, next_text, future_text))
    if "## Verified Repository State" not in status:
        failures.append("CURRENT_STATUS must preserve the accepted/open/blocked fact boundary")


def check_project_context(failures: list[str]) -> None:
    script = ROOT / "scripts" / "project_context.py"
    spec = importlib.util.spec_from_file_location("project_context_handoff_check", script)
    if spec is None or spec.loader is None:
        failures.append("cannot import scripts/project_context.py")
        return
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    try:
        spec.loader.exec_module(module)
        payload = module.build_capsule(
            offline=True,
            repository="Igzela/token-efficient-agent-harness-lab",
        )
    except Exception as error:
        failures.append(f"offline project context generation failed: {error}")
        return
    for field in [
        "schema_version",
        "accepted_baseline",
        "canonical_document_source",
        "local_checkout",
        "active_packet",
        "active_frontier",
        "next_permitted_action",
        "required_reading",
        "hard_stops",
    ]:
        if field not in payload:
            failures.append(f"offline project context is missing {field}")
    baseline = payload.get("accepted_baseline", {})
    if baseline.get("availability") not in {"confirmed", "local_only", "unavailable"}:
        failures.append("project context baseline availability is invalid")
    binding = payload.get("binding", {})
    projection = binding.get("review_state_projection")
    if not isinstance(projection, dict):
        failures.append("project context binding is missing review_state_projection")
    else:
        required_projection = {
            "review_protocol_version",
            "review_mode",
            "review_round",
            "prior_reviewed_head",
            "reviewed_head",
            "finding_ledger_digest",
            "open_blocker_ids",
            "deferred_note_ids",
            "autonomous_repairs_remaining",
            "stop_reason",
            "review_state",
        }
        missing = sorted(required_projection - set(projection))
        if missing:
            failures.append(
                f"review_state_projection is missing fields: {missing}"
            )
        for forbidden in ("severity", "findings", "acceptance_condition", "disposition"):
            if forbidden in projection:
                failures.append(
                    f"capsule review_state_projection must not project {forbidden!r}"
                )


def main() -> int:
    failures: list[str] = []
    check_required_text(failures)
    check_entrypoint_roles(failures)
    check_model_agnostic_governance(failures)
    check_schema_document_drift(failures)
    check_active_state_consistency(failures)
    check_project_context(failures)

    wire_guard = ROOT / "scripts" / "check_wire_codegen_drift.sh"
    if not wire_guard.is_file() or not os.access(wire_guard, os.X_OK):
        failures.append("missing or non-executable wire codegen drift guard")
    else:
        run_guard(["bash", str(wire_guard)], "wire codegen drift guard", failures)

    toolchain_guard = ROOT / "scripts" / "check_toolchain_drift.sh"
    if toolchain_guard.exists():
        run_guard(["bash", str(toolchain_guard)], "toolchain drift guard", failures)

    secret_scan = ROOT / "scripts" / "acp_secret_scan.py"
    if not secret_scan.is_file():
        failures.append("missing secret scan")
    else:
        secret_result = subprocess.run(
            [sys.executable, str(secret_scan)],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        if secret_result.returncode != 0:
            print("Agent handoff check FAILED — secret scan:")
            print((secret_result.stdout or secret_result.stderr).strip())
            return 1

    if failures:
        print("Agent handoff check FAILED:")
        for failure in failures:
            print(f"- {failure}")
        return 1

    print("Agent handoff check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
