"""Regenerate Python reference golden fixtures for dispatch wire contract v1."""

from __future__ import annotations

from common import write_python_golden_fixtures


def main() -> int:
    written = write_python_golden_fixtures()
    for path in written:
        print(f"wrote {path}")
    print(f"wrote {len(written)} Python reference golden fixtures")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
