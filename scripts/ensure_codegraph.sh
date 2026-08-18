#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if ! command -v codegraph >/dev/null 2>&1; then
  printf '%s\n' 'CodeGraph is mandatory but the codegraph CLI is unavailable.' >&2
  exit 1
fi

if [[ -d .codegraph ]]; then
  if ! codegraph sync "$repo_root"; then
    codegraph index "$repo_root"
  fi
else
  codegraph init "$repo_root"
fi

codegraph status "$repo_root"
