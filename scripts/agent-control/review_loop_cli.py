#!/usr/bin/env python3
"""Thin CLI entrypoint for the review-loop transport.

The operator injects the transport (browser adapter) and GitHub client via
environment variables:
  REVIEW_TRANSPORT_MODULE  - python import path of a Transport implementation
  REVIEW_GITHUB_MODULE     - python import path of a GitHubCommentClient impl
When unset, CI-safe fakes are used so no external side effect occurs.

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

from review_loop import journal as journal_mod  # noqa: E402
from review_loop.cli import main  # noqa: E402
from review_loop.transport import FakeTransport  # noqa: E402


def _load(name: str):
    module_name, _, attr = name.partition(":")
    module = importlib.import_module(module_name)
    return getattr(module, attr) if attr else module


def _default_transport():
    env = os.environ.get("REVIEW_TRANSPORT_MODULE")
    if env:
        return _load(env)
    return FakeTransport()


def _default_client():
    env = os.environ.get("REVIEW_GITHUB_MODULE")
    if env:
        return _load(env)
    return None


if __name__ == "__main__":
    journal_path = os.environ.get("REVIEW_JOURNAL_PATH", "review-loop-events.jsonl")
    journal = journal_mod.Journal(pathlib.Path(journal_path))
    main(
        sys.argv[1:],
        transport=_default_transport(),
        client=_default_client(),
        journal=journal,
    )
