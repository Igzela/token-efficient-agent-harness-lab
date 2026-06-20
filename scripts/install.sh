#!/usr/bin/env bash
set -euo pipefail

# Agent Control Plane — Native Installer
# Usage: ./install.sh [--prefix /usr/local]

PREFIX="${PREFIX:-/usr/local}"
BIN_DIR="${PREFIX}/bin"
DATA_DIR="${HOME}/.agent-control-plane"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Parse --prefix flag
while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix) PREFIX="$2"; BIN_DIR="${PREFIX}/bin"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "Agent Control Plane — Installer"
echo "  Binary:  ${BIN_DIR}/agent-control-plane"
echo "  Data:    ${DATA_DIR}/"
echo ""

# Prefer packaged release layout, then source checkout layout.
BINARY="${SCRIPT_DIR}/engine"
if [[ ! -f "${BINARY}" ]]; then
    BINARY="${REPO_ROOT}/target/release/agent-control-plane"
fi
if [[ ! -f "${BINARY}" ]]; then
    echo "Error: agent-control-plane binary not found"
    exit 1
fi

# Install binary
echo "Installing binary..."
mkdir -p "${BIN_DIR}"
cp "${BINARY}" "${BIN_DIR}/agent-control-plane"
chmod +x "${BIN_DIR}/agent-control-plane"
echo "  -> ${BIN_DIR}/agent-control-plane"

# Create data directory
echo "Setting up data directory..."
mkdir -p "${DATA_DIR}/backups"
echo "  -> ${DATA_DIR}/"

# Install .env.example if not present
if [[ ! -f "${DATA_DIR}/.env.example" ]]; then
    ENV_EXAMPLE="${SCRIPT_DIR}/.env.example"
    if [[ ! -f "${ENV_EXAMPLE}" ]]; then
        ENV_EXAMPLE="${REPO_ROOT}/.env.example"
    fi
    if [[ -f "${ENV_EXAMPLE}" ]]; then
        cp "${ENV_EXAMPLE}" "${DATA_DIR}/.env.example"
        echo "  -> ${DATA_DIR}/.env.example"
    fi
fi

# Install dashboard assets if available
DASHBOARD_SRC="${SCRIPT_DIR}/dashboard"
if [[ ! -d "${DASHBOARD_SRC}" ]]; then
    DASHBOARD_SRC="${REPO_ROOT}/dashboard/out"
fi
if [[ -d "${DASHBOARD_SRC}" ]]; then
    DASHBOARD_DST="${DATA_DIR}/dashboard"
    mkdir -p "${DASHBOARD_DST}"
    cp -r "${DASHBOARD_SRC}/"* "${DASHBOARD_DST}/" 2>/dev/null || true
    echo "  -> ${DASHBOARD_DST}/"
fi

echo ""
echo "Installation complete."
echo ""
echo "Quick start:"
echo "  agent-control-plane"
echo ""
echo "With dashboard:"
echo "  ACP_DASHBOARD_DIR=${DATA_DIR}/dashboard agent-control-plane"
echo ""
echo "With auth:"
echo "  ACP_REQUIRE_AUTH=1 ACP_ADMIN_API_KEY=<your-key> agent-control-plane"
