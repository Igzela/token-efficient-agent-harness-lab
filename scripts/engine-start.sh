#!/usr/bin/env bash
set -euo pipefail

HOST="${HOST:-127.0.0.1}"
PORT="${PORT:-8080}"
PIDFILE="${PIDFILE:-.engine.pid}"

if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
  echo "engine already running (pid $(cat "$PIDFILE"))"
  exit 0
fi

cargo run --release -p engine &
echo $! > "$PIDFILE"
echo "engine started (pid $!), waiting for health..."

for i in $(seq 1 30); do
  if curl -sf "http://${HOST}:${PORT}/api/v1/health" > /dev/null 2>&1; then
    echo "engine healthy on ${HOST}:${PORT}"
    exit 0
  fi
  sleep 1
done

echo "engine failed to become healthy"
exit 1
