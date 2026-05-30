#!/usr/bin/env python3
"""Validate the coding-agent handoff surface.

This check is intentionally lightweight. It prevents a future autonomous
session from committing a state where the entry documents no longer describe
how the next agent should continue.
"""

from __future__ import annotations

import os
from pathlib import Path
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
        "`app_layer` is annotated as dormant/unwired parity reference code",
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
        "`engine/src/app_layer/`",
        "Dormant/unwired parity code retained for reference",
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

    print("Agent handoff check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
