#!/usr/bin/env bash
set -euo pipefail

# Drift guard: fail if stale toolchain references reappear in authoritative docs/scripts.
# Historical closeout reports and stage acceptance docs are excluded.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAILURES=()

# Authoritative docs (living documents agents consult)
AUTHORITATIVE=(
  "AGENTS.md"
  "CLAUDE.md"
  "README.md"
  "docs/ARCHITECTURE_BOOK.md"
  "docs/CURRENT_STATUS.md"
  "docs/NEXT_DECISION.md"
  "docs/MODULE_MAP.md"
  "docs/REAL_WORLD_TESTING_PLAYBOOK.md"
  "docs/RUNBOOK.md"
)

# Authoritative scripts
SCRIPTS=(
  "scripts/verify_rust_typescript_stack.sh"
  "scripts/smoke_native_runtime.py"
  "scripts/install.sh"
  "scripts/upgrade.sh"
  "scripts/package-release.sh"
  "scripts/smoke_release.sh"
)

cd "${ROOT}"

check_pattern() {
  local pattern="$1"
  local label="$2"
  local files=("${@:3}")

  for f in "${files[@]}"; do
    [[ -f "${f}" ]] || continue
    while IFS= read -r line; do
      FAILURES+=("${f}: ${label} → ${line}")
    done < <(grep -nE "${pattern}" "${f}" 2>/dev/null || true)
  done
}

# Stale: bare python3 scripts/ without uv prefix
check_pattern '(^|[[:space:]])python3 scripts/' "bare python3 scripts/ (use: uv run --no-project python scripts/)" "${AUTHORITATIVE[@]}" "${SCRIPTS[@]}"

# Stale: bare python3 -m unittest without uv prefix
check_pattern '(^|[[:space:]])python3 -m unittest' "bare python3 -m unittest (use: uv run --no-project python -m unittest)" "${AUTHORITATIVE[@]}" "${SCRIPTS[@]}"

# Stale: bare python3 tools/ without uv prefix
check_pattern '(^|[[:space:]])python3 tools/' "bare python3 tools/ (use: uv run --no-project python tools/)" "${AUTHORITATIVE[@]}" "${SCRIPTS[@]}"

# Stale: pnpm (not in package-name context like "replaces pnpm-lock.yaml")
# Only flag pnpm when it appears as a command, not as a reference to the old lockfile
for f in "${AUTHORITATIVE[@]}" "${SCRIPTS[@]}"; do
  [[ -f "${f}" ]] || continue
  while IFS= read -r line; do
    # Skip lines that mention pnpm only as a historical reference (e.g., "replaces pnpm-lock.yaml")
    if echo "${line}" | grep -qE 'replaces.*pnpm|pnpm.*replaced|formerly.*pnpm|pnpm-lock'; then
      continue
    fi
    FAILURES+=("${f}: stale pnpm reference → ${line}")
  done < <(grep -nE 'pnpm' "${f}" 2>/dev/null || true)
done

# Stale: npm install or npm run as commands
for f in "${AUTHORITATIVE[@]}" "${SCRIPTS[@]}"; do
  [[ -f "${f}" ]] || continue
  while IFS= read -r line; do
    # Skip lines that mention npm only as historical context or package metadata
    if echo "${line}" | grep -qE 'npm pack|npm publish|replaces.*npm|npm.*replaced|formerly.*npm|not.*npm|no.*npm'; then
      continue
    fi
    FAILURES+=("${f}: stale npm reference → ${line}")
  done < <(grep -nE 'npm install|npm run' "${f}" 2>/dev/null || true)
done

# Stale: nvm references
check_pattern 'nvm' "stale nvm reference" "${AUTHORITATIVE[@]}" "${SCRIPTS[@]}"

if [[ ${#FAILURES[@]} -gt 0 ]]; then
  echo "Toolchain drift detected:"
  for f in "${FAILURES[@]}"; do
    echo "  - ${f}"
  done
  exit 1
fi

echo "Toolchain drift check passed."
