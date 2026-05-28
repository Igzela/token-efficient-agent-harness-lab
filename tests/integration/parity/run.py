"""Run dispatch wire-contract parity checks."""

from __future__ import annotations

from common import run_parity_checks


def main() -> int:
    report = run_parity_checks()
    print(
        "dispatch wire parity passed: "
        f"{report['checked_fixtures']} fixtures, "
        f"{report['schema_files']} schemas"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
