#!/usr/bin/env bash
set -Eeuo pipefail

# Agent Control Plane — verified transactional upgrade and recovery owner.

PREFIX="${PREFIX:-/usr/local}"
DATA_DIR="${ACP_DATA_DIR:-${HOME}/.agent-control-plane}"
STOP_COMMAND=""
RESTART_COMMAND=""
HEALTH_COMMAND=""
DEVELOPMENT=false
RECOVER_ONLY=false
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
        --stop-command) [[ $# -ge 2 ]] || exit 2; STOP_COMMAND="$2"; shift 2 ;;
        --restart-command) [[ $# -ge 2 ]] || exit 2; RESTART_COMMAND="$2"; shift 2 ;;
        --health-command) [[ $# -ge 2 ]] || exit 2; HEALTH_COMMAND="$2"; shift 2 ;;
        --artifact) [[ $# -ge 2 ]] || exit 2; ARTIFACT_PATH="$2"; shift 2 ;;
        --sbom) [[ $# -ge 2 ]] || exit 2; SBOM_PATH="$2"; shift 2 ;;
        --manifest) [[ $# -ge 2 ]] || exit 2; MANIFEST_PATH="$2"; shift 2 ;;
        --slsa-bundle) [[ $# -ge 2 ]] || exit 2; SLSA_BUNDLE_PATH="$2"; shift 2 ;;
        --spdx-bundle) [[ $# -ge 2 ]] || exit 2; SPDX_BUNDLE_PATH="$2"; shift 2 ;;
        --manifest-bundle) [[ $# -ge 2 ]] || exit 2; MANIFEST_BUNDLE_PATH="$2"; shift 2 ;;
        --development) DEVELOPMENT=true; shift ;;
        --recover) RECOVER_ONLY=true; shift ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
done
if [[ -n "${STOP_COMMAND}" || -n "${RESTART_COMMAND}" ]]; then
    [[ -n "${STOP_COMMAND}" && -n "${RESTART_COMMAND}" ]] || {
        echo "Error: stop and restart hooks must be supplied together" >&2
        exit 2
    }
fi

BIN_DIR="${PREFIX}/bin"
TARGET="${BIN_DIR}/agent-control-plane"
BACKUP="${TARGET}.bak"
DATA_BACKUP="${DATA_DIR}/dashboard.bak"
ROLLBACK_STATE="${DATA_DIR}/upgrade-rollback.state"
DASHBOARD_DST="${DATA_DIR}/dashboard"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BINARY="${SCRIPT_DIR}/engine"
[[ -s "${BINARY}" ]] || BINARY="${REPO_ROOT}/target/release/agent-control-plane"

sha256_path() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
    else shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

check_health() {
    if [[ -n "${HEALTH_COMMAND}" ]]; then bash -lc "${HEALTH_COMMAND}"
    else "${TARGET}" --help >/dev/null 2>&1
    fi
}

OLD_DIGEST=""
OLD_DASHBOARD_EXISTED=false
OLD_PROCESS_MANAGED=false
PROCESS_STOPPED=false
ROLLBACK_RUNNING=false
TXN="$$"
BINARY_STAGE="${TARGET}.new.${TXN}"
DASHBOARD_STAGE="${DATA_DIR}/.dashboard.new.${TXN}"

read_state_field() {
    local requested="$1" key value
    while IFS='=' read -r key value; do
        if [[ "${key}" == "${requested}" ]]; then printf '%s' "${value}"; return 0; fi
    done < "${ROLLBACK_STATE}"
    return 1
}

load_rollback_state() {
    [[ -f "${ROLLBACK_STATE}" ]] || { echo "Error: rollback state is missing" >&2; exit 1; }
    [[ "$(read_state_field schema)" == "upgrade_rollback.v1" ]] || {
        echo "Error: rollback state schema is invalid" >&2; exit 1;
    }
    OLD_DIGEST="$(read_state_field old_digest)"
    [[ "${OLD_DIGEST}" =~ ^[0-9a-f]{64}$ ]] || {
        echo "Error: rollback binary digest is invalid" >&2; exit 1;
    }
    case "$(read_state_field dashboard)" in
        present) OLD_DASHBOARD_EXISTED=true ;;
        absent) OLD_DASHBOARD_EXISTED=false ;;
        *) echo "Error: rollback Dashboard state is invalid" >&2; exit 1 ;;
    esac
    case "$(read_state_field process)" in
        managed) OLD_PROCESS_MANAGED=true ;;
        unmanaged) OLD_PROCESS_MANAGED=false ;;
        *) echo "Error: rollback process state is invalid" >&2; exit 1 ;;
    esac
}

rollback() {
    local original_status="${1:-1}"
    if [[ "${ROLLBACK_RUNNING}" == "true" ]]; then exit 70; fi
    ROLLBACK_RUNNING=true
    trap - ERR INT TERM
    set +e
    local failed=0
    rm -f "${BINARY_STAGE}"
    rm -rf "${DASHBOARD_STAGE}"

    if [[ -z "${OLD_DIGEST}" && -f "${BACKUP}" ]]; then OLD_DIGEST="$(sha256_path "${BACKUP}")"; fi
    local binary_failed=0
    if [[ -f "${BACKUP}" ]]; then
        local restore_tmp="${TARGET}.rollback.${TXN}"
        cp -p "${BACKUP}" "${restore_tmp}" || binary_failed=1
        chmod 0755 "${restore_tmp}" || binary_failed=1
        if [[ "${ACP_ROLLBACK_FAULT:-}" == "binary_restore" ]]; then binary_failed=1
        elif [[ ${binary_failed} -eq 0 ]]; then mv -f "${restore_tmp}" "${TARGET}" || binary_failed=1; fi
        if [[ ${binary_failed} -eq 0 && "$(sha256_path "${TARGET}")" != "${OLD_DIGEST}" ]]; then binary_failed=1; fi
    else
        binary_failed=1
    fi
    [[ ${binary_failed} -eq 0 ]] || failed=1

    local dashboard_failed=0
    if [[ "${OLD_DASHBOARD_EXISTED}" == "true" ]]; then
        [[ -d "${DATA_BACKUP}" ]] || dashboard_failed=1
        local dashboard_restore="${DATA_DIR}/.dashboard.rollback.${TXN}"
        rm -rf "${dashboard_restore}"
        [[ ${dashboard_failed} -ne 0 ]] || cp -a "${DATA_BACKUP}" "${dashboard_restore}" || dashboard_failed=1
        if [[ "${ACP_ROLLBACK_FAULT:-}" == "dashboard_restore" ]]; then dashboard_failed=1
        elif [[ ${dashboard_failed} -eq 0 ]]; then
            rm -rf "${DASHBOARD_DST}.failed.${TXN}" || dashboard_failed=1
            if [[ ${dashboard_failed} -eq 0 && -e "${DASHBOARD_DST}" ]]; then
                mv "${DASHBOARD_DST}" "${DASHBOARD_DST}.failed.${TXN}" || dashboard_failed=1
            fi
            if [[ ${dashboard_failed} -eq 0 ]]; then mv "${dashboard_restore}" "${DASHBOARD_DST}" || dashboard_failed=1; fi
        fi
        if [[ ${dashboard_failed} -eq 0 ]]; then diff -qr "${DATA_BACKUP}" "${DASHBOARD_DST}" >/dev/null || dashboard_failed=1; fi
    elif [[ -e "${DASHBOARD_DST}" ]]; then
        rm -rf "${DASHBOARD_DST}" || dashboard_failed=1
    fi
    [[ ${dashboard_failed} -eq 0 ]] || failed=1

    if [[ "${PROCESS_STOPPED}" == "true" && ${failed} -eq 0 ]]; then
        if [[ "${ACP_ROLLBACK_FAULT:-}" == "restart" ]]; then failed=1
        else bash -lc "${RESTART_COMMAND}" || failed=1
        fi
    fi
    if [[ ${failed} -eq 0 ]]; then
        if [[ "${ACP_ROLLBACK_FAULT:-}" == "health" ]]; then failed=1
        else check_health || failed=1
        fi
    fi

    if [[ ${failed} -eq 0 ]]; then
        echo "UPGRADE_FAILED_ROLLBACK_SUCCEEDED: previous installation restored and verified" >&2
        exit "${original_status}"
    fi
    echo "UPGRADE_FAILED_ROLLBACK_FAILED: rollback state and backup evidence preserved at ${ROLLBACK_STATE}" >&2
    exit 70
}

if [[ "${RECOVER_ONLY}" == "true" ]]; then
    [[ -f "${BACKUP}" ]] || { echo "Error: no binary backup is available" >&2; exit 1; }
    load_rollback_state
    [[ "$(sha256_path "${BACKUP}")" == "${OLD_DIGEST}" ]] || {
        echo "Error: binary backup differs from rollback state" >&2; exit 1;
    }
    if [[ "${OLD_PROCESS_MANAGED}" == "true" ]]; then
        [[ -n "${RESTART_COMMAND}" ]] || {
            echo "Error: managed-process recovery requires the recorded restart hooks" >&2; exit 1;
        }
        PROCESS_STOPPED=true
    fi
    rollback 1
fi

[[ -f "${TARGET}" ]] || { echo "Error: no existing installation at ${TARGET}" >&2; exit 1; }
verify_release_evidence() {
    if [[ "${DEVELOPMENT}" == "true" ]]; then
        echo "  Explicit development upgrade: production provenance is not claimed."
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
        --artifact "${ARTIFACT_PATH}" --sbom "${SBOM_PATH}" \
        --manifest "${MANIFEST_PATH}" --slsa-bundle "${SLSA_BUNDLE_PATH}" \
        --spdx-bundle "${SPDX_BUNDLE_PATH}" --manifest-bundle "${MANIFEST_BUNDLE_PATH}" \
        --mode production >/dev/null
}

verify_release_evidence
[[ -s "${BINARY}" ]] || { echo "Error: non-empty new binary is missing" >&2; exit 1; }
mkdir -p "${BIN_DIR}" "${DATA_DIR}"
install -m 0755 "${BINARY}" "${BINARY_STAGE}"
DASHBOARD_SRC="${SCRIPT_DIR}/dashboard"
[[ -d "${DASHBOARD_SRC}" ]] || DASHBOARD_SRC="${REPO_ROOT}/dashboard/out"
if [[ -d "${DASHBOARD_SRC}" ]]; then
    rm -rf "${DASHBOARD_STAGE}"
    mkdir -p "${DASHBOARD_STAGE}"
    cp -a "${DASHBOARD_SRC}/." "${DASHBOARD_STAGE}/"
fi

OLD_DIGEST="$(sha256_path "${TARGET}")"
cp -p "${TARGET}" "${BACKUP}.new.${TXN}"
if [[ -d "${DASHBOARD_DST}" ]]; then
    rm -rf "${DATA_BACKUP}.new.${TXN}"
    cp -a "${DASHBOARD_DST}" "${DATA_BACKUP}.new.${TXN}"
    OLD_DASHBOARD_EXISTED=true
fi
[[ -n "${RESTART_COMMAND}" ]] && OLD_PROCESS_MANAGED=true
STATE_STAGE="${ROLLBACK_STATE}.new.${TXN}"
printf 'schema=upgrade_rollback.v1\nold_digest=%s\ndashboard=%s\nprocess=%s\n' \
    "${OLD_DIGEST}" \
    "$([[ "${OLD_DASHBOARD_EXISTED}" == "true" ]] && printf present || printf absent)" \
    "$([[ "${OLD_PROCESS_MANAGED}" == "true" ]] && printf managed || printf unmanaged)" \
    > "${STATE_STAGE}"
chmod 0600 "${STATE_STAGE}"
mv -f "${BACKUP}.new.${TXN}" "${BACKUP}"
rm -rf "${DATA_BACKUP}"
if [[ "${OLD_DASHBOARD_EXISTED}" == "true" ]]; then
    mv "${DATA_BACKUP}.new.${TXN}" "${DATA_BACKUP}"
fi
mv -f "${STATE_STAGE}" "${ROLLBACK_STATE}"

trap 'rollback $?' ERR
trap 'rollback 130' INT TERM
if [[ -n "${STOP_COMMAND}" ]]; then
    PROCESS_STOPPED=true
    bash -lc "${STOP_COMMAND}"
fi
if [[ "${ACP_UPGRADE_FAULT:-}" == "binary_move" ]]; then false; fi
mv -f "${BINARY_STAGE}" "${TARGET}"
if [[ "${ACP_UPGRADE_FAULT:-}" == "after_binary" ]]; then false; fi
if [[ "${ACP_UPGRADE_FAULT:-}" == "interrupt_after_binary" ]]; then kill -TERM "$$"; fi
if [[ -d "${DASHBOARD_STAGE}" ]]; then
    if [[ "${ACP_UPGRADE_FAULT:-}" == "dashboard_move" ]]; then false; fi
    rm -rf "${DASHBOARD_DST}.old.${TXN}"
    if [[ -e "${DASHBOARD_DST}" ]]; then mv "${DASHBOARD_DST}" "${DASHBOARD_DST}.old.${TXN}"; fi
    mv "${DASHBOARD_STAGE}" "${DASHBOARD_DST}"
    if [[ "${ACP_UPGRADE_FAULT:-}" == "after_dashboard" ]]; then false; fi
    if [[ "${ACP_UPGRADE_FAULT:-}" == "interrupt_after_dashboard" ]]; then kill -TERM "$$"; fi
fi
if [[ -n "${RESTART_COMMAND}" ]]; then
    if [[ "${ACP_UPGRADE_FAULT:-}" == "restart" ]]; then false; fi
    bash -lc "${RESTART_COMMAND}"
fi
if [[ "${ACP_UPGRADE_FAULT:-}" == "health" ]]; then false; fi
check_health
PROCESS_STOPPED=false

trap - ERR INT TERM
rm -rf "${DASHBOARD_DST}.old.${TXN}"
echo "Upgrade complete. Rollback state and applicable backup evidence remain at ${ROLLBACK_STATE}."
