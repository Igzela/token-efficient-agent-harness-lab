#!/usr/bin/env python3
"""Validate the coding-agent handoff surface.

This check is intentionally lightweight. It prevents a future autonomous
session from committing a state where the entry documents no longer describe
how the next agent should continue.
"""

from __future__ import annotations

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]

REQUIRED_FILES = {
    "AGENTS.md": [
        "Autonomous Advancement Authority",
        "Autonomous Advancement Loop",
        "Documentation Maintenance Rule",
    ],
    "CLAUDE.md": [
        "Autonomous Advancement Protocol",
        "docs/CURRENT_STATUS.md",
        "scripts/check_agent_handoff.py",
    ],
    "README.md": [
        "autonomously advance safe repository work",
        "1639 tests pass",
        "scripts/check_agent_handoff.py",
    ],
    "docs/SESSION_START_HERE.md": [
        "Autonomous Session Closeout",
        "1639 pass",
        "Dispatch Kernel Phase 4",
    ],
    "docs/CURRENT_STATUS.md": [
        "Branch:",
        "Tests:",
        "Phase 4",
    ],
    "docs/NEXT_DECISION.md": [
        "Autonomously maintain",
        "Allowed Next Paths",
        "Disallowed by Default",
    ],
    "docs/MODULE_MAP.md": [
        "# Module Map",
        "| Module | Stage | Purpose |",
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

    if failures:
        print("Agent handoff check FAILED:")
        for failure in failures:
            print(f"- {failure}")
        return 1

    print("Agent handoff check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
