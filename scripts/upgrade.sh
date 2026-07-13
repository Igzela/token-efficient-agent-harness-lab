#!/usr/bin/env bash
set -Eeuo pipefail

# Agent Control Plane — Upgrade Script
# Usage: ./upgrade.sh [--prefix /usr/local] [--data-dir PATH]
#                     [--stop-command CMD --restart-command CMD]
#                     [release evidence flags] | [--development]

PREFIX="${PREFIX:-/usr/local}"
DATA_DIR="${ACP_DATA_DIR:-${HOME}/.agent-control-plane}"
STOP_COMMAND=""
RESTART_COMMAND=""
HEALTH_COMMAND=""
DEVELOPMENT=false
ARTIFACT_PATH=""
SBOM_PATH=""
ATTESTATION_PATH=""
PROVENANCE_PATH=""
EXTERNAL_VERIFICATION_PATH=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix)
            [[ $# -ge 2 ]] || { echo "Error: --prefix requires a value" >&2; exit 1; }
            PREFIX="$2"
            shift 2
            ;;
        --data-dir)
            [[ $# -ge 2 ]] || { echo "Error: --data-dir requires a value" >&2; exit 1; }
            DATA_DIR="$2"
            shift 2
            ;;
        --stop-command)
            [[ $# -ge 2 ]] || { echo "Error: --stop-command requires a value" >&2; exit 1; }
            STOP_COMMAND="$2"
            shift 2
            ;;
        --restart-command)
            [[ $# -ge 2 ]] || { echo "Error: --restart-command requires a value" >&2; exit 1; }
            RESTART_COMMAND="$2"
            shift 2
            ;;
        --health-command)
            [[ $# -ge 2 ]] || { echo "Error: --health-command requires a value" >&2; exit 1; }
            HEALTH_COMMAND="$2"
            shift 2
            ;;
        --artifact)
            [[ $# -ge 2 ]] || { echo "Error: --artifact requires a value" >&2; exit 1; }
            ARTIFACT_PATH="$2"
            shift 2
            ;;
        --sbom)
            [[ $# -ge 2 ]] || { echo "Error: --sbom requires a value" >&2; exit 1; }
            SBOM_PATH="$2"
            shift 2
            ;;
        --attestation)
            [[ $# -ge 2 ]] || { echo "Error: --attestation requires a value" >&2; exit 1; }
            ATTESTATION_PATH="$2"
            shift 2
            ;;
        --provenance)
            [[ $# -ge 2 ]] || { echo "Error: --provenance requires a value" >&2; exit 1; }
            PROVENANCE_PATH="$2"
            shift 2
            ;;
        --external-verification)
            [[ $# -ge 2 ]] || { echo "Error: --external-verification requires a value" >&2; exit 1; }
            EXTERNAL_VERIFICATION_PATH="$2"
            shift 2
            ;;
        --development)
            DEVELOPMENT=true
            shift
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

if [[ -n "${STOP_COMMAND}" || -n "${RESTART_COMMAND}" ]]; then
    if [[ -z "${STOP_COMMAND}" || -z "${RESTART_COMMAND}" ]]; then
        echo "Error: --stop-command and --restart-command must be provided together" >&2
        exit 1
    fi
fi

BIN_DIR="${PREFIX}/bin"
TARGET="${BIN_DIR}/agent-control-plane"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

if [[ ! -f "${TARGET}" ]]; then
    echo "Error: no existing installation found at ${TARGET}" >&2
    echo "Run install.sh first." >&2
    exit 1
fi

BINARY="${SCRIPT_DIR}/engine"
if [[ ! -f "${BINARY}" ]]; then
    BINARY="${REPO_ROOT}/target/release/agent-control-plane"
fi
if [[ ! -s "${BINARY}" ]]; then
    echo "Error: non-empty agent-control-plane binary not found" >&2
    exit 1
fi

verify_release_evidence() {
    if [[ "${DEVELOPMENT}" == "true" ]]; then
        echo "  Explicit development upgrade: production provenance is not claimed."
        return 0
    fi
    if [[ -z "${ARTIFACT_PATH}" || -z "${SBOM_PATH}" || -z "${ATTESTATION_PATH}" || -z "${PROVENANCE_PATH}" ]]; then
        echo "Error: release evidence is required; use --development only for an explicit source/development upgrade." >&2
        exit 1
    fi
    local verifier="${SCRIPT_DIR}/release_provenance.py"
    if [[ ! -f "${verifier}" ]]; then
        verifier="${REPO_ROOT}/scripts/release_provenance.py"
    fi
    if [[ ! -f "${verifier}" ]]; then
        echo "Error: release provenance verifier is unavailable; refusing upgrade." >&2
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
        echo "Error: release provenance verification failed; previous installation was not changed." >&2
        exit 1
    fi
    rm -f "${result}"
    echo "  Release provenance verified before activation."
}

verify_release_evidence

DASHBOARD_SRC="${SCRIPT_DIR}/dashboard"
if [[ ! -d "${DASHBOARD_SRC}" ]]; then
    DASHBOARD_SRC="${REPO_ROOT}/dashboard/out"
fi

BACKUP="${TARGET}.bak"
BINARY_TMP="${TARGET}.new.$$"
DASHBOARD_DST="${DATA_DIR}/dashboard"
DASHBOARD_BACKUP="${DATA_DIR}/dashboard.bak"
DASHBOARD_TMP="${DATA_DIR}/.dashboard.new.$$"
UPGRADE_STARTED=false
DASHBOARD_BACKED_UP=false
DASHBOARD_REPLACED=false
PROCESS_STOPPED=false

rollback() {
    local status="${1:-1}"
    trap - ERR INT TERM
    rm -f "${BINARY_TMP}" 2>/dev/null || true
    rm -rf "${DASHBOARD_TMP}" 2>/dev/null || true

    if [[ "${UPGRADE_STARTED}" == "true" && -f "${BACKUP}" ]]; then
        local rollback_tmp="${TARGET}.rollback.$$"
        cp -p "${BACKUP}" "${rollback_tmp}" 2>/dev/null || true
        chmod +x "${rollback_tmp}" 2>/dev/null || true
        mv -f "${rollback_tmp}" "${TARGET}" 2>/dev/null || true
    fi
    if [[ "${DASHBOARD_BACKED_UP}" == "true" && -d "${DASHBOARD_BACKUP}" ]]; then
        rm -rf "${DASHBOARD_DST}" 2>/dev/null || true
        mv "${DASHBOARD_BACKUP}" "${DASHBOARD_DST}" 2>/dev/null || true
    elif [[ "${DASHBOARD_REPLACED}" == "true" ]]; then
        rm -rf "${DASHBOARD_DST}" 2>/dev/null || true
    fi
    if [[ "${PROCESS_STOPPED}" == "true" ]]; then
        bash -lc "${RESTART_COMMAND}" >/dev/null 2>&1 || true
    fi
    echo "Upgrade failed; previous installation restored." >&2
    exit "${status}"
}

trap 'rollback $?' ERR
trap 'rollback 130' INT TERM

if [[ -n "${STOP_COMMAND}" ]]; then
    echo "Stopping service with the explicit operator hook..."
    bash -lc "${STOP_COMMAND}"
    PROCESS_STOPPED=true
fi

OLD_SIZE="$(wc -c < "${TARGET}" | tr -d ' ')"
cp -p "${TARGET}" "${BACKUP}.new.$$"
mv -f "${BACKUP}.new.$$" "${BACKUP}"
UPGRADE_STARTED=true
echo "  Backed up current binary to ${BACKUP}"

install -m 0755 "${BINARY}" "${BINARY_TMP}"
mv -f "${BINARY_TMP}" "${TARGET}"
NEW_SIZE="$(wc -c < "${TARGET}" | tr -d ' ')"
echo "  Upgraded atomically: ${OLD_SIZE} bytes -> ${NEW_SIZE} bytes"

if [[ -d "${DASHBOARD_SRC}" ]]; then
    mkdir -p "${DATA_DIR}"
    rm -rf "${DASHBOARD_TMP}"
    mkdir -p "${DASHBOARD_TMP}"
    cp -a "${DASHBOARD_SRC}/." "${DASHBOARD_TMP}/"
    rm -rf "${DASHBOARD_BACKUP}"
    if [[ -d "${DASHBOARD_DST}" ]]; then
        mv "${DASHBOARD_DST}" "${DASHBOARD_BACKUP}"
        DASHBOARD_BACKED_UP=true
    fi
    mv "${DASHBOARD_TMP}" "${DASHBOARD_DST}"
    DASHBOARD_REPLACED=true
    echo "  Dashboard assets replaced atomically"
fi

if [[ -n "${RESTART_COMMAND}" ]]; then
    echo "Restarting service with the explicit operator hook..."
    bash -lc "${RESTART_COMMAND}"
else
    echo "  Process state was not changed; restart the service with your process manager if needed."
fi

if [[ -n "${HEALTH_COMMAND}" ]]; then
    echo "Checking health with the explicit operator hook..."
    bash -lc "${HEALTH_COMMAND}"
else
    echo "Checking executable health..."
    "${TARGET}" --help >/dev/null 2>&1
fi
PROCESS_STOPPED=false

trap - ERR INT TERM
echo ""
echo "Upgrade complete. Rollback binary remains at ${BACKUP}."
