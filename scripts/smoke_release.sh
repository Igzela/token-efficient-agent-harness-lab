#!/usr/bin/env bash
set -euo pipefail

# Agent Control Plane — Release Smoke Test
# Extracts tarball to temp dir, installs, starts engine, verifies endpoints, tears down.
# Usage: ./smoke_release.sh [version]

VERSION="${1:-0.1.0}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ARTIFACT_NAME="agent-control-plane-v${VERSION}-linux-x86_64"
TARBALL="${REPO_ROOT}/dist/${ARTIFACT_NAME}.tar.gz"

echo "Agent Control Plane — Release Smoke Test v${VERSION}"
echo ""

# Check tarball exists
if [[ ! -f "${TARBALL}" ]]; then
    echo "Error: tarball not found at ${TARBALL}"
    echo "Run ./package-release.sh ${VERSION} first."
    exit 1
fi

# Create temp directory for smoke test
SMOKE_DIR=$(mktemp -d)
trap 'rm -rf "${SMOKE_DIR}"' EXIT

echo "Extracting to ${SMOKE_DIR}..."
cd "${SMOKE_DIR}"
tar xzf "${TARBALL}"

RELEASE_DIR="${SMOKE_DIR}/${ARTIFACT_NAME}"
if [[ ! -f "${RELEASE_DIR}/engine" ]]; then
    echo "Error: engine binary not found in extracted release"
    exit 1
fi

# Set up paths
PORT=$(shuf -i 10000-60000 -n 1)
DB_PATH="${SMOKE_DIR}/test.db"
BACKUP_DIR="${SMOKE_DIR}/backups"
DASHBOARD_DIR="${RELEASE_DIR}/dashboard"
ENGINE="${RELEASE_DIR}/engine"

mkdir -p "${BACKUP_DIR}"

# Set env vars BEFORE starting engine (main.rs reads them at startup)
export HOST=127.0.0.1
export PORT
export ACP_DB_PATH="${DB_PATH}"
export ACP_BACKUP_DIR="${BACKUP_DIR}"
export ACP_DASHBOARD_DIR="${DASHBOARD_DIR}"

echo "Starting engine on port ${PORT}..."
"${ENGINE}" &
ENGINE_PID=$!
trap 'kill ${ENGINE_PID} 2>/dev/null; rm -rf "${SMOKE_DIR}"' EXIT

# Wait for engine to be ready
echo "Waiting for engine to be ready..."
for i in $(seq 1 30); do
    if curl -sf "http://127.0.0.1:${PORT}/api/v1/health" >/dev/null 2>&1; then
        echo "  Engine ready after ${i}s"
        break
    fi
    if ! kill -0 ${ENGINE_PID} 2>/dev/null; then
        echo "Error: engine process died"
        exit 1
    fi
    sleep 1
done

PASS=0
FAIL=0

check() {
    local name="$1"
    local url="$2"
    local expect="$3"
    local code
    code=$(curl -sf -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}${url}" 2>/dev/null || echo "000")
    if [[ "${code}" == "${expect}" ]]; then
        echo "  PASS  ${name} (${code})"
        PASS=$((PASS + 1))
    else
        echo "  FAIL  ${name} (expected ${expect}, got ${code})"
        FAIL=$((FAIL + 1))
    fi
}

check_body() {
    local name="$1"
    local url="$2"
    local needle="$3"
    local body
    body=$(curl -sf "http://127.0.0.1:${PORT}${url}" 2>/dev/null || echo "")
    if echo "${body}" | grep -q "${needle}"; then
        echo "  PASS  ${name}"
        PASS=$((PASS + 1))
    else
        echo "  FAIL  ${name} (body missing '${needle}')"
        FAIL=$((FAIL + 1))
    fi
}

echo ""
echo "Running smoke checks..."

check "Health endpoint" "/api/v1/health" "200"
check_body "Health body" "/api/v1/health" "status"
check "Readiness endpoint" "/api/v1/ready" "200"
check "OpenAPI endpoint" "/api/v1/openapi.json" "200"
check_body "Dashboard root" "/" "Agent Control Plane"

# Dispatch check
DISPATCH_CODE=$(curl -sf -o /dev/null -w "%{http_code}" -X POST \
    -H "Content-Type: application/json" \
    -d '{"raw_request":"smoke test task","request_source":"smoke"}' \
    "http://127.0.0.1:${PORT}/api/v1/dispatch" 2>/dev/null || echo "000")
if [[ "${DISPATCH_CODE}" == "200" ]]; then
    echo "  PASS  Dispatch endpoint (${DISPATCH_CODE})"
    PASS=$((PASS + 1))
else
    echo "  FAIL  Dispatch endpoint (expected 200, got ${DISPATCH_CODE})"
    FAIL=$((FAIL + 1))
fi

echo ""
echo "Results: ${PASS} passed, ${FAIL} failed"

if [[ ${FAIL} -gt 0 ]]; then
    echo "SMOKE TEST FAILED"
    exit 1
fi
echo "SMOKE TEST PASSED"
