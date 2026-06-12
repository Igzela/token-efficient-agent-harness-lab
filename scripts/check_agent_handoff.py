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
        "Architecture Refactor R-series sealed at R7. R8 is not approved.",
        "Post-R7 wire/type governance hardening implemented:",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "CLAUDE.md": [
        "Autonomous Advancement Protocol",
        "docs/CURRENT_STATUS.md",
        "scripts/check_agent_handoff.py",
        "**Architecture Refactor R-series**: **SEALED AT R7**. R8 is not approved.",
        "**Post-R7 Wire/Type Governance Hardening**: IMPLEMENTED",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "README.md": [
        "autonomously advance safe repository work",
        "scripts/check_agent_handoff.py",
        "R-series is sealed at R7. R8 is not approved.",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "docs/SESSION_START_HERE.md": [
        "Autonomous Session Closeout",
        "Dispatch Kernel Phase 4",
        "Architecture Refactor R-series | Sealed at R7; R8 is not approved.",
        "Post-R7 Wire/Type Governance Hardening",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "docs/CURRENT_STATUS.md": [
        "Branch:",
        "Tests:",
        "Phase 4",
        "Architecture Refactor R-series Seal",
        "**SEALED AT R7**",
        "R8 is not approved.",
        "Post-R7 Wire/Type Governance Hardening",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "docs/NEXT_DECISION.md": [
        "Autonomously maintain",
        "Allowed Next Paths",
        "Disallowed by Default",
        "Architecture refactor (R-series)",
        "**SEALED AT R7.**",
        "R8 is not approved.",
        "No further R-series file splitting is approved.",
    ],
    "docs/MODULE_MAP.md": [
        "# Module Map",
        "| Module | Stage | Purpose |",
        "`scripts/check_wire_codegen_drift.sh`",
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

    # --- Drift checks ---

    # Check 1: Schema version constant exists and is readable
    migrations_path = ROOT / "engine" / "src" / "storage" / "local_product_store" / "migrations.rs"
    if migrations_path.exists():
        migrations_text = migrations_path.read_text(encoding="utf-8")
        if "CURRENT_SCHEMA_VERSION" not in migrations_text:
            failures.append("migrations.rs is missing CURRENT_SCHEMA_VERSION constant")
        else:
            match = re.search(r'CURRENT_SCHEMA_VERSION\s*:\s*i64\s*=\s*(\d+)', migrations_text)
            if not match:
                failures.append("Cannot parse CURRENT_SCHEMA_VERSION from migrations.rs")
    else:
        failures.append("migrations.rs not found at expected path")

    # Check 2: Docs inventory — all referenced files exist
    inventory_path = ROOT / "docs" / "DOCS_INVENTORY.md"
    if inventory_path.exists():
        inv_text = inventory_path.read_text(encoding="utf-8")
        for match in re.finditer(r'\|\s*`?(docs/[^`\s|]+)`?\s*\|', inv_text):
            doc_path = match.group(1).strip()
            full_path = ROOT / doc_path
            if not full_path.exists():
                failures.append(f"DOCS_INVENTORY references missing file: {doc_path}")
    else:
        failures.append("docs/DOCS_INVENTORY.md not found")

    # Check 3: Architecture Book exists, is non-empty, and schema version matches migrations.rs
    arch_book = ROOT / "docs" / "ARCHITECTURE_BOOK.md"
    if not arch_book.exists():
        failures.append("docs/ARCHITECTURE_BOOK.md not found")
    elif arch_book.stat().st_size == 0:
        failures.append("docs/ARCHITECTURE_BOOK.md is empty")
    elif migrations_path.exists():
        # Cross-check schema version between migrations.rs and ARCHITECTURE_BOOK.md
        arch_text = arch_book.read_text(encoding="utf-8")
        m_code = re.search(r'CURRENT_SCHEMA_VERSION\s*:\s*i64\s*=\s*(\d+)', migrations_text)
        m_doc = re.search(r'Current version:\s*v(\d+)', arch_text)
        if m_code and m_doc:
            code_version = int(m_code.group(1))
            doc_version = int(m_doc.group(1))
            if code_version != doc_version:
                failures.append(
                    f"Schema version mismatch: migrations.rs has v{code_version}, "
                    f"ARCHITECTURE_BOOK.md has v{doc_version}"
                )

    # Check 4: Phase 6 plan exists (only required when Phase 6 is declared active)
    current_status_path = ROOT / "docs" / "CURRENT_STATUS.md"
    if current_status_path.exists():
        status_text = current_status_path.read_text(encoding="utf-8")
        if "Phase 6" in status_text and ("active track" in status_text or "IN PROGRESS" in status_text):
            phase6_plan = ROOT / "docs" / "PHASE6_OPERATIONAL_READINESS_PLAN.md"
            if not phase6_plan.exists():
                failures.append("docs/PHASE6_OPERATIONAL_READINESS_PLAN.md not found (Phase 6 is active in CURRENT_STATUS)")

    if failures:
        print("Agent handoff check FAILED:")
        for failure in failures:
            print(f"- {failure}")
        return 1

    print("Agent handoff check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
