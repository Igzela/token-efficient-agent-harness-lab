#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/release.yml"
INSTALLER="${ROOT}/scripts/install-from-release.sh"

grep -Fq 'PACKAGE="${{ env.PACKAGE_PREFIX }}-${GITHUB_REF_NAME}-${{ matrix.target }}"' "${WORKFLOW}"
grep -Fq 'agent-control-plane-${version}-${target}.tar.gz' "${INSTALLER}"
grep -Fq 'target="${arch}-${os}"' "${INSTALLER}"
grep -Fq 'echo "unknown-linux-gnu"' "${INSTALLER}"
grep -Fq 'echo "apple-darwin"' "${INSTALLER}"
grep -Fq '"${extracted_dir}/engine"' "${INSTALLER}"
grep -Fq 'shasum -a 256' "${WORKFLOW}"

echo "release contract ok"
