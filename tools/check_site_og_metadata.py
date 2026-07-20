#!/usr/bin/env python3
"""Fail closed when site social metadata or OG asset is missing."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "site" / "index.html"
OG = ROOT / "site" / "og.svg"


def main() -> int:
    failures: list[str] = []
    if not OG.is_file():
        failures.append(f"missing asset: {OG.relative_to(ROOT)}")
    text = INDEX.read_text(encoding="utf-8") if INDEX.is_file() else ""
    if not INDEX.is_file():
        failures.append("missing site/index.html")
    for needle in (
        'property="og:image"',
        'name="twitter:image"',
        "og.svg",
    ):
        if needle not in text:
            failures.append(f"site/index.html missing {needle!r}")
    if failures:
        print("site OG metadata check FAILED:", file=sys.stderr)
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1
    print("site OG metadata check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
