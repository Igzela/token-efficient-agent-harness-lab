#!/usr/bin/env python3
"""Validate coding-agent handoff, autonomy, and active-document contracts."""

from __future__ import annotations

import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]

REQUIRED_FILES = {
    "AGENTS.md": [
        "Autonomous Operating Model",
        "Model Selection",
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
        "Post-R7 wire/type governance hardening implemented:",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "CLAUDE.md": [
        "Autonomous Advancement Protocol",
        "docs/CURRENT_STATUS.md",
        "scripts/check_agent_handoff.py",
        "Full Agent Autonomy Mode",
        "**Post-R7 Wire/Type Governance Hardening**: IMPLEMENTED",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "README.md": [
        "Full Agent Autonomy Mode",
        "repo-scoped, testable, observable, reviewable, and rollbackable",
        "scripts/check_agent_handoff.py",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "docs/ARCHITECTURE_BOOK.md": [
        "# Architecture Book",
        "Current version: v",
        "Product Boundary",
        "Dashboard Boundary",
        "Full Agent Autonomy Mode",
    ],
    "docs/CURRENT_STATUS.md": [
        "Branch:",
        "Tests:",
        "Phase 4",
        "Full Agent Autonomy Mode",
        "Model selection",
        "READY_FOR_EXECUTION",
        "Autonomous decision authority",
        "Post-R7 Wire/Type Governance Hardening",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "docs/NEXT_DECISION.md": [
        "Full Agent Autonomy Mode",
        "Autonomously maintain and evolve",
        "Model Selection",
        "Execution-Ready Packet Protocol",
        "READY_FOR_EXECUTION",
        "DECISION_REQUIRED",
        "Hard Stops",
        "repo-scoped, testable, observable, CI-gated, and rollbackable",
        "Packet PE1-UI-1",
        "Packet PE2-CONTRACT-1",
        "Packet PE3-CONTRACT-1",
        "Packet PE4-CONTRACT-1",
        "Packet PE5-SBOM-1",
        "Packet PE6-INVARIANTS-1",
    ],
    "docs/MODULE_MAP.md": [
        "# Module Map",
        "| Module | Stage | Purpose |",
        "Full Agent Autonomy Mode",
        "READY_FOR_EXECUTION",
        "`scripts/check_wire_codegen_drift.sh`",
    ],
    "docs/REAL_WORLD_TESTING_PLAYBOOK.md": [
        "# Real-World Testing Playbook",
        "Full Agent Autonomy Mode",
        "Model Selection",
        "Action Permission Matrix",
        "New architecture/authority/recovery decision",
        "docs/archive/",
    ],
    "docs/RUNBOOK.md": [
        "# Agent Control Plane",
        "Operator procedures",
    ],
    "scripts/verify_rust_typescript_stack.sh": [
        "bash scripts/check_wire_codegen_drift.sh",
    ],
    ".github/workflows/tests.yml": [
        "run: bash scripts/check_wire_codegen_drift.sh",
    ],
}

MODEL_AGNOSTIC_FILES = [
    "AGENTS.md",
    "docs/CURRENT_STATUS.md",
    "docs/NEXT_DECISION.md",
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


def check_required_text(failures: list[str]) -> None:
    for relative_path, snippets in REQUIRED_FILES.items():
        path = ROOT / relative_path
        if not path.exists():
            failures.append(f"missing required handoff file: {relative_path}")
            continue
        text = path.read_text(encoding="utf-8")
        for snippet in snippets:
            if snippet not in text:
                failures.append(f"{relative_path} is missing required text: {snippet!r}")


def check_model_agnostic_governance(failures: list[str]) -> None:
    for relative_path in MODEL_AGNOSTIC_FILES:
        path = ROOT / relative_path
        if not path.exists():
            continue
        text = path.read_text(encoding="utf-8")
        for marker in FORBIDDEN_MODEL_LOCK_MARKERS:
            if marker in text:
                failures.append(
                    f"{relative_path} must remain model-agnostic; found stale marker {marker!r}"
                )


def run_guard(command: list[str], label: str, failures: list[str]) -> None:
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        output = (result.stdout or result.stderr).strip()
        failures.append(f"{label} failed: {output}")


def check_schema_document_drift(failures: list[str]) -> None:
    schema_path = ROOT / "engine" / "src" / "storage" / "local_product_store" / "schema.rs"
    migrations_path = ROOT / "engine" / "src" / "storage" / "local_product_store" / "migrations.rs"
    architecture_path = ROOT / "docs" / "ARCHITECTURE_BOOK.md"

    code_version: int | None = None
    if not schema_path.exists():
        failures.append("schema.rs not found at expected path")
    else:
        match = re.search(
            r"CURRENT_SQLITE_SCHEMA_VERSION\s*:\s*i64\s*=\s*(\d+)",
            schema_path.read_text(encoding="utf-8"),
        )
        if not match:
            failures.append("Cannot parse CURRENT_SQLITE_SCHEMA_VERSION from schema.rs")
        else:
            code_version = int(match.group(1))

    if not migrations_path.exists():
        failures.append("migrations.rs not found at expected path")
    elif "CURRENT_SCHEMA_VERSION" not in migrations_path.read_text(encoding="utf-8"):
        failures.append("migrations.rs is missing CURRENT_SCHEMA_VERSION constant")

    if not architecture_path.exists():
        failures.append("docs/ARCHITECTURE_BOOK.md not found")
    elif architecture_path.stat().st_size == 0:
        failures.append("docs/ARCHITECTURE_BOOK.md is empty")
    elif code_version is not None:
        match = re.search(
            r"Current version:\s*v(\d+)",
            architecture_path.read_text(encoding="utf-8"),
        )
        if not match:
            failures.append(
                "ARCHITECTURE_BOOK.md is missing 'Current version: vN' "
                "(required for schema version drift check)"
            )
        elif int(match.group(1)) != code_version:
            failures.append(
                f"Schema version mismatch: schema.rs has v{code_version}, "
                f"ARCHITECTURE_BOOK.md has v{match.group(1)}"
            )


def check_phase_handoff(failures: list[str]) -> None:
    status_path = ROOT / "docs" / "CURRENT_STATUS.md"
    next_path = ROOT / "docs" / "NEXT_DECISION.md"
    if not status_path.exists():
        return
    status_text = status_path.read_text(encoding="utf-8")
    if "Phase 6" in status_text and (
        "active track" in status_text.lower() or "IN PROGRESS" in status_text
    ):
        next_text = next_path.read_text(encoding="utf-8") if next_path.exists() else ""
        if "Phase 6" not in next_text:
            failures.append(
                "docs/NEXT_DECISION.md must describe active Phase 6 work "
                "when CURRENT_STATUS declares Phase 6 active"
            )


def main() -> int:
    failures: list[str] = []
    check_required_text(failures)
    check_model_agnostic_governance(failures)

    wire_guard = ROOT / "scripts" / "check_wire_codegen_drift.sh"
    if not wire_guard.exists():
        failures.append("missing required wire codegen drift guard")
    elif not wire_guard.is_file():
        failures.append("wire codegen drift guard is not a file")
    elif not os.access(wire_guard, os.X_OK):
        failures.append("wire codegen drift guard is not executable")
    else:
        run_guard(["bash", str(wire_guard)], "wire codegen drift guard", failures)

    toolchain_guard = ROOT / "scripts" / "check_toolchain_drift.sh"
    if toolchain_guard.exists():
        run_guard(["bash", str(toolchain_guard)], "toolchain drift guard", failures)

    secret_scan = ROOT / "scripts" / "acp_secret_scan.py"
    secret_result = subprocess.run(
        [sys.executable, str(secret_scan)],
        capture_output=True,
        text=True,
    )
    if secret_result.returncode != 0:
        print("Agent handoff check FAILED — secret scan:")
        print((secret_result.stdout or secret_result.stderr).strip())
        return 1

    check_schema_document_drift(failures)
    check_phase_handoff(failures)

    if failures:
        print("Agent handoff check FAILED:")
        for failure in failures:
            print(f"- {failure}")
        return 1

    print("Agent handoff check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())