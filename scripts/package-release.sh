#!/usr/bin/env bash
set -euo pipefail

# Agent Control Plane — Release Packager
# Builds release binary + dashboard, assembles tarball.
# Usage: ./package-release.sh [version]

VERSION="${1:-0.1.0}"
TAG="${VERSION#v}"
TAG="v${TAG}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET="${TARGET:-x86_64-unknown-linux-gnu}"
ARTIFACT_NAME="agent-control-plane-${TAG}-${TARGET}"
DIST_DIR="${REPO_ROOT}/dist"
STAGE_DIR="${DIST_DIR}/${ARTIFACT_NAME}"

echo "Agent Control Plane — Release Packager ${TAG}"
echo ""

# Step 1: Build release binary
echo "Building release binary..."
cd "${REPO_ROOT}"
HOST_TARGET="$(rustc -vV | sed -n 's/^host: //p')"
BUILD_TOOL="cargo"
if [[ "${TARGET}" != "${HOST_TARGET}" ]] && command -v cross >/dev/null 2>&1; then
    BUILD_TOOL="cross"
fi
"${BUILD_TOOL}" build --release -p engine --target "${TARGET}" 2>&1 | tail -3
BINARY="${REPO_ROOT}/target/${TARGET}/release/agent-control-plane"
if [[ ! -f "${BINARY}" ]]; then
    echo "Error: release binary not found after build"
    exit 1
fi
echo "  Binary: $(wc -c < "${BINARY}" | tr -d ' ') bytes"

# Step 2: Build static dashboard
echo "Building static dashboard..."
cd "${REPO_ROOT}/dashboard"
if command -v bun &>/dev/null; then
    bun run build:static 2>&1 | tail -3
else
    echo "  Warning: bun not found, skipping dashboard build"
    echo "  If dashboard/out/ exists, it will be included as-is"
fi
DASHBOARD_OUT="${REPO_ROOT}/dashboard/out"
if [[ ! -d "${DASHBOARD_OUT}" ]]; then
    echo "  Warning: dashboard/out/ not found, including without dashboard"
fi

# Step 3: Assemble release directory
echo "Assembling release artifact..."
rm -rf "${STAGE_DIR}"
mkdir -p "${STAGE_DIR}/dashboard"

cp "${BINARY}" "${STAGE_DIR}/engine"
chmod +x "${STAGE_DIR}/engine"

if [[ -d "${DASHBOARD_OUT}" ]]; then
    cp -a "${DASHBOARD_OUT}/." "${STAGE_DIR}/dashboard/"
fi

cp "${REPO_ROOT}/.env.example" "${STAGE_DIR}/.env.example"
cp "${REPO_ROOT}/scripts/install.sh" "${STAGE_DIR}/install.sh"
cp "${REPO_ROOT}/scripts/upgrade.sh" "${STAGE_DIR}/upgrade.sh"
cp "${REPO_ROOT}/scripts/release_provenance.py" "${STAGE_DIR}/release_provenance.py"
chmod +x "${STAGE_DIR}/install.sh" "${STAGE_DIR}/upgrade.sh"

# Create README for the release
cat > "${STAGE_DIR}/README.md" << 'READMEEOF'
# Agent Control Plane

Local deterministic agent control plane for studying token-efficient agent workflows.

## Quick Start

```bash
# Non-publishing local dry-run install (fixture evidence only)
./install.sh --development

# Run
agent-control-plane

# Run with dashboard
ACP_DASHBOARD_DIR=~/.agent-control-plane/dashboard agent-control-plane
```

## Configuration

Copy `.env.example` to your environment and adjust:

```bash
cp .env.example ~/.agent-control-plane/.env
```

See `.env.example` for all available options.

## Upgrade

```bash
# A production upgrade must be invoked by install-from-release.sh after
# external attestation verification.  A local dry-run is explicit:
./upgrade.sh --development
```

## Architecture

- **Engine**: Rust binary serving REST API on port 8080
- **Dashboard**: Static Next.js app served by the engine
- **Storage**: Local SQLite database at `~/.agent-control-plane/local-team.db`
- **No cloud, no external dependencies, no provider calls by default**

## Documentation

See the repository README for full architecture and API documentation.
READMEEOF

# Step 4: Create tarball
echo "Creating tarball..."
cd "${DIST_DIR}"
tar czf "${ARTIFACT_NAME}.tar.gz" "${ARTIFACT_NAME}/"
TARBALL="${DIST_DIR}/${ARTIFACT_NAME}.tar.gz"
TARBALL_SIZE=$(stat -c%s "${TARBALL}")
SHA256=$(sha256sum "${TARBALL}" | cut -d' ' -f1)
CHECKSUM_FILE="${TARBALL}.sha256"
printf '%s  %s\n' "${SHA256}" "$(basename "${TARBALL}")" > "${CHECKSUM_FILE}"

METADATA_FILE="${DIST_DIR}/${ARTIFACT_NAME}.metadata.json"
SBOM_FILE="${TARBALL}.spdx.json"
ATTESTATION_FILE="${TARBALL}.attestation.json"
PROVENANCE_FILE="${TARBALL}.provenance.json"
VERIFICATION_FILE="${TARBALL}.verification.json"
SOURCE_COMMIT="$(git -C "${REPO_ROOT}" rev-parse HEAD)"

echo "Generating deterministic dry-run release evidence..."
python3 "${REPO_ROOT}/scripts/release_provenance.py" write-metadata \
    --root "${REPO_ROOT}" \
    --output "${METADATA_FILE}" \
    --repository "$(git -C "${REPO_ROOT}" config --get remote.origin.url | sed -E 's#.*github.com[:/]##; s#\.git$##')" \
    --source-commit "${SOURCE_COMMIT}" \
    --ref "refs/tags/${TAG}" \
    --workflow ".github/workflows/release.yml" \
    --workflow-ref "local-dry-run/.github/workflows/release.yml@refs/tags/${TAG}" \
    --run-id "local-dry-run" \
    --job "local-package" \
    --builder-id "local-dry-run" \
    --target-os "linux" \
    --target-architecture "${TARGET%%-*}" \
    --target-triple "${TARGET}" \
    --package-kind "package" \
    --artifact-name "$(basename "${TARBALL}")" \
    --previous-known-good "${ACP_PREVIOUS_KNOWN_GOOD:-not-published-dry-run}" \
    --rollback-target "${ACP_PREVIOUS_KNOWN_GOOD:-not-published-dry-run}" \
    --publication-mode "dry-run" \
    --lockfile "Cargo.lock" \
    --lockfile "dashboard/bun.lock" \
    --lockfile "sdk/typescript/bun.lock" \
    --build-input "engine/Cargo.toml" \
    --build-input ".env.example" \
    --build-input "dashboard/package.json"
python3 "${REPO_ROOT}/scripts/release_provenance.py" create-sbom \
    --metadata "${METADATA_FILE}" \
    --artifact "${TARBALL}" \
    --output "${SBOM_FILE}"
python3 "${REPO_ROOT}/scripts/release_provenance.py" create-attestation \
    --metadata "${METADATA_FILE}" \
    --artifact "${TARBALL}" \
    --sbom "${SBOM_FILE}" \
    --output "${ATTESTATION_FILE}" \
    --identity fixture
python3 "${REPO_ROOT}/scripts/release_provenance.py" create-provenance \
    --metadata "${METADATA_FILE}" \
    --artifact "${TARBALL}" \
    --sbom "${SBOM_FILE}" \
    --attestation "${ATTESTATION_FILE}" \
    --output "${PROVENANCE_FILE}"
python3 "${REPO_ROOT}/scripts/release_provenance.py" verify \
    --artifact "${TARBALL}" \
    --sbom "${SBOM_FILE}" \
    --attestation "${ATTESTATION_FILE}" \
    --provenance "${PROVENANCE_FILE}" \
    --mode fixture \
    --output "${VERIFICATION_FILE}" >/dev/null

echo ""
echo "Release artifact ready:"
echo "  Path:    ${TARBALL}"
echo "  Size:    ${TARBALL_SIZE} bytes"
echo "  SHA256:  ${SHA256}"
echo "  SBOM:    ${SBOM_FILE}"
echo "  Provenance: ${PROVENANCE_FILE}"
echo "  Verification: ${VERIFICATION_FILE} (fixture; non-authoritative)"
echo ""
echo "Contents:"
ls -la "${STAGE_DIR}/"
