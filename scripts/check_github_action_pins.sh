#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
failures=0

while IFS= read -r line; do
    reference="${line#*uses: }"
    reference="${reference%% *}"
    if [[ "${reference}" == ./* ]]; then
        continue
    fi
    if [[ ! "${reference}" =~ @[0-9a-f]{40}$ ]]; then
        echo "Unpinned GitHub Action: ${line}" >&2
        failures=1
    fi
done < <(grep -R -n -E '^[[:space:]]+-[[:space:]]+uses:[[:space:]]+' "${ROOT}/.github/workflows" --include='*.yml' --include='*.yaml')

if [[ "${failures}" -ne 0 ]]; then
    exit 1
fi

echo "github action pins ok"
