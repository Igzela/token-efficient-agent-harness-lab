#!/usr/bin/env python3
"""Validate canonical navigation, handoff, and active-document contracts."""

from __future__ import annotations

import dataclasses
import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
MISSION_CONTRACT_PATH = ROOT / "scripts" / "agent-control" / "mission_contract.py"

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
        "docs/ARCHITECTURE.md",
        "docs/AUTONOMY.md",
    ],
    "README.md": [
        "START_HERE.md",
        "AGENTS.md",
        "docs/ARCHITECTURE.md",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "docs/ARCHITECTURE.md": [
        "# Architecture",
        "Current version: v",
        "Three-Tier Operational Model",
        "Core Module Ownership",
    ],
    "docs/AUTONOMY.md": [
        "# Autonomy and Testing Contract",
        "Lifecycle State Machine",
        "Three-Tier Contract Hierarchy",
        "Review Convergence Protocol",
        "Exact-Head CI and Guarded Merge",
    ],
    "docs/ROADMAP.md": [
        "# Project Roadmap",
        "Autonomous Steward Migration Milestones",
        "Research Horizons",
    ],
    "docs/RUNBOOK.md": [
        "# Agent Control Plane — Runbook",
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
        "def _build_checkpoint",
        "def classify_resume",
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
    "docs/ARCHITECTURE.md",
    "docs/AUTONOMY.md",
    "docs/ROADMAP.md",
    "docs/RUNBOOK.md",
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
        "docs/ARCHITECTURE.md",
        "docs/AUTONOMY.md",
        "docs/ROADMAP.md",
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
    architecture = read("docs/ARCHITECTURE.md")
    version = re.search(r"CURRENT_SQLITE_SCHEMA_VERSION\s*:\s*i64\s*=\s*(\d+)", schema)
    if not version:
        failures.append("Cannot parse CURRENT_SQLITE_SCHEMA_VERSION from schema.rs")
        return
    if "CURRENT_SCHEMA_VERSION" not in migrations:
        failures.append("migrations.rs is missing CURRENT_SCHEMA_VERSION constant")
    documented = re.search(r"Current version:\s*v(\d+)", architecture)
    if not documented:
        failures.append("ARCHITECTURE.md is missing 'Current version: vN'")
    elif documented.group(1) != version.group(1):
        failures.append(
            f"Schema version mismatch: schema.rs has v{version.group(1)}, "
            f"ARCHITECTURE.md has v{documented.group(1)}"
        )


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
        packet = module._canonical_session_packet(
            {"docs/AUTONOMY.md": "# Autonomy contract\n"}
        )
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
    required = (
        ROOT / "START_HERE.md",
        ROOT / "AGENTS.md",
        ROOT / "docs" / "ARCHITECTURE.md",
        ROOT / "docs" / "AUTONOMY.md",
        ROOT / "docs" / "ROADMAP.md",
        ROOT / "docs" / "RUNBOOK.md",
    )
    missing = [path.relative_to(ROOT).as_posix() for path in required if not path.is_file()]
    if missing:
        failures.append(f"Missing required canonical documents: {missing}")
    spec = importlib.util.spec_from_file_location("mission_contract_check", MISSION_CONTRACT_PATH)
    if spec and spec.loader:
        mc = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = mc
        try:
            spec.loader.exec_module(mc)
            mc.validate_registered_campaign()
        except Exception as e:
            failures.append(f"Registered campaign validation failed: {e}")


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
