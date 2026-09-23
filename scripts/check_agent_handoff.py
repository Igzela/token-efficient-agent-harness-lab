#!/usr/bin/env python3
"""Validate direct repository navigation, canonical docs, and handoff checks."""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
CANONICAL_DOCUMENTS = (
    "README.md",
    "START_HERE.md",
    "AGENTS.md",
    "docs/ARCHITECTURE.md",
    "docs/AUTONOMY.md",
    "docs/ROADMAP.md",
    "docs/RUNBOOK.md",
)
REQUIRED_TEXT = {
    "START_HERE.md": (
        "# Start Here",
        "## Choose and start the work",
        "## Safety and evidence",
        "scripts/project_context.py",
        "## Handoff",
        "## Documentation boundaries",
    ),
    "AGENTS.md": (
        "Read `START_HERE.md` first",
        "## Autonomous Operating Model",
        "## Ordinary Repository Maintenance",
        "## Delivery and Verification",
        "## Hard Stops",
        "scripts/check_wire_codegen_drift.sh",
    ),
    "CLAUDE.md": (
        "# Claude Code Adapter",
        "START_HERE.md",
        "AGENTS.md",
        "docs/ARCHITECTURE.md",
        "docs/AUTONOMY.md",
    ),
    "README.md": (
        "START_HERE.md",
        "AGENTS.md",
        "docs/ARCHITECTURE.md",
        "docs/ROADMAP.md",
        "scripts/check_wire_codegen_drift.sh",
    ),
    "docs/ARCHITECTURE.md": (
        "# Architecture",
        "Repository Development and Delivery",
        "Core Module Ownership",
        "Managed Effect and Single Ownership Invariant",
    ),
    "docs/AUTONOMY.md": (
        "# Autonomy and Testing Contract",
        "## Autonomous repository work",
        "## Verification and evidence",
        "## Review Convergence Protocol",
        "## Exact-head CI and guarded merge",
    ),
    "docs/ROADMAP.md": (
        "# Project Roadmap",
        "## Mainline: Autonomous Development and Maintenance",
        "## Optional Research Program (Parked)",
    ),
    "docs/RUNBOOK.md": (
        "# Agent Control Plane — Runbook",
        "## Repository delivery",
        "## Agent Runtime and Tool Policy Operations",
        "## Release Upgrade and Rollback",
    ),
}
FORBIDDEN_MODEL_LOCK_MARKERS = (
    "gpt-5.6-terra",
    "READY_FOR_TERRA",
    "model_profile_mismatch",
    "Mandatory Codex Execution Profile",
    "Mandatory Executor Profile",
)
FORBIDDEN_ADAPTER_HEADINGS = {
    "CLAUDE.md": (
        "## Current State",
        "## Authority and Safety",
        "## Autonomous Advancement Protocol",
        "## Documentation Maintenance",
        "## Test Strategy",
    ),
}


def read(relative_path: str) -> str:
    try:
        return (ROOT / relative_path).read_text(encoding="utf-8")
    except OSError:
        return ""


def run_guard(command: list[str], label: str, failures: list[str]) -> None:
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    if result.returncode:
        failures.append(f"{label} failed: {(result.stdout or result.stderr).strip()}")


def check_required_text(failures: list[str]) -> None:
    for relative_path, snippets in REQUIRED_TEXT.items():
        if not (ROOT / relative_path).is_file():
            failures.append(f"missing required handoff file: {relative_path}")
            continue
        content = read(relative_path)
        for snippet in snippets:
            if snippet not in content:
                failures.append(f"{relative_path} is missing required text: {snippet!r}")


def check_entrypoint(failures: list[str]) -> None:
    start = read("START_HERE.md")
    if re.search(r"\bPR #\d+\b", start) or "Last updated:" in start:
        failures.append("START_HERE.md must remain stable navigation, not current PR/status")
    for path, headings in FORBIDDEN_ADAPTER_HEADINGS.items():
        for heading in headings:
            if heading in read(path):
                failures.append(f"{path} duplicates canonical section {heading!r}")
    for relative_path in CANONICAL_DOCUMENTS:
        if f"`{relative_path}`" not in start:
            failures.append(f"START_HERE.md does not route to {relative_path}")


def check_model_agnostic_governance(failures: list[str]) -> None:
    for relative_path in (
        "START_HERE.md",
        "AGENTS.md",
        "docs/ARCHITECTURE.md",
        "docs/AUTONOMY.md",
        "docs/ROADMAP.md",
        "docs/RUNBOOK.md",
    ):
        for marker in FORBIDDEN_MODEL_LOCK_MARKERS:
            if marker in read(relative_path):
                failures.append(f"{relative_path} contains stale model lock {marker!r}")


def check_schema_document_drift(failures: list[str]) -> None:
    schema = read("engine/src/storage/local_product_store/schema.rs")
    migrations = read("engine/src/storage/local_product_store/migrations.rs")
    architecture = read("docs/ARCHITECTURE.md")
    version = re.search(r"CURRENT_SQLITE_SCHEMA_VERSION\s*:\s*i64\s*=\s*(\d+)", schema)
    documented = re.search(r"Current version:\s*v(\d+)", architecture)
    if not version:
        failures.append("cannot parse CURRENT_SQLITE_SCHEMA_VERSION from schema.rs")
    elif not documented:
        failures.append("ARCHITECTURE.md is missing 'Current version: vN'")
    elif documented.group(1) != version.group(1):
        failures.append(
            f"schema version mismatch: schema.rs has v{version.group(1)}, "
            f"ARCHITECTURE.md has v{documented.group(1)}"
        )
    if "CURRENT_SCHEMA_VERSION" not in migrations:
        failures.append("migrations.rs is missing CURRENT_SCHEMA_VERSION")


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
        capsule = module.build_capsule(
            offline=True,
            repository="Igzela/token-efficient-agent-harness-lab",
        )
    except Exception as error:
        failures.append(f"offline project-context generation failed: {error}")
        return
    for field in (
        "schema_version",
        "accepted_baseline",
        "canonical_document_source",
        "local_checkout",
        "current_pr",
        "suggested_next_step",
        "required_reading",
        "hard_stops",
    ):
        if field not in capsule:
            failures.append(f"offline project context is missing {field}")
    for removed_field in (
        "active_mission",
        "steward_continuity",
        "execution_authority",
        "owner_direct_repair",
    ):
        if removed_field in capsule or removed_field in capsule.get("binding", {}):
            failures.append(f"project context still exposes removed routing field {removed_field}")
    matrix = (capsule.get("binding") or {}).get("source_required_check_matrix")
    if not isinstance(matrix, list) or len(matrix) != len(module.REQUIRED_SOURCE_CI_CHECKS):
        failures.append("project context required-check matrix has an invalid shape")
    next_step = capsule.get("suggested_next_step", "")
    if any(marker in next_step for marker in ("Mission", "WorkCard", "earliest eligible")):
        failures.append("project context must not route work through a lifecycle contract")


def main() -> int:
    failures: list[str] = []
    check_required_text(failures)
    check_entrypoint(failures)
    check_model_agnostic_governance(failures)
    check_schema_document_drift(failures)
    check_project_context(failures)

    wire_guard = ROOT / "scripts/check_wire_codegen_drift.sh"
    if not wire_guard.is_file() or not os.access(wire_guard, os.X_OK):
        failures.append("missing or non-executable wire codegen drift guard")
    else:
        run_guard(["bash", str(wire_guard)], "wire codegen drift guard", failures)

    toolchain_guard = ROOT / "scripts/check_toolchain_drift.sh"
    if toolchain_guard.exists():
        run_guard(["bash", str(toolchain_guard)], "toolchain drift guard", failures)

    secret_scan = ROOT / "scripts/acp_secret_scan.py"
    if not secret_scan.is_file():
        failures.append("missing secret scan")
    else:
        result = subprocess.run(
            [sys.executable, str(secret_scan)],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        if result.returncode:
            print("Agent handoff check FAILED — secret scan:")
            print((result.stdout or result.stderr).strip())
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
