#!/usr/bin/env bash
set -euo pipefail

HOST="${HOST:-127.0.0.1}"
PORT="${PORT:-8080}"

curl -sf "http://${HOST}:${PORT}/api/v1/health" | python3 -m json.tool
