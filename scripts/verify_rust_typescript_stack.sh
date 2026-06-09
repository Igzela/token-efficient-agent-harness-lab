#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ACP_VERIFY_PORT:-18080}"
TMP_DIR="$(mktemp -d)"
ENGINE_PID=""

cleanup() {
  if [[ -n "${ENGINE_PID}" ]] && kill -0 "${ENGINE_PID}" 2>/dev/null; then
    kill "${ENGINE_PID}" 2>/dev/null || true
    wait "${ENGINE_PID}" 2>/dev/null || true
  fi
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

# Preflight: require bun, cargo, and uv
if ! command -v bun &>/dev/null; then
  echo "ERROR: bun is required but not found in PATH." >&2
  echo "Install: curl -fsSL https://bun.sh/install | bash" >&2
  exit 1
fi

if ! command -v cargo &>/dev/null; then
  echo "ERROR: cargo is required but not found in PATH." >&2
  echo "Install: https://rustup.rs/" >&2
  exit 1
fi

if ! command -v uv &>/dev/null; then
  echo "ERROR: uv is required but not found in PATH." >&2
  echo "Install: curl -LsSf https://astral.sh/uv/install.sh | sh" >&2
  exit 1
fi

cd "${ROOT}"

bash scripts/check_wire_codegen_drift.sh
cargo fmt --check
cargo clippy -p engine -- -D warnings
cargo test -p engine

cd "${ROOT}/sdk/typescript"
bun install --frozen-lockfile
bun run test
bun run build

cd "${ROOT}/dashboard"
bun install --frozen-lockfile
bun run lint
bun run typecheck
bun run build
bun run build:static

cd "${ROOT}"
cargo build -p engine

unset ACP_PROVIDER_TYPE
unset ACP_ENABLE_PROVIDER_EXECUTION
unset ACP_API_KEY
unset ACP_MODEL
unset ACP_BASE_URL
unset ACP_REQUIRE_AUTH
unset ACP_ADMIN_API_KEY

HOST=127.0.0.1 \
PORT="${PORT}" \
ACP_DB_PATH="${TMP_DIR}/local-team.db" \
ACP_BACKUP_DIR="${TMP_DIR}/backups" \
ACP_DASHBOARD_DIR="${ROOT}/dashboard/out" \
"${ROOT}/target/debug/agent-control-plane" >"${TMP_DIR}/engine.log" 2>&1 &
ENGINE_PID="$!"

for _ in {1..80}; do
  if curl -fsS "http://127.0.0.1:${PORT}/api/v1/health" >"${TMP_DIR}/health.json"; then
    break
  fi
  sleep 0.25
done

curl -fsS "http://127.0.0.1:${PORT}/api/v1/dashboard" >"${TMP_DIR}/dashboard.json"
curl -fsS "http://127.0.0.1:${PORT}/" >"${TMP_DIR}/dashboard.html"
curl -fsS \
  -X POST "http://127.0.0.1:${PORT}/api/v1/dispatch" \
  -H "content-type: application/json" \
  -d '{"raw_request":"Summarize docs without provider calls","request_source":"api"}' \
  >"${TMP_DIR}/dispatch.json"

grep -q '"status"' "${TMP_DIR}/health.json"
grep -q 'local_dashboard.v1' "${TMP_DIR}/dashboard.json"
grep -q 'Agent Control Plane' "${TMP_DIR}/dashboard.html"
grep -q '"record"' "${TMP_DIR}/dispatch.json"

echo "Rust + TypeScript stack verification passed."
