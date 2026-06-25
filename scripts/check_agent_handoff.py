#!/usr/bin/env python3
"""Validate the coding-agent handoff surface.

This check is intentionally lightweight. It prevents a future autonomous
session from committing a state where the entry documents no longer describe
how the next agent should continue.
"""

from __future__ import annotations

import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]

REQUIRED_FILES = {
    "AGENTS.md": [
        "Autonomous Advancement Authority",
        "Autonomous Advancement Loop",
        "Documentation Maintenance Rule",
        "Full Agent Autonomy Mode",
        "new architecture directions",
        "authority-boundary changes",
        "default execution/profile changes",
        "auth/security redesign",
        "database migrations",
        "release/tag/deploy workflow changes",
        "target-output authority changes",
        "superseding accepted decisions",
        "do not commit real secrets",
        "do not falsify test or CI evidence",
        "do not intentionally hide failures",
        "do not remove rollback paths",
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
        "Post-R7 Wire/Type Governance Hardening",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "docs/NEXT_DECISION.md": [
        "Full Agent Autonomy Mode",
        "Autonomously maintain and evolve",
        "Allowed Next Paths",
        "Hard Stops",
        "Architecture refactor (R-series)",
        "repo-scoped, testable, observable, and rollbackable",
    ],
    "docs/MODULE_MAP.md": [
        "# Module Map",
        "| Module | Stage | Purpose |",
        "Full Agent Autonomy Mode",
        "`scripts/check_wire_codegen_drift.sh`",
    ],
    "docs/REAL_WORLD_TESTING_PLAYBOOK.md": [
        "# Real-World Testing Playbook",
        "Agent Autonomous Maintenance Mode",
        "Full Agent Autonomy Mode",
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


def main() -> int:
    failures: list[str] = []

    for relative_path, snippets in REQUIRED_FILES.items():
        path = ROOT / relative_path
        if not path.exists():
            failures.append(f"missing required handoff file: {relative_path}")
            continue

        text = path.read_text(encoding="utf-8")
        for snippet in snippets:
            if snippet not in text:
                failures.append(f"{relative_path} is missing required text: {snippet!r}")

    wire_codegen_guard = ROOT / "scripts" / "check_wire_codegen_drift.sh"
    if not wire_codegen_guard.exists():
        failures.append("missing required wire codegen drift guard: scripts/check_wire_codegen_drift.sh")
    elif not wire_codegen_guard.is_file():
        failures.append("wire codegen drift guard is not a file: scripts/check_wire_codegen_drift.sh")
    elif not os.access(wire_codegen_guard, os.X_OK):
        failures.append("wire codegen drift guard is not executable: scripts/check_wire_codegen_drift.sh")

    if failures:
        print("Agent handoff check FAILED:")
        for failure in failures:
            print(f"- {failure}")
        return 1

    drift_guard = ROOT / "scripts" / "check_toolchain_drift.sh"
    if drift_guard.exists():
        result = subprocess.run(
            ["bash", str(drift_guard)],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print("Agent handoff check FAILED — toolchain drift guard:")
            print(result.stdout.strip())
            return 1

    result = subprocess.run(
        ["bash", str(wire_codegen_guard)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print("Agent handoff check FAILED — wire codegen drift guard:")
        print(result.stdout.strip())
        return 1

    secret_scan = ROOT / "scripts" / "acp_secret_scan.py"
    result = subprocess.run(
        [sys.executable, str(secret_scan)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print("Agent handoff check FAILED — secret scan:")
        print(result.stdout.strip())
        return 1

    # --- Drift checks ---

    # Check 1: Schema catalog version constant exists and is readable
    schema_path = ROOT / "engine" / "src" / "storage" / "local_product_store" / "schema.rs"
    migrations_path = ROOT / "engine" / "src" / "storage" / "local_product_store" / "migrations.rs"
    code_version = None
    if schema_path.exists():
        schema_text = schema_path.read_text(encoding="utf-8")
        match = re.search(r'CURRENT_SQLITE_SCHEMA_VERSION\s*:\s*i64\s*=\s*(\d+)', schema_text)
        if not match:
            failures.append("Cannot parse CURRENT_SQLITE_SCHEMA_VERSION from schema.rs")
        else:
            code_version = int(match.group(1))
    else:
        failures.append("schema.rs not found at expected path")

    if migrations_path.exists():
        migrations_text = migrations_path.read_text(encoding="utf-8")
        if "CURRENT_SCHEMA_VERSION" not in migrations_text:
            failures.append("migrations.rs is missing CURRENT_SCHEMA_VERSION constant")
    else:
        failures.append("migrations.rs not found at expected path")

    # Check 2: Architecture Book exists, is non-empty, and schema version matches schema.rs
    arch_book = ROOT / "docs" / "ARCHITECTURE_BOOK.md"
    if not arch_book.exists():
        failures.append("docs/ARCHITECTURE_BOOK.md not found")
    elif arch_book.stat().st_size == 0:
        failures.append("docs/ARCHITECTURE_BOOK.md is empty")
    elif code_version is not None:
        # Cross-check schema version between schema.rs and ARCHITECTURE_BOOK.md
        arch_text = arch_book.read_text(encoding="utf-8")
        m_doc = re.search(r'Current version:\s*v(\d+)', arch_text)
        if not m_doc:
            failures.append(
                "ARCHITECTURE_BOOK.md is missing 'Current version: vN' "
                "(required for schema version drift check)"
            )
        else:
            doc_version = int(m_doc.group(1))
            if code_version != doc_version:
                failures.append(
                    f"Schema version mismatch: schema.rs has v{code_version}, "
                    f"ARCHITECTURE_BOOK.md has v{doc_version}"
                )

    # Check 4: active Phase 6 work has a forward plan in NEXT_DECISION.
    current_status_path = ROOT / "docs" / "CURRENT_STATUS.md"
    if current_status_path.exists():
        status_text = current_status_path.read_text(encoding="utf-8")
        if "Phase 6" in status_text and ("active track" in status_text or "IN PROGRESS" in status_text):
            next_decision_path = ROOT / "docs" / "NEXT_DECISION.md"
            next_decision_text = (
                next_decision_path.read_text(encoding="utf-8")
                if next_decision_path.exists()
                else ""
            )
            if "Phase 6" not in next_decision_text:
                failures.append(
                    "docs/NEXT_DECISION.md must describe active Phase 6 work "
                    "when CURRENT_STATUS declares Phase 6 active"
                )

    if failures:
        print("Agent handoff check FAILED:")
        for failure in failures:
            print(f"- {failure}")
        return 1

    print("Agent handoff check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
