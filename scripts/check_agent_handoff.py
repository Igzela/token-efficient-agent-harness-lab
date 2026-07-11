#!/usr/bin/env python3
"""Validate the coding-agent handoff and Terra Medium execution contract."""

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
        "Mandatory Codex Execution Profile",
        "gpt-5.6-terra",
        "reasoning effort: `medium`",
        "READY_FOR_TERRA",
        "model_profile_mismatch",
        "two coherent repair cycles",
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
        "Codex executor profile",
        "gpt-5.6-terra",
        "READY_FOR_TERRA",
        ".codex/config.toml",
        "Post-R7 Wire/Type Governance Hardening",
        "scripts/check_wire_codegen_drift.sh",
    ],
    "docs/NEXT_DECISION.md": [
        "Full Agent Autonomy Mode",
        "Autonomously maintain and evolve",
        "Mandatory Executor Profile",
        "Terra-Ready Packet Protocol",
        "READY_FOR_TERRA",
        "profile mismatch is a hard stop",
        "Hard Stops",
        "repo-scoped, testable, observable, and rollbackable",
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

EXPECTED_CODEX_PROFILE = {
    "model": "gpt-5.6-terra",
    "review_model": "gpt-5.6-terra",
    "model_reasoning_effort": "medium",
    "plan_mode_reasoning_effort": "medium",
}


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


def check_codex_profile(failures: list[str]) -> None:
    path = ROOT / ".codex" / "config.toml"
    if not path.exists():
        failures.append("missing project Codex profile: .codex/config.toml")
        return
    text = path.read_text(encoding="utf-8")
    parsed: dict[str, str] = {}
    for key, value in re.findall(r'^([A-Za-z0-9_]+)\s*=\s*"([^"]+)"\s*$', text, re.MULTILINE):
        parsed[key] = value
    for key, expected in EXPECTED_CODEX_PROFILE.items():
        actual = parsed.get(key)
        if actual != expected:
            failures.append(
                f".codex/config.toml must set {key}={expected!r}; found {actual!r}"
            )
    lowered = text.lower()
    if "gpt-5.6-sol" in lowered or re.search(r'\bsol\b', lowered):
        failures.append(".codex/config.toml must not configure Sol")


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
    check_codex_profile(failures)

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
