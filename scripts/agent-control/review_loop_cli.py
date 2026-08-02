#!/usr/bin/env python3
"""Thin CLI entrypoint for the review-loop transport.

The operator injects the browser transport and the read-only GitHub adapter
via environment variables:
  REVIEW_TRANSPORT_MODULE  - python import path of a Transport implementation
  REVIEW_GITHUB_MODULE     - python import path of a LiveGitHub implementation
  REVIEW_LOCK_DIR          - directory for per-chat lock files
  REVIEW_JOURNAL_PATH      - append-only journal file path

Without an explicit transport and GitHub adapter the launcher FAILS CLOSED:
no implicit fake may claim SENT_CONFIRMED or post a comment.  The `--test`
flag (for provider-free smoke only) enables deterministic fakes.

This file only wires dependencies; all decisions live in review_loop/ modules.
"""

from __future__ import annotations

import importlib
import os
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from review_loop import github_adapter, journal as journal_mod  # noqa: E402
from review_loop.cli import main  # noqa: E402
from review_loop.transport import FakeTransport  # noqa: E402


def _load(name: str):
    module_name, _, attr = name.partition(":")
    module = importlib.import_module(module_name)
    return getattr(module, attr) if attr else module


def _default_transport(test: bool):
    env = os.environ.get("REVIEW_TRANSPORT_MODULE")
    if env:
        return _load(env)
    if test:
        return FakeTransport()
    return None


def _default_github(test: bool):
    env = os.environ.get("REVIEW_GITHUB_MODULE")
    if env:
        return _load(env)
    if test:
        return github_adapter.FakeGitHub()
    return None


if __name__ == "__main__":
    test = "--test" in sys.argv
    if test:
        sys.argv.remove("--test")
    journal_path = os.environ.get("REVIEW_JOURNAL_PATH", "review-loop-events.jsonl")
    lock_dir = os.environ.get("REVIEW_LOCK_DIR") or None
    transport = _default_transport(test)
    github = _default_github(test)
    if not test and (transport is None or github is None):
        print(
            "error: review-loop refuses to run without REVIEW_TRANSPORT_MODULE and "
            "REVIEW_GITHUB_MODULE; use --test only for provider-free smoke",
            file=sys.stderr,
        )
        raise SystemExit(1)
    main(
        sys.argv[1:],
        transport=transport,
        github=github,
        journal=journal_mod.Journal(pathlib.Path(journal_path)),
        lock_dir=pathlib.Path(lock_dir) if lock_dir else None,
    )
