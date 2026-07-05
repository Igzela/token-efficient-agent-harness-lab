#!/usr/bin/env bash
set -euo pipefail

# Exercises the trusted-local adaptive completion path using a stub provider only.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ACP_ADAPTIVE_VERIFY_PORT:-18081}"
TMP_DIR="$(mktemp -d)"
ENGINE_PID=""
AUTH_TOKEN="harness_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
ENDPOINTS='[{"endpoint_id":"stub-a","provider_type":"stub","model":"stub-model","timeout_ms":30000,"input_cost_per_1k_usd":0.001,"output_cost_per_1k_usd":0.001}]'

cleanup() {
  if [[ -n "${ENGINE_PID}" ]] && kill -0 "${ENGINE_PID}" 2>/dev/null; then
    kill "${ENGINE_PID}" 2>/dev/null || true
    wait "${ENGINE_PID}" 2>/dev/null || true
  fi
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

cd "${ROOT}"

if [[ ! -x "${ROOT}/target/debug/agent-control-plane" ]]; then
  echo "ERROR: target/debug/agent-control-plane is missing; run cargo build -p engine first." >&2
  exit 1
fi

HOST=127.0.0.1 \
PORT="${PORT}" \
ACP_DB_PATH="${TMP_DIR}/local-team.db" \
ACP_BACKUP_DIR="${TMP_DIR}/backups" \
ACP_DASHBOARD_DIR="${ROOT}/dashboard/out" \
ACP_REQUIRE_AUTH=1 \
ACP_ADMIN_API_KEY="${AUTH_TOKEN}" \
ACP_TRUSTED_LOCAL_PROFILE=1 \
ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON="${ENDPOINTS}" \
ACP_COST_PER_DISPATCH_USD=1 \
ACP_COST_DAILY_USD=10 \
"${ROOT}/target/debug/agent-control-plane" >"${TMP_DIR}/engine.log" 2>&1 &
ENGINE_PID="$!"

for _ in {1..80}; do
  if curl -fsS "http://127.0.0.1:${PORT}/api/v1/health" >"${TMP_DIR}/health.json" 2>/dev/null; then
    break
  fi
  sleep 0.25
done
if [[ ! -s "${TMP_DIR}/health.json" ]]; then
  echo "adaptive stub engine did not become healthy on port ${PORT}" >&2
  sed -n '1,200p' "${TMP_DIR}/engine.log" >&2 || true
  exit 1
fi

curl -fsS \
  -X POST "http://127.0.0.1:${PORT}/api/v1/adaptive-fusion/completions" \
  -H "content-type: application/json" \
  -H "authorization: Bearer ${AUTH_TOKEN}" \
  -d '{"prompt":"hello adaptive stub","task_class":"smoke","risk_level":"low"}' \
  >"${TMP_DIR}/completion.json"

grep -q '"output"' "${TMP_DIR}/completion.json"
grep -q '\[stub:stub-a\]' "${TMP_DIR}/completion.json"

echo "Adaptive stub completion smoke passed."
