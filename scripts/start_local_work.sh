#!/usr/bin/env bash
# Start the engine and complete one local folder task.
# Usage:
#   ./scripts/start_local_work.sh
#   ./scripts/start_local_work.sh --cleanup
#   ./scripts/start_local_work.sh --self-test
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

MODE="${1:-}"
case "${MODE}" in
  --cleanup|--self-test|"")
    ;;
  -h|--help)
    sed -n '2,6p' "$0"
    exit 0
    ;;
  *)
    echo "Unknown option: ${MODE}" >&2
    echo "Usage: $0 [--cleanup|--self-test]" >&2
    exit 2
    ;;
esac

if [[ "${MODE}" == "--cleanup" || "${MODE}" == "--self-test" ]]; then
  exec uv run --no-project python scripts/start_local_work.py "${MODE}"
fi

ENGINE_BIN="${ROOT}/target/debug/agent-control-plane"
if [[ ! -x "${ENGINE_BIN}" ]]; then
  echo "Building engine (cargo build -p engine)…"
  cargo build -p engine
fi

if [[ ! -f "${ROOT}/dashboard/out/index.html" ]] && command -v bun >/dev/null 2>&1; then
  echo "Building static dashboard…"
  (cd dashboard && bun install --frozen-lockfile && bun run build:static)
fi

exec uv run --no-project python scripts/start_local_work.py
