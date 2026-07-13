#!/usr/bin/env bash
set -euo pipefail

# Agent Control Plane — Native Installer
# Usage: ./install.sh [--prefix /usr/local] [--data-dir PATH]
#                     [release evidence flags] | [--development]

PREFIX="${PREFIX:-/usr/local}"
BIN_DIR="${PREFIX}/bin"
DATA_DIR="${ACP_DATA_DIR:-${HOME}/.agent-control-plane}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEVELOPMENT=false
ARTIFACT_PATH=""
SBOM_PATH=""
ATTESTATION_PATH=""
PROVENANCE_PATH=""
EXTERNAL_VERIFICATION_PATH=""

# Parse --prefix flag
while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix)
            [[ $# -ge 2 ]] || { echo "Error: --prefix requires a value" >&2; exit 1; }
            PREFIX="$2"; BIN_DIR="${PREFIX}/bin"; shift 2 ;;
        --data-dir)
            [[ $# -ge 2 ]] || { echo "Error: --data-dir requires a value" >&2; exit 1; }
            DATA_DIR="$2"; shift 2 ;;
        --artifact)
            [[ $# -ge 2 ]] || { echo "Error: --artifact requires a value" >&2; exit 1; }
            ARTIFACT_PATH="$2"; shift 2 ;;
        --sbom)
            [[ $# -ge 2 ]] || { echo "Error: --sbom requires a value" >&2; exit 1; }
            SBOM_PATH="$2"; shift 2 ;;
        --attestation)
            [[ $# -ge 2 ]] || { echo "Error: --attestation requires a value" >&2; exit 1; }
            ATTESTATION_PATH="$2"; shift 2 ;;
        --provenance)
            [[ $# -ge 2 ]] || { echo "Error: --provenance requires a value" >&2; exit 1; }
            PROVENANCE_PATH="$2"; shift 2 ;;
        --external-verification)
            [[ $# -ge 2 ]] || { echo "Error: --external-verification requires a value" >&2; exit 1; }
            EXTERNAL_VERIFICATION_PATH="$2"; shift 2 ;;
        --development)
            DEVELOPMENT=true; shift ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "Agent Control Plane — Installer"
echo "  Binary:  ${BIN_DIR}/agent-control-plane"
echo "  Data:    ${DATA_DIR}/"
echo ""

verify_release_evidence() {
    if [[ "${DEVELOPMENT}" == "true" ]]; then
        echo "  Explicit development install: production provenance is not claimed."
        return 0
    fi
    if [[ -z "${ARTIFACT_PATH}" || -z "${SBOM_PATH}" || -z "${ATTESTATION_PATH}" || -z "${PROVENANCE_PATH}" ]]; then
        echo "Error: release evidence is required; use --development only for an explicit source/development install." >&2
        exit 1
    fi
    local verifier="${SCRIPT_DIR}/release_provenance.py"
    if [[ ! -f "${verifier}" ]]; then
        verifier="${REPO_ROOT}/scripts/release_provenance.py"
    fi
    if [[ ! -f "${verifier}" ]]; then
        echo "Error: release provenance verifier is unavailable; refusing installation." >&2
        exit 1
    fi
    local result
    local -a verify_args
    verify_args=(
        verify
        --artifact "${ARTIFACT_PATH}"
        --sbom "${SBOM_PATH}"
        --attestation "${ATTESTATION_PATH}"
        --provenance "${PROVENANCE_PATH}"
        --mode production
    )
    if [[ -n "${EXTERNAL_VERIFICATION_PATH}" ]]; then
        verify_args+=(--external-verification "${EXTERNAL_VERIFICATION_PATH}")
    fi
    result="$(mktemp)"
    if ! python3 "${verifier}" "${verify_args[@]}" --output "${result}" >/dev/null; then
        rm -f "${result}"
        echo "Error: release provenance verification failed; no activation occurred." >&2
        exit 1
    fi
    rm -f "${result}"
    echo "  Release provenance verified before activation."
}

verify_release_evidence

# Prefer packaged release layout, then source checkout layout.
BINARY="${SCRIPT_DIR}/engine"
if [[ ! -f "${BINARY}" ]]; then
    BINARY="${REPO_ROOT}/target/release/agent-control-plane"
fi
if [[ ! -f "${BINARY}" ]]; then
    echo "Error: agent-control-plane binary not found"
    exit 1
fi

if [[ -e "${BIN_DIR}/agent-control-plane" ]]; then
    echo "Error: an existing installation was found at ${BIN_DIR}/agent-control-plane" >&2
    echo "Use upgrade.sh so the previous known-good installation remains recoverable." >&2
    exit 1
fi

# Install binary
echo "Installing binary..."
if [[ -d "${BIN_DIR}" && -w "${BIN_DIR}" ]]; then
    install -m 0755 "${BINARY}" "${BIN_DIR}/agent-control-plane"
else
    sudo mkdir -p "${BIN_DIR}"
    sudo install -m 0755 "${BINARY}" "${BIN_DIR}/agent-control-plane"
fi
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
    cp -a "${DASHBOARD_SRC}/." "${DASHBOARD_DST}/"
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
