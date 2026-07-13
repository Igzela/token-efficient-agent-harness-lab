#!/usr/bin/env bash
set -Eeuo pipefail

# Agent Control Plane — transactional native installer.
# Production callers must provide the three exact local attestation bundles.

PREFIX="${PREFIX:-/usr/local}"
DATA_DIR="${ACP_DATA_DIR:-${HOME}/.agent-control-plane}"
DEVELOPMENT=false
ARTIFACT_PATH=""
SBOM_PATH=""
MANIFEST_PATH=""
SLSA_BUNDLE_PATH=""
SPDX_BUNDLE_PATH=""
MANIFEST_BUNDLE_PATH=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix) [[ $# -ge 2 ]] || exit 2; PREFIX="$2"; shift 2 ;;
        --data-dir) [[ $# -ge 2 ]] || exit 2; DATA_DIR="$2"; shift 2 ;;
        --artifact) [[ $# -ge 2 ]] || exit 2; ARTIFACT_PATH="$2"; shift 2 ;;
        --sbom) [[ $# -ge 2 ]] || exit 2; SBOM_PATH="$2"; shift 2 ;;
        --manifest) [[ $# -ge 2 ]] || exit 2; MANIFEST_PATH="$2"; shift 2 ;;
        --slsa-bundle) [[ $# -ge 2 ]] || exit 2; SLSA_BUNDLE_PATH="$2"; shift 2 ;;
        --spdx-bundle) [[ $# -ge 2 ]] || exit 2; SPDX_BUNDLE_PATH="$2"; shift 2 ;;
        --manifest-bundle) [[ $# -ge 2 ]] || exit 2; MANIFEST_BUNDLE_PATH="$2"; shift 2 ;;
        --development) DEVELOPMENT=true; shift ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
done

BIN_DIR="${PREFIX}/bin"
TARGET="${BIN_DIR}/agent-control-plane"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BINARY="${SCRIPT_DIR}/engine"
[[ -s "${BINARY}" ]] || BINARY="${REPO_ROOT}/target/release/agent-control-plane"
[[ -s "${BINARY}" ]] || { echo "Error: non-empty release binary is missing" >&2; exit 1; }

verify_release_evidence() {
    if [[ "${DEVELOPMENT}" == "true" ]]; then
        echo "  Explicit development install: production provenance is not claimed."
        return
    fi
    local value
    for value in "${ARTIFACT_PATH}" "${SBOM_PATH}" "${MANIFEST_PATH}" \
        "${SLSA_BUNDLE_PATH}" "${SPDX_BUNDLE_PATH}" "${MANIFEST_BUNDLE_PATH}"; do
        [[ -n "${value}" && -f "${value}" ]] || {
            echo "Error: all exact release evidence paths are required" >&2
            exit 1
        }
    done
    python3 "${SCRIPT_DIR}/release_provenance.py" verify-release \
        --artifact "${ARTIFACT_PATH}" \
        --sbom "${SBOM_PATH}" \
        --manifest "${MANIFEST_PATH}" \
        --slsa-bundle "${SLSA_BUNDLE_PATH}" \
        --spdx-bundle "${SPDX_BUNDLE_PATH}" \
        --manifest-bundle "${MANIFEST_BUNDLE_PATH}" \
        --mode production >/dev/null
    echo "  Exact local release bundles verified before activation."
}

verify_release_evidence
[[ ! -e "${TARGET}" ]] || {
    echo "Error: existing installation requires upgrade.sh" >&2
    exit 1
}
mkdir -p "${BIN_DIR}" "${DATA_DIR}"
[[ -w "${BIN_DIR}" && -w "${DATA_DIR}" ]] || {
    echo "Error: install prefix and data directory must be writable; rerun with appropriate privileges" >&2
    exit 1
}

TXN="$$"
BINARY_STAGE="${BIN_DIR}/.agent-control-plane.install.${TXN}"
DASHBOARD_STAGE="${DATA_DIR}/.dashboard.install.${TXN}"
DASHBOARD_BACKUP="${DATA_DIR}/.dashboard.preinstall.${TXN}"
ENV_STAGE="${DATA_DIR}/.env.example.install.${TXN}"
DASHBOARD_DST="${DATA_DIR}/dashboard"
BINARY_ACTIVATED=false
DASHBOARD_ACTIVATED=false
DASHBOARD_EXISTED=false
ENV_ACTIVATED=false
COMMITTED=false

cleanup_failed_install() {
    local status="${1:-1}"
    trap - ERR INT TERM
    if [[ "${COMMITTED}" == "true" ]]; then
        return
    fi
    set +e
    local cleanup_ok=true
    rm -f "${BINARY_STAGE}" "${ENV_STAGE}" || cleanup_ok=false
    rm -rf "${DASHBOARD_STAGE}" || cleanup_ok=false
    if [[ "${BINARY_ACTIVATED}" == "true" ]]; then
        rm -f "${TARGET}" || cleanup_ok=false
    fi
    if [[ "${DASHBOARD_ACTIVATED}" == "true" ]]; then
        rm -rf "${DASHBOARD_DST}" || cleanup_ok=false
    fi
    if [[ "${DASHBOARD_EXISTED}" == "true" && -d "${DASHBOARD_BACKUP}" ]]; then
        mv "${DASHBOARD_BACKUP}" "${DASHBOARD_DST}" || cleanup_ok=false
    fi
    if [[ "${ENV_ACTIVATED}" == "true" ]]; then
        rm -f "${DATA_DIR}/.env.example" || cleanup_ok=false
    fi
    if [[ "${cleanup_ok}" == "true" && ! -e "${TARGET}" ]]; then
        echo "INSTALL_FAILED_NO_PARTIAL_ACTIVATION" >&2
    else
        echo "INSTALL_FAILED_CLEANUP_FAILED: pre-install backup retained at ${DASHBOARD_BACKUP}" >&2
    fi
    exit "${status}"
}
trap 'cleanup_failed_install $?' ERR
trap 'cleanup_failed_install 130' INT TERM

install -m 0755 "${BINARY}" "${BINARY_STAGE}"
DASHBOARD_SRC="${SCRIPT_DIR}/dashboard"
[[ -d "${DASHBOARD_SRC}" ]] || DASHBOARD_SRC="${REPO_ROOT}/dashboard/out"
if [[ -d "${DASHBOARD_SRC}" ]]; then
    mkdir -p "${DASHBOARD_STAGE}"
    cp -a "${DASHBOARD_SRC}/." "${DASHBOARD_STAGE}/"
fi
ENV_SOURCE="${SCRIPT_DIR}/.env.example"
[[ -f "${ENV_SOURCE}" ]] || ENV_SOURCE="${REPO_ROOT}/.env.example"
if [[ ! -e "${DATA_DIR}/.env.example" && -f "${ENV_SOURCE}" ]]; then
    cp "${ENV_SOURCE}" "${ENV_STAGE}"
fi

if [[ "${ACP_INSTALL_FAULT:-}" == "binary_move" ]]; then false; fi
mv "${BINARY_STAGE}" "${TARGET}"
BINARY_ACTIVATED=true
if [[ "${ACP_INSTALL_FAULT:-}" == "after_binary" ]]; then false; fi
if [[ "${ACP_INSTALL_FAULT:-}" == "interrupt_after_binary" ]]; then kill -TERM "$$"; fi

if [[ -d "${DASHBOARD_STAGE}" ]]; then
    if [[ -d "${DASHBOARD_DST}" ]]; then
        mv "${DASHBOARD_DST}" "${DASHBOARD_BACKUP}"
        DASHBOARD_EXISTED=true
    fi
    if [[ "${ACP_INSTALL_FAULT:-}" == "dashboard_move" ]]; then false; fi
    mv "${DASHBOARD_STAGE}" "${DASHBOARD_DST}"
    DASHBOARD_ACTIVATED=true
    if [[ "${ACP_INSTALL_FAULT:-}" == "after_dashboard" ]]; then false; fi
    if [[ "${ACP_INSTALL_FAULT:-}" == "interrupt_after_dashboard" ]]; then kill -TERM "$$"; fi
fi
if [[ -f "${ENV_STAGE}" ]]; then
    mv "${ENV_STAGE}" "${DATA_DIR}/.env.example"
    ENV_ACTIVATED=true
fi

COMMITTED=true
trap - ERR INT TERM
if [[ "${DASHBOARD_EXISTED}" == "true" ]]; then
    echo "Pre-install Dashboard preserved at ${DASHBOARD_BACKUP}."
fi
echo "Installation complete."
