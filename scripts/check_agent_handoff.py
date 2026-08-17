#!/usr/bin/env python3
"""Validate canonical navigation, handoff, and active-document contracts."""

from __future__ import annotations

import dataclasses
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]

NEXT_DECISION_MAX_BYTES = 64 * 1024
NEXT_DECISION_MAX_LINES = 600
NEXT_DECISION_APPEND_ONLY_HEADING_RE = re.compile(
    r"^#{2,6}\s+(?:change(?:log| history)|progress log|session notes|"
    r"handoff history|work log|status history)\s*$",
    re.IGNORECASE | re.MULTILINE,
)

REQUIRED_TEXT = {
    "START_HERE.md": [
        "# Start Here",
        "## Quality Order",
        "## Source-of-Truth Hierarchy",
        "## Establish the Leading Valid Frontier",
        "## One-Command Session Bootstrap",
        "## Role Routes",
        "agent-context-routes:v1",
        "scripts/project_context.py",
        "scripts/session_context.py",
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
    "scripts/session_context.py": [
        "agent_context_routes.v1",
        "agent_session_handoff.v1",
        "def parse_route_contract",
        "def extract_packet",
        "def _build_checkpoint",
        "def classify_resume",
        "DECISION_REQUIRED",
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
    "scripts/session_context.py",
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
HISTORICAL_PACKET_RE = re.compile(
    rf"^## Retained .*?\(historical:\s*(?P<packet>{PACKET_ID_PATTERN})\)\s*$",
    re.MULTILINE,
)
HISTORICAL_PACKET_STATE_RE = re.compile(
    r"^\*\*Historical state:\*\* `BLOCKED_PREREQUISITE`\s*$", re.MULTILINE
)
HISTORICAL_PACKET_SOURCE_RE = re.compile(
    r"^\*\*Historical source:\*\*.*(?<![0-9a-f])"
    r"(?P<source>[0-9a-f]{40})(?![0-9a-f]).*$",
    re.MULTILINE | re.IGNORECASE,
)
ROUTED_PACKET_STATE_RE = re.compile(
    rf"`(?P<packet>{PACKET_ID_PATTERN})`\s*(?:\u2014|-)\s*"
    r"`(?P<state>[A-Z0-9_]+)`",
    re.MULTILINE,
)
PACKET_STATE_RE = re.compile(
    r"^\*\*State:\*\* `(?P<state>[A-Z0-9_]+)`(?:[ \t]+.*)?$", re.MULTILINE
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
    "T3_REQUIRED",
}

ACCEPTED_PACKET_RECEIPT_RE = re.compile(
    rf"^\|\s*`?(?P<packet>{PACKET_ID_PATTERN})`?\s*\|\s*`?COMPLETE`?\s*\|"
    r"\s*(?P<evidence>[^|]+?)\s*\|\s*$",
    re.MULTILINE,
)
FUTURE_ROUTE_INVENTORY_RE = re.compile(
    r"<!-- future-route-inventory:v1\s*(?P<payload>\{.*?\})\s*-->", re.DOTALL
)
WEAK_AGENT_DISPATCH_RE = re.compile(
    r"<!-- weak-agent-dispatch:v1\s*(?P<payload>\{.*?\})\s*-->", re.DOTALL
)
ROUTE_BOOTSTRAP_RECONCILE_RE = re.compile(
    rf"<!-- route-bootstrap-reconcile:v1 packet_id=(?P<packet>{PACKET_ID_PATTERN}) -->"
)
FUTURE_ROUTE_REQUIRED_SECTIONS = (
    "## Worker Tiers",
    "## Known Planned-Seam Gaps",
    "## Promotion Profile Contract",
    "## Stop and Resume Protocol",
    "## Portfolio Inventory Manifest",
)
FUTURE_PACKET_BASE_FIELDS = (
    "Prerequisite",
    "Class",
    "Outcome",
    "Allowed delta",
    "Exit",
    "Stop",
)
WEAK_AGENT_DISPATCH_LIST_FIELDS = (
    "allowed_paths",
    "read_paths",
    "allowed_outputs",
    "prerequisites",
    "prerequisite_receipts",
    "forbidden_changes",
    "ordered_steps",
    "verification",
    "pause_gates",
    "expected_artifacts",
    "forbidden_next_actions",
)


def _dispatch_scope_paths(value: object) -> list[str] | None:
    if not isinstance(value, list) or not value or len(value) > 50:
        return None
    normalized: list[str] = []
    for item in value:
        if (
            not isinstance(item, str)
            or not item
            or any(character.isspace() for character in item)
            or "\x00" in item
        ):
            return None
        directory = item.endswith("/")
        candidate = item[:-1] if directory else item
        path = PurePosixPath(candidate)
        if (
            not candidate
            or path.is_absolute()
            or any(part in {"", ".", ".."} for part in path.parts)
            or str(path) != candidate
        ):
            return None
        normalized.append(candidate + ("/" if directory else ""))
    if len(normalized) != len(set(normalized)):
        return None
    return normalized
PROFILE_WORKER_TIERS = frozenset({"T0", "T1", "T2", "T3"})
PROFILE_RISK_CLASSES = frozenset(
    {"none", "store_mutation", "authority", "external_effect", "evaluator"}
)
PROFILE_VERIFICATION_FAMILIES = frozenset(
    {
        "docs_evidence_review",
        "source_focused_full",
        "external_effect_evidence",
        "evidence_review",
        "REFRESH_AT_PROMOTION",
    }
)
PROFILE_PACKET_CLASSES = frozenset({"CONTRACT", "IMPLEMENT", "EFFECT", "CLOSEOUT"})
CLASS_DEFAULT_TIER = {"CONTRACT": "T2", "IMPLEMENT": "T1", "EFFECT": "T3", "CLOSEOUT": "T2"}
CLASS_DEFAULT_RISK = {"EFFECT": "external_effect"}
CLASS_DEFAULT_VERIFICATION = {
    "CONTRACT": "docs_evidence_review",
    "IMPLEMENT": "source_focused_full",
    "EFFECT": "external_effect_evidence",
    "CLOSEOUT": "evidence_review",
}


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
        packet_class = _packet_class(block)
        packets[packet] = {
            "state": states[0],
            "class": packet_class,
            "prerequisites": (
                re.findall(PACKET_ID_PATTERN, prerequisite.group("value"))
                if prerequisite
                else []
            ),
        }
    return packets


REVIEW_RECEIPT_REQUIRED_AXES = frozenset({
    "architecture",
    "authority",
    "compatibility",
    "security",
    "audit",
    "rollback",
    "scope/path binding",
})


def validate_review_receipt_text(
    body: str, expected_head: str | None = None, expected_base: str | None = None
) -> list[str]:
    """Validate a canonical exact-head review receipt format and required fields."""

    failures: list[str] = []
    if "EXACT-HEAD REVIEW RECEIPT" not in body:
        failures.append("review receipt missing EXACT-HEAD REVIEW RECEIPT marker")
        return failures

    def _field(label: str) -> str | None:
        matches = re.findall(rf"(?im)^\s*{re.escape(label)}\s*:\s*(.*?)\s*$", body)
        return matches[0].strip() if len(matches) == 1 else None

    reviewed_sha = _field("Reviewed SHA")
    if not reviewed_sha or not re.fullmatch(r"[0-9a-f]{40}", reviewed_sha):
        failures.append("review receipt missing or invalid Reviewed SHA")
    elif expected_head and reviewed_sha != expected_head:
        failures.append(f"review receipt Reviewed SHA {reviewed_sha} does not match expected {expected_head}")

    reviewed_range = _field("Reviewed range")
    range_match = re.fullmatch(r"([0-9a-f]{40})\.\.\.([0-9a-f]{40})", reviewed_range or "")
    if not range_match:
        failures.append("review receipt missing or invalid Reviewed range")
    else:
        if expected_base and range_match.group(1) != expected_base:
            failures.append(f"review receipt base SHA {range_match.group(1)} does not match expected {expected_base}")
        if expected_head and range_match.group(2) != expected_head:
            failures.append(f"review receipt head SHA {range_match.group(2)} does not match expected {expected_head}")

    reviewer_session = _field("Reviewer session identity")
    if not reviewer_session or reviewer_session.lower() in {"self", "self-review", "unknown", "implementation-agent"}:
        failures.append("review receipt missing or invalid Reviewer session identity")

    reviewer_auth = _field("Reviewer authenticated identity")
    if not reviewer_auth:
        failures.append("review receipt missing Reviewer authenticated identity")

    transport = _field("Review transport")
    if transport not in {"direct-github-reviewer", "parent-posted-on-behalf-of-independent-session"}:
        failures.append(f"review receipt invalid Review transport: {transport!r}")
    elif transport == "parent-posted-on-behalf-of-independent-session":
        impl_session = _field("Implementation session identity")
        if not impl_session:
            failures.append("review receipt parent transport missing Implementation session identity")
        elif reviewer_session and impl_session.lower() == reviewer_session.lower():
            failures.append("review receipt reviewer and implementation sessions must differ")

    axes_raw = _field("Axes")
    if not axes_raw:
        failures.append("review receipt missing Axes")
    else:
        found_axes = {axis.strip().lower() for axis in axes_raw.split(",")}
        missing_axes = REVIEW_RECEIPT_REQUIRED_AXES - found_axes
        if missing_axes:
            failures.append(f"review receipt missing required axes: {sorted(missing_axes)}")

    outcome = (_field("Outcome") or "").upper()
    if outcome != "PASS":
        failures.append(f"review receipt Outcome must be exact PASS, found: {outcome!r}")

    unresolved = (_field("Unresolved objections") or "").lower()
    if unresolved != "none":
        failures.append(f"review receipt Unresolved objections must be 'none', found: {unresolved!r}")

    return failures


def is_doc_path(p: str) -> bool:
    return (
        p.startswith("docs/")
        or p.endswith(".md")
        or p in {"START_HERE.md", "AGENTS.md", "README.md", "CLAUDE.md", "LICENSE"}
    )


def is_test_path(p: str) -> bool:
    norm = p.replace("\\", "/")
    parts = norm.split("/")
    if "tests" in parts or "fixtures" in parts:
        return True
    filename = parts[-1]
    if (
        filename.startswith("test_")
        or filename.endswith("_test.rs")
        or filename.endswith(".test.ts")
        or filename.endswith(".test.js")
        or filename.endswith(".test.mjs")
        or filename.endswith(".spec.ts")
    ):
        return True
    return False


def is_production_source_path(p: str) -> bool:
    return not is_doc_path(p) and not is_test_path(p)


def accepted_packet_receipts(
    status_text: str, failures: list[str] | None = None
) -> set[str]:
    """Return durable completion identities from their sole owning status section."""

    receipt_section = section(status_text, "## Accepted Packet Receipts")
    if not receipt_section:
        return set()
    accepted: set[str] = set()
    for match in ACCEPTED_PACKET_RECEIPT_RE.finditer(receipt_section):
        packet_id = match.group("packet")
        evidence = match.group("evidence")
        if not re.search(
            r"(?<![0-9a-f])(?:[0-9a-f]{64}|[0-9a-f]{40})(?![0-9a-f])",
            evidence,
            re.IGNORECASE,
        ):
            if failures is not None:
                failures.append(
                    f"accepted packet receipt {packet_id} lacks a durable evidence identity"
                )
            continue
        if packet_id in accepted and failures is not None:
            failures.append(f"accepted packet receipt {packet_id} is duplicated")
        accepted.add(packet_id)
    return accepted


def _json_sha256(value: object) -> str:
    encoded = json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _packet_field_values(block: str, label: str) -> list[str]:
    return [
        value.strip()
        for value in re.findall(
            rf"^\*\*{re.escape(label)}:\*\*\s*(?P<value>\S.*)$",
            block,
            re.MULTILINE,
        )
    ]


def _packet_class(block: str) -> str | None:
    match = re.search(
        r"^\*\*Class:\*\*\s*`?(?P<value>[A-Z]+)`?\s*$",
        block,
        re.MULTILINE,
    )
    return match.group("value") if match else None


def _packet_profile_row(block: str, packet_id: str) -> list[object] | None:
    """Derive the canonical manifest row for one packet from its prose fields.

    The row is ``[packet_id, class, worker_tier, risk_class, verification_family]``.
    ``class`` must come from the packet's prose ``Class`` field; tier, risk, and
    verification family are promotion-time candidates validated only against the
    profile vocabularies and the ``EFFECT`` constraint, never treated as prose.
    """

    packet_class = _packet_class(block)
    if packet_class is None or packet_class not in PROFILE_PACKET_CLASSES:
        return None
    return [
        packet_id,
        packet_class,
        CLASS_DEFAULT_TIER[packet_class],
        CLASS_DEFAULT_RISK.get(packet_class, "none"),
        CLASS_DEFAULT_VERIFICATION[packet_class],
    ]


def historical_packet_ids(next_text: str, failures: list[str]) -> set[str]:
    """Return retained packet identities only when their provenance is explicit."""

    headings = list(HISTORICAL_PACKET_RE.finditer(next_text))
    historical: set[str] = set()
    accepted_main_result = subprocess.run(
        ["git", "rev-parse", "origin/main"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    accepted_main_sha = accepted_main_result.stdout.strip()
    if accepted_main_result.returncode != 0 or not re.fullmatch(
        r"[0-9a-f]{40}", accepted_main_sha, re.IGNORECASE
    ):
        failures.append("historical packet provenance requires origin/main")
        return historical
    for index, heading in enumerate(headings):
        next_heading = re.search(r"^## ", next_text[heading.end() :], re.MULTILINE)
        end = (
            heading.end() + next_heading.start()
            if next_heading
            else len(next_text)
        )
        block = next_text[heading.start() : end]
        packet_id = heading.group("packet")
        if packet_id in historical:
            failures.append(f"historical packet {packet_id} is duplicated")
            continue
        if not HISTORICAL_PACKET_STATE_RE.search(block):
            failures.append(
                f"historical packet {packet_id} must declare BLOCKED_PREREQUISITE state"
            )
            continue
        source = HISTORICAL_PACKET_SOURCE_RE.search(block)
        if not source:
            failures.append(
                f"historical packet {packet_id} must bind a 40-character source digest"
            )
            continue
        source_sha = source.group("source")
        if subprocess.run(
            ["git", "cat-file", "-e", f"{source_sha}^{{commit}}"],
            cwd=ROOT,
            capture_output=True,
            check=False,
        ).returncode != 0:
            failures.append(
                f"historical packet {packet_id} source is not a repository commit"
            )
            continue
        if subprocess.run(
            ["git", "merge-base", "--is-ancestor", source_sha, accepted_main_sha],
            cwd=ROOT,
            capture_output=True,
            check=False,
        ).returncode != 0:
            failures.append(
                f"historical packet {packet_id} source is not an ancestor of HEAD"
            )
            continue
        source_document = subprocess.run(
            ["git", "show", f"{source_sha}:docs/NEXT_DECISION.md"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        ).stdout
        if not re.search(
            rf"^#{{2,3}} Packet {re.escape(packet_id)}\b",
            source_document,
            re.MULTILINE,
        ):
            failures.append(
                f"historical packet {packet_id} is absent from its source commit"
            )
            continue
        historical.add(packet_id)
    return historical


def future_route_inventory_payload(future_text: str) -> dict[str, object]:
    """Build the canonical inventory bound by FUTURE_ROUTE's checked manifest."""

    headings = list(PACKET_HEADING_RE.finditer(future_text))
    ordered_packet_ids: list[str] = []
    dependency_graph: list[dict[str, object]] = []
    profile_rows: list[list[object]] = []
    for index, match in enumerate(headings):
        packet_id = match.group("packet")
        end = headings[index + 1].start() if index + 1 < len(headings) else len(future_text)
        block = future_text[match.start() : end]
        prerequisite = re.search(
            r"^\*\*Prerequisite:\*\*\s*(?P<value>.+)$", block, re.MULTILINE
        )
        prerequisites = (
            re.findall(PACKET_ID_PATTERN, prerequisite.group("value"))
            if prerequisite
            else []
        )
        prerequisites = list(
            dict.fromkeys(item for item in prerequisites if item != packet_id)
        )
        ordered_packet_ids.append(packet_id)
        dependency_graph.append(
            {"packet_id": packet_id, "prerequisites": prerequisites}
        )
        row = _packet_profile_row(block, packet_id)
        if row is not None:
            profile_rows.append(row)

    return {
        "schema_version": "future_route_inventory.v1",
        "packet_count": len(ordered_packet_ids),
        "ordered_packet_ids": ordered_packet_ids,
        "ordered_packet_ids_sha256": _json_sha256(ordered_packet_ids),
        "dependency_graph_sha256": _json_sha256(dependency_graph),
        "profiles_sha256": _json_sha256(profile_rows),
        "profiles": profile_rows,
    }


def future_route_profile_failures(future_text: str) -> list[str]:
    """Validate the bounded promotion-profile dossier for every future packet."""

    failures: list[str] = []
    seen_headings: set[str] = set()
    for line in future_text.splitlines():
        if line.startswith("#"):
            heading = line.strip()
            if heading in seen_headings:
                failures.append(f"FUTURE_ROUTE contains duplicate heading {heading!r}")
            seen_headings.add(heading)

    for heading in FUTURE_ROUTE_REQUIRED_SECTIONS:
        if heading not in future_text:
            failures.append(f"FUTURE_ROUTE is missing future-route section {heading!r}")

    headings = list(PACKET_HEADING_RE.finditer(future_text))
    for index, match in enumerate(headings):
        packet_id = match.group("packet")
        end = headings[index + 1].start() if index + 1 < len(headings) else len(future_text)
        block = future_text[match.start() : end]
        for label in FUTURE_PACKET_BASE_FIELDS:
            values = _packet_field_values(block, label)
            if not values:
                failures.append(f"{packet_id} is missing {label}")
            elif len(values) != 1:
                failures.append(f"{packet_id} must have exactly one {label} field")
            else:
                value = values[0].strip()
                normalized = value.strip("` .").upper()
                if label in {"Prerequisite", "Class"} and normalized in {
                    "TBD",
                    "TODO",
                    "FIXME",
                    "UNKNOWN",
                    "N/A",
                    "TO BE DETERMINED",
                }:
                    failures.append(f"{packet_id} has placeholder {label}: {value!r}")
        packet_class = _packet_class(block)
        if packet_class is None:
            failures.append(f"{packet_id} is missing or has invalid Class")
        elif packet_class not in PROFILE_PACKET_CLASSES:
            failures.append(f"{packet_id} has unsupported Class {packet_class!r}")
        states = PACKET_STATE_RE.findall(block)
        if len(states) != 1 or states[0] != "BLOCKED_PREREQUISITE":
            failures.append(f"{packet_id} must remain BLOCKED_PREREQUISITE")
    failures.extend(_future_route_inventory_failures(future_text))
    return failures


def _future_route_inventory_failures(future_text: str) -> list[str]:
    markers = list(FUTURE_ROUTE_INVENTORY_RE.finditer(future_text))
    if not markers:
        return ["FUTURE_ROUTE is missing future-route-inventory:v1 marker"]
    if len(markers) != 1:
        return [
            "FUTURE_ROUTE must contain exactly one future-route-inventory:v1 marker"
        ]
    try:
        observed = json.loads(markers[0].group("payload"))
    except json.JSONDecodeError as exc:
        return [f"FUTURE_ROUTE inventory manifest is invalid JSON: {exc.msg}"]
    if not isinstance(observed, dict):
        return ["FUTURE_ROUTE inventory manifest must be a JSON object"]
    if observed.get("schema_version") != "future_route_inventory.v1":
        return ["FUTURE_ROUTE inventory manifest has wrong schema_version"]

    headings = list(PACKET_HEADING_RE.finditer(future_text))
    ordered_packet_ids: list[str] = []
    dependency_graph: list[dict[str, object]] = []
    for index, match in enumerate(headings):
        packet_id = match.group("packet")
        end = headings[index + 1].start() if index + 1 < len(headings) else len(future_text)
        block = future_text[match.start() : end]
        prerequisite = re.search(
            r"^\*\*Prerequisite:\*\*\s*(?P<value>.+)$", block, re.MULTILINE
        )
        prerequisites = (
            re.findall(PACKET_ID_PATTERN, prerequisite.group("value"))
            if prerequisite
            else []
        )
        prerequisites = list(
            dict.fromkeys(item for item in prerequisites if item != packet_id)
        )
        ordered_packet_ids.append(packet_id)
        dependency_graph.append(
            {"packet_id": packet_id, "prerequisites": prerequisites}
        )

    failures: list[str] = []
    if observed.get("packet_count") != len(ordered_packet_ids):
        failures.append("FUTURE_ROUTE inventory packet_count is stale")
    if observed.get("ordered_packet_ids") != ordered_packet_ids:
        failures.append(
            "FUTURE_ROUTE inventory ordered_packet_ids must equal the prose packet order"
        )
    if observed.get("ordered_packet_ids_sha256") != _json_sha256(ordered_packet_ids):
        failures.append("FUTURE_ROUTE inventory ordered_packet_ids_sha256 is stale")
    if observed.get("dependency_graph_sha256") != _json_sha256(dependency_graph):
        failures.append("FUTURE_ROUTE inventory dependency_graph_sha256 is stale")

    rows = observed.get("profiles")
    if not isinstance(rows, list) or len(rows) != len(ordered_packet_ids):
        failures.append(
            "FUTURE_ROUTE inventory profiles must contain exactly one row per packet"
        )
        return failures
    for index, row in enumerate(rows):
        packet_id = ordered_packet_ids[index]
        if (
            not isinstance(row, list)
            or len(row) != 5
            or row[0] != packet_id
            or not all(isinstance(item, str) for item in row)
        ):
            failures.append(f"FUTURE_ROUTE inventory profile row {index} is malformed")
            continue
        _packet, packet_class, tier, risk, family = row
        if packet_class not in PROFILE_PACKET_CLASSES:
            failures.append(
                f"{packet_id} profile has unsupported Class {packet_class!r}"
            )
        if tier not in PROFILE_WORKER_TIERS:
            failures.append(f"{packet_id} profile has invalid Worker tier {tier!r}")
        if risk not in PROFILE_RISK_CLASSES:
            failures.append(f"{packet_id} profile has invalid risk class {risk!r}")
        if family not in PROFILE_VERIFICATION_FAMILIES:
            failures.append(
                f"{packet_id} profile has invalid verification family {family!r}"
            )
        if packet_class == "EFFECT":
            if tier != "T3":
                failures.append(f"{packet_id} EFFECT profile must use Worker tier T3")
            if risk != "external_effect":
                failures.append(
                    f"{packet_id} EFFECT profile must use risk class external_effect"
                )
    if observed.get("profiles_sha256") != _json_sha256(rows):
        failures.append("FUTURE_ROUTE inventory profiles_sha256 is stale")
    return failures


def weak_agent_dispatch_failures(
    next_text: str, current_packets: dict[str, dict[str, object]]
) -> list[str]:
    """Validate the bounded direct/Issue-lane dispatch capsule for the current packet.

    The capsule is required only while exactly one execution-ready or in-progress
    current packet exists. A planning-parked window (`DECISION_REQUIRED`) carries
    no capsule; requiring one there would force the packet's contract to be
    invented rather than expanded by the planning owner.
    """

    failures: list[str] = []
    executable = {
        packet_id: packet
        for packet_id, packet in current_packets.items()
        if packet["state"] in {"READY_FOR_EXECUTION", "IN_PROGRESS"}
    }
    if not executable:
        return failures
    if len(executable) != 1:
        failures.append("NEXT_DECISION must expose at most one executable current packet")
        return failures
    current_packet_id = next(iter(executable))
    if not re.search(
        r"^### 11\. (?:Bounded Autonomous Worker Dispatch Capsule|Weak-Agent Dispatch Capsule)$",
        next_text,
        re.MULTILINE,
    ):
        failures.append("NEXT_DECISION is missing Weak-Agent Dispatch Capsule section")
    markers = list(WEAK_AGENT_DISPATCH_RE.finditer(next_text))
    if not markers:
        failures.append("NEXT_DECISION is missing weak-agent-dispatch:v1 marker")
        return failures
    if len(markers) != 1:
        failures.append(
            "NEXT_DECISION must contain exactly one weak-agent-dispatch:v1 marker"
        )
        return failures
    try:
        payload = json.loads(markers[0].group("payload"))
    except json.JSONDecodeError as exc:
        failures.append(f"weak-agent dispatch capsule is invalid JSON: {exc.msg}")
        return failures
    if not isinstance(payload, dict):
        failures.append("weak-agent dispatch capsule must be a JSON object")
        return failures

    if payload.get("schema_version") != "weak_agent_dispatch.v1":
        failures.append("weak-agent dispatch capsule has wrong schema_version")
    if payload.get("packet_id") != current_packet_id:
        failures.append(
            f"weak-agent dispatch packet_id must equal {current_packet_id!r}"
        )
    if not isinstance(payload.get("dispatch_lane"), str) or not payload.get(
        "dispatch_lane"
    ):
        failures.append("weak-agent dispatch must declare a bounded dispatch lane")
    if payload.get("plan_lane_state") not in {
        "plan_lane_deferred_until_terminal_owners",
        "plan_lane_active",
    }:
        failures.append(
            "weak-agent dispatch must declare a known plan_lane_state"
        )
    if payload.get("external_effect_limit") != 0:
        failures.append("weak-agent dispatch must set external_effect_limit=0")
    if payload.get("authority_consumption_allowed") is not False:
        failures.append("weak-agent dispatch must forbid authority consumption")
    if payload.get("secret_values_allowed") is not False:
        failures.append("weak-agent dispatch must forbid secret values")
    if payload.get("private_paths_allowed") is not False:
        failures.append("weak-agent dispatch must forbid private paths")

    goal = payload.get("goal")
    rollback = payload.get("rollback")
    if not isinstance(goal, str) or len(goal.strip()) < 20:
        failures.append("weak-agent dispatch goal must be a concrete narrative")
    if not isinstance(rollback, str) or len(rollback.strip()) < 20:
        failures.append("weak-agent dispatch rollback must be a concrete narrative")
    expected_artifacts = payload.get("expected_artifacts")
    if isinstance(expected_artifacts, list):
        for art in expected_artifacts:
            if not isinstance(art, str):
                continue
            art_str = art.strip()
            if art_str in {
                "Canonical route evidence.",
                "A provider-free change.",
                "Canonical route evidence. (docs/NEXT_DECISION.md:canonical)",
                "Canonical route evidence",
            }:
                failures.append(
                    f"weak-agent dispatch expected_artifacts contains generic placeholder {art_str!r}; must be concrete package-specific narrative/paths"
                )
            elif len(art_str) < 15:
                failures.append(
                    f"weak-agent dispatch expected_artifacts {art_str!r} is too short; must be concrete package-specific narrative/paths"
                )
    for field in WEAK_AGENT_DISPATCH_LIST_FIELDS:
        value = payload.get(field)
        if not isinstance(value, list) or not value or not all(
            isinstance(item, str) and item.strip() for item in value
        ):
            failures.append(f"weak-agent dispatch {field} must be a non-empty string list")
    allowed_scope = _dispatch_scope_paths(payload.get("allowed_paths"))
    if allowed_scope is None:
        failures.append(
            "weak-agent dispatch allowed_paths must be safe repository-relative paths"
        )
    read_scope = _dispatch_scope_paths(payload.get("read_paths"))
    if read_scope is None:
        failures.append(
            "weak-agent dispatch read_paths must be safe repository-relative paths"
        )
    elif allowed_scope is not None and not set(allowed_scope).issubset(read_scope):
        failures.append("weak-agent dispatch read_paths must contain allowed_paths")
    if allowed_scope is not None:
        packet_info = current_packets.get(current_packet_id, {})
        packet_cls = packet_info.get("class")
        if packet_cls == "IMPLEMENT":
            prod_paths = [p for p in allowed_scope if is_production_source_path(p)]
            test_paths = [p for p in allowed_scope if is_test_path(p)]
            if not prod_paths:
                if test_paths:
                    failures.append(
                        f"weak-agent dispatch for IMPLEMENT packet {current_packet_id} cannot be satisfied by test-only paths: {allowed_scope}"
                    )
                else:
                    failures.append(
                        f"weak-agent dispatch for IMPLEMENT packet {current_packet_id} allowed_paths must contain production source paths, found only documentation paths: {allowed_scope}"
                    )
            else:
                if current_packet_id.startswith(
                    ("PE7-AC", "PE7-HE", "PE7-CWS", "PE7-MEMORY", "PE7-SKILL", "PE7-RWE")
                ):
                    engine_prod = [
                        p for p in prod_paths
                        if p.startswith("engine/src/") or p == "engine/src" or p.startswith("engine/")
                    ]
                    if not engine_prod:
                        failures.append(
                            f"weak-agent dispatch for product IMPLEMENT packet {current_packet_id} must target engine production source, found unrelated path: {prod_paths}"
                        )
                elif current_packet_id.startswith(
                    ("PE7-ROUTE-", "PE7-PLAN-", "PE7-CTRL-", "TOOL-")
                ):
                    route_prod = [
                        p for p in prod_paths if p.startswith(("scripts/", "tools/"))
                    ]
                    if not route_prod:
                        failures.append(
                            f"weak-agent dispatch for route-control IMPLEMENT packet {current_packet_id} must target route control source, found unrelated path: {prod_paths}"
                        )
    known_store_mutations = payload.get("known_store_mutations")
    if known_store_mutations is not None and (
        not isinstance(known_store_mutations, list)
        or not all(isinstance(item, str) and item.strip() for item in known_store_mutations)
    ):
        failures.append(
            "weak-agent dispatch known_store_mutations must be a string list"
        )
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


FORWARD_ORDER_WINDOW_RE = re.compile(
    r"\[window:\s*(?P<label>[^\]]*?)\s*—\s*(?P<state>[A-Z0-9_]+)\s*,\s*(?P<detail>[^\]]*)\]",
    re.MULTILINE,
)


def forward_order_window_failures(
    next_text: str, current_packets: dict[str, dict[str, object]]
) -> list[str]:
    """The Authoritative Forward Order window projection must not contradict
    the actual current packet state (docs/NEXT_DECISION.md self-conflict guard).

    The guard applies only while exactly one current packet exists; when the
    forward order carries a window projection, its declared state must equal
    the current packet's structural State, and an unparseable projection is a
    fail-closed failure rather than a silently skipped check.
    """

    failures: list[str] = []
    if len(current_packets) != 1:
        return failures
    window_section = section(next_text, "## Authoritative Forward Order")
    if "window:" not in window_section:
        return failures
    projections = list(FORWARD_ORDER_WINDOW_RE.finditer(window_section))
    if not projections:
        failures.append(
            "Authoritative Forward Order window projection is unparseable; "
            "expected [window: <label> — <STATE>, <detail>]"
        )
        return failures
    packet_id = next(iter(current_packets))
    packet_state = str(current_packets[packet_id]["state"])
    for match in projections:
        projected_state = match.group("state")
        if projected_state != packet_state:
            failures.append(
                f"Authoritative Forward Order window projection says "
                f"{projected_state} but current packet {packet_id} is {packet_state}"
            )
    return failures


def active_state_failures(
    status_text: str, next_text: str, future_text: str = ""
) -> list[str]:
    failures: list[str] = []
    current_packets = parse_packet_contracts(next_text, failures)
    future_packets = parse_packet_contracts(future_text, failures)
    historical_packets = historical_packet_ids(next_text, failures)
    collisions = historical_packets & (set(current_packets) | set(future_packets))
    for packet_id in sorted(collisions):
        failures.append(
            f"historical packet {packet_id} collides with a current or future packet"
        )
    duplicate_packets = sorted(set(current_packets) & set(future_packets))
    for packet_id in duplicate_packets:
        failures.append(
            f"{packet_id} is duplicated between NEXT_DECISION and FUTURE_ROUTE"
        )
    packets = {**future_packets, **current_packets}
    accepted_packets = accepted_packet_receipts(status_text, failures)
    failures.extend(forward_order_window_failures(next_text, current_packets))
    bootstrap_packets = ROUTE_BOOTSTRAP_RECONCILE_RE.findall(next_text)
    if len(bootstrap_packets) > 1:
        failures.append("NEXT_DECISION must contain at most one route-bootstrap-reconcile marker")
    elif bootstrap_packets:
        bootstrap_packet = bootstrap_packets[0]
        if (
            bootstrap_packet not in current_packets
            or bootstrap_packet not in accepted_packets
            or current_packets[bootstrap_packet]["state"] != "READY_FOR_EXECUTION"
        ):
            failures.append(
                "route-bootstrap-reconcile marker must name one accepted READY_FOR_EXECUTION current packet"
            )

    for packet_id in sorted(accepted_packets & set(packets)):
        if packets[packet_id]["state"] != "COMPLETE":
            bootstrap_allowed = (
                packets[packet_id]["state"] == "READY_FOR_EXECUTION"
                and bootstrap_packets == [packet_id]
            )
            if not bootstrap_allowed:
                failures.append(
                    f"{packet_id} is COMPLETE in accepted receipts but active as "
                    f"{packets[packet_id]['state']}"
                )

    if future_text:
        failures.extend(future_route_profile_failures(future_text))
        failures.extend(weak_agent_dispatch_failures(next_text, current_packets))
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
                if (
                    prerequisite not in packets
                    and prerequisite not in historical_packets
                    and prerequisite not in accepted_packets
                )
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
    for routed in ROUTED_PACKET_STATE_RE.finditer(routing):
        packet_id = routed.group("packet")
        if packet_id in current_packets:
            declared_state = routed.group("state")
            actual_state = str(current_packets[packet_id]["state"])
            if declared_state != actual_state:
                failures.append(
                    f"Active Routing says {packet_id} is {declared_state} "
                    f"but its structural State is {actual_state}"
                )
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


def next_decision_hygiene_failures(next_text: str) -> list[str]:
    """Keep the executable window bounded and replace-only."""

    failures: list[str] = []
    byte_count = len(next_text.encode("utf-8"))
    line_count = len(next_text.splitlines())
    if byte_count > NEXT_DECISION_MAX_BYTES:
        failures.append(
            "NEXT_DECISION exceeds byte budget: "
            f"{byte_count} > {NEXT_DECISION_MAX_BYTES}"
        )
    if line_count > NEXT_DECISION_MAX_LINES:
        failures.append(
            "NEXT_DECISION exceeds line budget: "
            f"{line_count} > {NEXT_DECISION_MAX_LINES}"
        )
    headings = NEXT_DECISION_APPEND_ONLY_HEADING_RE.findall(next_text)
    if headings:
        failures.append(
            "NEXT_DECISION contains append-only history; replace stale state in place "
            "and rely on Git history"
        )
    return failures


def session_context_route_failures(start_here: str) -> list[str]:
    """Verify every agent role has one bounded machine-readable route."""

    script = ROOT / "scripts" / "session_context.py"
    spec = importlib.util.spec_from_file_location("session_context_handoff_check", script)
    if spec is None or spec.loader is None:
        return ["cannot import scripts/session_context.py"]
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    try:
        spec.loader.exec_module(module)
        for schema_name in (
            "RouteContract",
            "ContextRoute",
            "PacketExtract",
            "PacketBinding",
            "CheckoutSnapshot",
            "VerificationResult",
            "SessionCheckpoint",
            "ResumeDisposition",
            "SessionEntry",
        ):
            schema = getattr(module, schema_name, None)
            if not dataclasses.is_dataclass(schema) or not schema.__dataclass_params__.frozen:
                return [f"session context schema {schema_name} must be a frozen dataclass"]
        contract = module.parse_route_contract(start_here)
        packet = {
            "packet_id": "TOOL-ROUTE-CHECK-1",
            "state": "READY_FOR_EXECUTION",
            "source_path": "docs/NEXT_DECISION.md",
            "packet_sha256": "0" * 64,
            "allowed_paths": ["scripts/"],
            "execution_authorized": False,
            "checkpoint_allowed": True,
        }
        for role in sorted(module.ROLES):
            route = module.build_route(
                contract,
                role=role,
                accepted_main_sha="0" * 40,
                packet=packet,
            )
            if route["documents"][0] != "START_HERE.md":
                return [f"session context route for {role} does not start at START_HERE.md"]
            if len(route["documents"]) > contract.max_required_documents:
                return [f"session context route for {role} exceeds the required-document budget"]
            if route["execution_authorized"] or route["checkpoint_allowed"]:
                return [f"session context route for {role} grants execution authority"]
    except Exception as error:
        reason = getattr(error, "reason", str(error))
        return [f"START_HERE session context route contract invalid: {reason}"]
    return []


def check_active_state_consistency(failures: list[str]) -> None:
    status = read("docs/CURRENT_STATUS.md")
    next_text = read("docs/NEXT_DECISION.md")
    future_text = read("docs/FUTURE_ROUTE.md")
    failures.extend(next_decision_hygiene_failures(next_text))
    failures.extend(active_state_failures(status, next_text, future_text))
    if "## Verified Repository State" not in status:
        failures.append("CURRENT_STATUS must preserve the accepted/open/blocked fact boundary")


def check_session_context_routes(failures: list[str]) -> None:
    failures.extend(session_context_route_failures(read("START_HERE.md")))


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
    check_session_context_routes(failures)
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
