#!/usr/bin/env bash
set -euo pipefail

# Agent Control Plane — Release Packager
# Builds release binary + dashboard, assembles tarball.
# Usage: ./package-release.sh [version]

VERSION="${1:-0.1.0}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ARTIFACT_NAME="agent-control-plane-v${VERSION}-linux-x86_64"
DIST_DIR="${REPO_ROOT}/dist"
STAGE_DIR="${DIST_DIR}/${ARTIFACT_NAME}"

echo "Agent Control Plane — Release Packager v${VERSION}"
echo ""

# Step 1: Build release binary
echo "Building release binary..."
cd "${REPO_ROOT}"
cargo build --release -p engine 2>&1 | tail -3
BINARY="${REPO_ROOT}/target/release/engine"
if [[ ! -f "${BINARY}" ]]; then
    echo "Error: release binary not found after build"
    exit 1
fi
echo "  Binary: $(stat -c%s "${BINARY}") bytes"

# Step 2: Build static dashboard
echo "Building static dashboard..."
cd "${REPO_ROOT}/dashboard"
if command -v pnpm &>/dev/null; then
    pnpm build:static 2>&1 | tail -3
else
    echo "  Warning: pnpm not found, skipping dashboard build"
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
    cp -r "${DASHBOARD_OUT}/"* "${STAGE_DIR}/dashboard/" 2>/dev/null || true
fi

cp "${REPO_ROOT}/.env.example" "${STAGE_DIR}/.env.example"
cp "${REPO_ROOT}/scripts/install.sh" "${STAGE_DIR}/install.sh"
cp "${REPO_ROOT}/scripts/upgrade.sh" "${STAGE_DIR}/upgrade.sh"
chmod +x "${STAGE_DIR}/install.sh" "${STAGE_DIR}/upgrade.sh"

# Create README for the release
cat > "${STAGE_DIR}/README.md" << 'READMEEOF'
# Agent Control Plane

Local deterministic agent control plane for studying token-efficient agent workflows.

## Quick Start

```bash
# Install
./install.sh

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
# Place new release files in the same directory, then:
./upgrade.sh
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

echo ""
echo "Release artifact ready:"
echo "  Path:    ${TARBALL}"
echo "  Size:    ${TARBALL_SIZE} bytes"
echo "  SHA256:  ${SHA256}"
echo ""
echo "Contents:"
ls -la "${STAGE_DIR}/"
