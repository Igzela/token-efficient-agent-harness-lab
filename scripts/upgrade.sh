#!/usr/bin/env bash
set -euo pipefail

# Agent Control Plane — Upgrade Script
# Usage: ./upgrade.sh [--prefix /usr/local]

PREFIX="${PREFIX:-/usr/local}"
BIN_DIR="${PREFIX}/bin"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PID_FILE="${REPO_ROOT}/.engine.pid"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix) PREFIX="$2"; BIN_DIR="${PREFIX}/bin"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

TARGET="${BIN_DIR}/agent-control-plane"
if [[ ! -f "${TARGET}" ]]; then
    echo "Error: no existing installation found at ${TARGET}"
    echo "Run install.sh first."
    exit 1
fi

BINARY="${SCRIPT_DIR}/engine"
if [[ ! -f "${BINARY}" ]]; then
    BINARY="${REPO_ROOT}/target/release/agent-control-plane"
fi
if [[ ! -f "${BINARY}" ]]; then
    echo "Error: agent-control-plane binary not found"
    exit 1
fi

# Get current version (if --version works) or just file size
OLD_SIZE=$(stat -c%s "${TARGET}" 2>/dev/null || echo "0")

# Stop engine if running
WAS_RUNNING=false
if [[ -f "${PID_FILE}" ]]; then
    PID=$(cat "${PID_FILE}" 2>/dev/null || echo "")
    if [[ -n "${PID}" ]] && kill -0 "${PID}" 2>/dev/null; then
        echo "Stopping running engine (PID ${PID})..."
        "${SCRIPT_DIR}/engine-stop.sh" 2>/dev/null || kill "${PID}" 2>/dev/null || true
        WAS_RUNNING=true
        sleep 1
    fi
fi

# Back up current binary
BACKUP="${TARGET}.bak"
cp "${TARGET}" "${BACKUP}"
echo "  Backed up current binary to ${BACKUP}"

# Install new binary
cp "${BINARY}" "${TARGET}"
chmod +x "${TARGET}"
NEW_SIZE=$(stat -c%s "${TARGET}" 2>/dev/null || echo "0")
echo "  Upgraded: ${OLD_SIZE} bytes -> ${NEW_SIZE} bytes"

# Update dashboard assets if available
DASHBOARD_SRC="${SCRIPT_DIR}/dashboard"
if [[ ! -d "${DASHBOARD_SRC}" ]]; then
    DASHBOARD_SRC="${REPO_ROOT}/dashboard/out"
fi
DATA_DIR="${HOME}/.agent-control-plane"
if [[ -d "${DASHBOARD_SRC}" ]]; then
    DASHBOARD_DST="${DATA_DIR}/dashboard"
    mkdir -p "${DASHBOARD_DST}"
    cp -r "${DASHBOARD_SRC}/"* "${DASHBOARD_DST}/" 2>/dev/null || true
    echo "  Dashboard assets updated"
fi

# Restart engine if it was running
if [[ "${WAS_RUNNING}" == "true" ]]; then
    echo "Restarting engine..."
    "${SCRIPT_DIR}/engine-start.sh" 2>/dev/null || true
fi

echo ""
echo "Upgrade complete."
