#!/usr/bin/env bash
# Clean-environment validation entrypoint for strangers evaluating this repo.
#
# Usage:
#   ./scripts/external_validation.sh
#   ./scripts/external_validation.sh --report /tmp/external_validation_report.json
#   ./scripts/external_validation.sh --self-test
#   ./scripts/external_validation.sh --skip-demo   # install + exact-head only
#
# No API key, no provider call, no target-repository write.
# Leaves no background process when successful.
# Does not claim external adoption.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  sed -n '2,14p' "$0"
  exit 0
fi

exec uv run --no-project python scripts/external_validation.py "$@"
