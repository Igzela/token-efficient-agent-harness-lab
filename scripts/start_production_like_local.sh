#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${1:-$ROOT/.env.production-like.local}"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "Missing env file: $ENV_FILE" >&2
  echo "Create one with:" >&2
  echo "  cp $ROOT/.env.production-like.local.example $ENV_FILE" >&2
  exit 1
fi

set -a
# shellcheck disable=SC1090
source "$ENV_FILE"
set +a

HOST="${HOST:-127.0.0.1}"
PORT="${PORT:-8080}"
ACP_DB_PATH="${ACP_DB_PATH:-.agent-control-plane/production-like/local-team.db}"
ACP_BACKUP_DIR="${ACP_BACKUP_DIR:-.agent-control-plane/production-like/backups}"
ACP_DASHBOARD_DIR="${ACP_DASHBOARD_DIR:-dashboard/out}"
ACP_PROVIDER_TYPE="${ACP_PROVIDER_TYPE:-anthropic}"
ACP_BASE_URL="${ACP_BASE_URL:-https://token-plan-cn.xiaomimimo.com/anthropic}"
ACP_MODEL="${ACP_MODEL:-mimo-v2.5}"

require_true() {
  local name="$1"
  local value="${!name:-}"
  if [[ "$value" != "1" && "${value,,}" != "true" ]]; then
    echo "$name must be 1/true for production-like local profile" >&2
    exit 1
  fi
}

require_true ACP_REQUIRE_AUTH
require_true ACP_ENABLE_PROVIDER_EXECUTION

if [[ ! "${ACP_ADMIN_API_KEY:-}" =~ ^harness_[0-9a-fA-F]{64}$ ]]; then
  echo "ACP_ADMIN_API_KEY must match harness_<64 hex chars>" >&2
  exit 1
fi

if [[ "$ACP_PROVIDER_TYPE" != "anthropic" ]]; then
  echo "This profile expects ACP_PROVIDER_TYPE=anthropic" >&2
  exit 1
fi

if [[ -z "${ACP_API_KEY:-}" ]]; then
  echo "ACP_API_KEY must name the environment variable holding the provider secret" >&2
  exit 1
fi

provider_secret="${!ACP_API_KEY:-}"
if [[ -z "$provider_secret" ]]; then
  echo "Provider secret variable '$ACP_API_KEY' is empty" >&2
  exit 1
fi

if [[ ! -f "$ROOT/$ACP_DASHBOARD_DIR/index.html" && ! -f "$ACP_DASHBOARD_DIR/index.html" ]]; then
  echo "Dashboard export not found at $ACP_DASHBOARD_DIR/index.html" >&2
  echo "Build it first with: cd dashboard && node scripts/build-static.mjs" >&2
  exit 1
fi

mkdir -p "$(dirname "$ACP_DB_PATH")" "$ACP_BACKUP_DIR"

echo "[acp-production-like] host=$HOST port=$PORT provider=$ACP_PROVIDER_TYPE model=$ACP_MODEL base_url=$ACP_BASE_URL"
echo "[acp-production-like] auth=on cost_per_dispatch=${ACP_COST_PER_DISPATCH_USD:-unset} cost_daily=${ACP_COST_DAILY_USD:-unset}"
echo "[acp-production-like] db=$ACP_DB_PATH backups=$ACP_BACKUP_DIR dashboard=$ACP_DASHBOARD_DIR"
echo "[acp-production-like] provider_secret_env=$ACP_API_KEY value=***"

cd "$ROOT"
exec cargo run -p engine
