#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/release.yml"
INSTALLER="${ROOT}/scripts/install-from-release.sh"
UPGRADER="${ROOT}/scripts/upgrade.sh"

grep -Fq 'PACKAGE="${{ env.PACKAGE_PREFIX }}-${GITHUB_REF_NAME}-${{ matrix.target }}"' "${WORKFLOW}"
grep -Fq 'agent-control-plane-${version}-${target}.tar.gz' "${INSTALLER}"
grep -Fq 'target="${arch}-${os}"' "${INSTALLER}"
grep -Fq 'echo "unknown-linux-gnu"' "${INSTALLER}"
grep -Fq 'echo "apple-darwin"' "${INSTALLER}"
grep -Fq '"${extracted_dir}/engine"' "${INSTALLER}"
grep -Fq 'checksum_url="${tarball_url}.sha256"' "${INSTALLER}"
grep -Fq 'sha256sum -c "${archive_name}.sha256"' "${INSTALLER}"
grep -Fq 'shasum -a 256 -c "${archive_name}.sha256"' "${INSTALLER}"
grep -Fq 'shasum -a 256' "${WORKFLOW}"
grep -Fq 'contents: read' "${WORKFLOW}"
grep -Fq 'needs: verify' "${WORKFLOW}"
grep -Fq '      contents: write' "${WORKFLOW}"
grep -Fq '88f49ff79e777bef6d3564531636ee4d3cc2f8d2' "${WORKFLOW}"
grep -Fq 'target/${TARGET}/release/agent-control-plane' "${ROOT}/scripts/package-release.sh"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT
PREFIX="${TMP_DIR}/prefix"
DATA_DIR="${TMP_DIR}/data"
RELEASE_DIR="${TMP_DIR}/release"
mkdir -p "${PREFIX}/bin" "${DATA_DIR}/dashboard" "${RELEASE_DIR}/dashboard"
printf '#!/usr/bin/env bash\necho old\n' > "${PREFIX}/bin/agent-control-plane"
printf '#!/usr/bin/env bash\necho new\n' > "${RELEASE_DIR}/engine"
printf 'new-dashboard\n' > "${RELEASE_DIR}/dashboard/index.html"
printf 'stale-dashboard\n' > "${DATA_DIR}/dashboard/stale.js"
chmod +x "${PREFIX}/bin/agent-control-plane" "${RELEASE_DIR}/engine"
cp "${UPGRADER}" "${RELEASE_DIR}/upgrade.sh"

"${RELEASE_DIR}/upgrade.sh" --prefix "${PREFIX}" --data-dir "${DATA_DIR}" >/dev/null
grep -Fq 'echo new' "${PREFIX}/bin/agent-control-plane"
grep -Fq 'echo old' "${PREFIX}/bin/agent-control-plane.bak"
grep -Fq 'new-dashboard' "${DATA_DIR}/dashboard/index.html"
test ! -e "${DATA_DIR}/dashboard/stale.js"

BEFORE_ROLLBACK_SHA="$(sha256sum "${PREFIX}/bin/agent-control-plane" | cut -d' ' -f1)"
printf '#!/usr/bin/env bash\necho broken-upgrade\n' > "${RELEASE_DIR}/engine"
chmod +x "${RELEASE_DIR}/engine"
if "${RELEASE_DIR}/upgrade.sh" \
    --prefix "${PREFIX}" \
    --data-dir "${DATA_DIR}" \
    --stop-command true \
    --restart-command false >/dev/null 2>&1; then
    echo "upgrade rollback test unexpectedly succeeded" >&2
    exit 1
fi
AFTER_ROLLBACK_SHA="$(sha256sum "${PREFIX}/bin/agent-control-plane" | cut -d' ' -f1)"
test "${BEFORE_ROLLBACK_SHA}" = "${AFTER_ROLLBACK_SHA}"

FRESH_PREFIX="${TMP_DIR}/fresh-prefix"
FRESH_DATA_DIR="${TMP_DIR}/fresh-data"
mkdir -p "${FRESH_PREFIX}/bin" "${FRESH_DATA_DIR}"
printf '#!/usr/bin/env bash\necho fresh-old\n' > "${FRESH_PREFIX}/bin/agent-control-plane"
chmod +x "${FRESH_PREFIX}/bin/agent-control-plane"
if "${RELEASE_DIR}/upgrade.sh" \
    --prefix "${FRESH_PREFIX}" \
    --data-dir "${FRESH_DATA_DIR}" \
    --stop-command true \
    --restart-command false >/dev/null 2>&1; then
    echo "fresh-dashboard rollback test unexpectedly succeeded" >&2
    exit 1
fi
grep -Fq 'echo fresh-old' "${FRESH_PREFIX}/bin/agent-control-plane"
test ! -e "${FRESH_DATA_DIR}/dashboard"

echo "release contract ok"
