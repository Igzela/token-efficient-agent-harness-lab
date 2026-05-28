#!/usr/bin/env bash
set -euo pipefail

PIDFILE="${PIDFILE:-.engine.pid}"

if [ ! -f "$PIDFILE" ]; then
  echo "no PID file found"
  exit 0
fi

PID=$(cat "$PIDFILE")
if kill -0 "$PID" 2>/dev/null; then
  kill "$PID"
  echo "sent SIGTERM to $PID"
  for i in $(seq 1 10); do
    kill -0 "$PID" 2>/dev/null || { echo "engine stopped"; rm -f "$PIDFILE"; exit 0; }
    sleep 1
  done
  echo "engine did not stop, sending SIGKILL"
  kill -9 "$PID"
  rm -f "$PIDFILE"
else
  echo "engine not running (stale PID $PID)"
  rm -f "$PIDFILE"
fi
