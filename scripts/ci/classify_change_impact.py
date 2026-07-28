#!/usr/bin/env python3
"""Conservative CI lane classifier for pull-request candidate heads."""

from __future__ import annotations

import argparse
import json
from pathlib import PurePosixPath

ROOT_DOCUMENTS = {
    "AGENTS.md",
    "CHANGELOG.md",
    "CITATION.cff",
    "CLAUDE.md",
    "CODE_OF_CONDUCT.md",
    "CONTRIBUTING.md",
    "README.md",
    "SECURITY.md",
    "START_HERE.md",
    "SUPPORT.md",
    "THIRD_PARTY_NOTICES.md",
}


def is_documentation_path(raw_path: str) -> bool:
    path = raw_path.strip().replace("\\", "/")
    if not path or path.startswith("/") or ".." in PurePosixPath(path).parts:
        return False
    return path in ROOT_DOCUMENTS or path.startswith("docs/")


def classify(paths: list[str], *, draft: bool) -> dict[str, object]:
    normalized = sorted({path.strip().replace("\\", "/") for path in paths if path.strip()})
    docs_only = bool(normalized) and all(is_documentation_path(path) for path in normalized)
    # Only Draft work is eligible for non-canonical fast feedback. Once a PR
    # becomes Ready, every candidate uses the canonical complete matrix. This
    # keeps the existing exact-head orchestrator contract singular.
    mode = "fast_draft" if draft else "full"
    return {
        "schema_version": "ci_change_impact.v1",
        "mode": mode,
        "fast_only": mode != "full",
        "docs_only": docs_only,
        "changed_files": normalized,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--path", action="append", default=[])
    parser.add_argument("--draft", action="store_true")
    args = parser.parse_args()
    print(json.dumps(classify(args.path, draft=args.draft), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
