#!/usr/bin/env bash
# Five-minute no-provider demo entrypoint.
# Usage:
#   ./scripts/demo.sh              # run and auto-cleanup
#   ./scripts/demo.sh --keep       # leave engine running under .acp-demo-state/
#   ./scripts/demo.sh --cleanup    # stop --keep session
#   ./scripts/demo.sh --self-test  # pure checks, no engine
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

MODE="${1:-}"
case "${MODE}" in
  --cleanup|--self-test|--keep|"")
    ;;
  -h|--help)
    sed -n '2,8p' "$0"
    exit 0
    ;;
  *)
    echo "Unknown option: ${MODE}" >&2
    echo "Usage: $0 [--keep|--cleanup|--self-test]" >&2
    exit 2
    ;;
esac

if [[ "${MODE}" == "--cleanup" || "${MODE}" == "--self-test" ]]; then
  exec uv run --no-project python scripts/demo_no_provider.py "${MODE}"
fi

ENGINE_BIN="${ROOT}/target/debug/agent-control-plane"
if [[ ! -x "${ENGINE_BIN}" ]]; then
  echo "Building engine (cargo build -p engine)…"
  cargo build -p engine
fi

if [[ ! -f "${ROOT}/dashboard/out/index.html" ]]; then
  echo "Building static dashboard…"
  (cd dashboard && bun install --frozen-lockfile && bun run build:static)
fi

if [[ "${MODE}" == "--keep" ]]; then
  exec uv run --no-project python scripts/demo_no_provider.py --keep
fi

exec uv run --no-project python scripts/demo_no_provider.py
