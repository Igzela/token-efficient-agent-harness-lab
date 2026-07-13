#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PYTHONPATH="${ROOT}" python3 "${ROOT}/tools/release_workflow_contract.py" \
    "${ROOT}/.github/workflows/release.yml" \
    "${ROOT}/scripts/install-from-release.sh" \
    "${ROOT}/README.md" \
    "${ROOT}/docs/RUNBOOK.md"
PYTHONPATH="${ROOT}" python3 -m unittest \
    tools.test_release_workflow_contract \
    tools.test_release_provenance_v2 \
    tools.test_release_installation
