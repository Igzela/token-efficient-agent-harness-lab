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
    if path in ROOT_DOCUMENTS:
        return True
    return path.startswith("docs/") and PurePosixPath(path).suffix.lower() in {".md", ".txt"}


def classify(paths: list[str], *, draft: bool) -> dict[str, object]:
    normalized = sorted({path.strip().replace("\\", "/") for path in paths if path.strip()})
    docs_only = bool(normalized) and all(is_documentation_path(path) for path in normalized)
    if draft:
        mode = "fast_draft"
    elif docs_only:
        mode = "docs_only"
    else:
        mode = "full"
    return {
        "schema_version": "ci_change_impact.v2",
        "mode": mode,
        "fast_only": mode == "fast_draft",
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
