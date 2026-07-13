#!/usr/bin/env python3
"""Compatibility entry point for the authoritative control-state setup.

The complete required-label contract and control-Issue setup live in
``control_state.py``.  This command remains for operators who used the older
label-only CLI, but it intentionally delegates to the same implementation.
"""

from __future__ import annotations

import json
import sys

import control_state


REQUIRED_LABELS = control_state.REQUIRED_LABELS


def main() -> None:
    repo = None
    arguments = sys.argv[1:]
    if not arguments:
        pass
    elif len(arguments) == 2 and arguments[0] == "--repo":
        repo = arguments[1]
    else:
        raise SystemExit("Usage: setup_labels.py [--repo OWNER/REPO]")
    try:
        print(json.dumps(control_state.setup(repo), sort_keys=True))
    except control_state.ControlStateError as exc:
        print(f"CONTROL_STATE_ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
