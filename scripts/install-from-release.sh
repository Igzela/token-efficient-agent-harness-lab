#!/usr/bin/env bash
set -Eeuo pipefail

# Immutable release bootstrap. Download and verify this exact release asset and
# its .slsa.bundle.json before execution; mutable branch URLs are unsupported.

REPO="Igzela/token-efficient-agent-harness-lab"
PREFIX="${PREFIX:-/usr/local}"
DATA_DIR="${ACP_DATA_DIR:-${HOME}/.agent-control-plane}"
VERSION="${VERSION:-}"
RELEASE_SOURCE_COMMIT="${RELEASE_SOURCE_COMMIT:-}"
BOOTSTRAP_BUNDLE="${BOOTSTRAP_BUNDLE:-}"
MAX_ARCHIVE_BYTES=$((256 * 1024 * 1024))
MAX_SIDECAR_BYTES=$((16 * 1024 * 1024))

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) [[ $# -ge 2 ]] || exit 2; VERSION="$2"; shift 2 ;;
        --source-commit) [[ $# -ge 2 ]] || exit 2; RELEASE_SOURCE_COMMIT="$2"; shift 2 ;;
        --bootstrap-bundle) [[ $# -ge 2 ]] || exit 2; BOOTSTRAP_BUNDLE="$2"; shift 2 ;;
        --prefix) [[ $# -ge 2 ]] || exit 2; PREFIX="$2"; shift 2 ;;
        --data-dir) [[ $# -ge 2 ]] || exit 2; DATA_DIR="$2"; shift 2 ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
done

[[ "${VERSION}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]] || {
    echo "Error: --version must be an exact immutable version tag" >&2
    exit 1
}
[[ "${RELEASE_SOURCE_COMMIT}" =~ ^[0-9a-f]{40}$ ]] || {
    echo "Error: --source-commit must be the exact release commit" >&2
    exit 1
}
[[ -f "${BOOTSTRAP_BUNDLE}" ]] || {
    echo "Error: --bootstrap-bundle must name the exact downloaded installer bundle" >&2
    exit 1
}
for command in curl gh python3; do
    command -v "${command}" >/dev/null 2>&1 || {
        echo "Error: ${command} is required before immutable bootstrap" >&2
        exit 1
    }
done

SCRIPT_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
[[ -f "${SCRIPT_PATH}" && "${SCRIPT_PATH}" != /dev/fd/* ]] || {
    echo "Error: installer must be a downloaded local file, never a pipe" >&2
    exit 1
}

verify_slsa_asset() {
    local asset="$1" bundle="$2"
    gh attestation verify "${asset}" \
        --bundle "${bundle}" \
        --predicate-type "https://slsa.dev/provenance/v1" \
        --repo "${REPO}" \
        --signer-workflow "${REPO}/.github/workflows/release.yml" \
        --source-ref "refs/tags/${VERSION}" \
        --source-digest "${RELEASE_SOURCE_COMMIT}" \
        --cert-oidc-issuer "https://token.actions.githubusercontent.com" \
        --deny-self-hosted-runners >/dev/null
}

# This is deliberately the first external action: reverify the exact local
# installer bytes that the operator verified before choosing to execute them.
verify_slsa_asset "${SCRIPT_PATH}" "${BOOTSTRAP_BUNDLE}"

download() {
    local url="$1" output="$2" maximum="$3"
    curl --fail --location --silent --show-error --max-filesize "${maximum}" \
        "${url}" --output "${output}"
    [[ -f "${output}" && "$(wc -c < "${output}")" -le "${maximum}" ]]
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT
base_url="https://github.com/${REPO}/releases/download/${VERSION}"
verifier="${tmp_dir}/release_provenance.py"
verifier_bundle="${tmp_dir}/release_provenance.py.slsa.bundle.json"
download "${base_url}/release_provenance.py" "${verifier}" "${MAX_SIDECAR_BYTES}"
download "${base_url}/release_provenance.py.slsa.bundle.json" "${verifier_bundle}" "${MAX_SIDECAR_BYTES}"
verify_slsa_asset "${verifier}" "${verifier_bundle}"

os="$(uname -s)"
case "${os}" in Linux*) os="unknown-linux-gnu" ;; Darwin*) os="apple-darwin" ;; *) echo "unsupported OS" >&2; exit 1 ;; esac
arch="$(uname -m)"
case "${arch}" in x86_64|amd64) arch="x86_64" ;; aarch64|arm64) arch="aarch64" ;; *) echo "unsupported architecture" >&2; exit 1 ;; esac
target="${arch}-${os}"
archive_name="agent-control-plane-${VERSION}-${target}.tar.gz"
artifact="${tmp_dir}/${archive_name}"
sbom="${artifact}.spdx.json"
manifest="${artifact}.release-manifest.json"
slsa_bundle="${artifact}.slsa.bundle.json"
spdx_bundle="${artifact}.spdx.bundle.json"
manifest_bundle="${artifact}.release-manifest.bundle.json"
checksum="${artifact}.sha256"

download "${base_url}/${archive_name}" "${artifact}" "${MAX_ARCHIVE_BYTES}"
for suffix in .sha256 .spdx.json .release-manifest.json .slsa.bundle.json \
    .spdx.bundle.json .release-manifest.bundle.json; do
    download "${base_url}/${archive_name}${suffix}" "${artifact}${suffix}" "${MAX_SIDECAR_BYTES}"
done

(
    cd "${tmp_dir}"
    if command -v sha256sum >/dev/null 2>&1; then sha256sum -c "${archive_name}.sha256"
    else shasum -a 256 -c "${archive_name}.sha256"
    fi
)

python3 "${verifier}" verify-release \
    --artifact "${artifact}" --sbom "${sbom}" --manifest "${manifest}" \
    --slsa-bundle "${slsa_bundle}" --spdx-bundle "${spdx_bundle}" \
    --manifest-bundle "${manifest_bundle}" --mode production >/dev/null
python3 "${verifier}" verify-bootstrap \
    --manifest "${manifest}" --source-commit "${RELEASE_SOURCE_COMMIT}" \
    --asset "install-from-release.sh=${SCRIPT_PATH}" \
    --asset "release_provenance.py=${verifier}"

# No archive header or byte is extracted until all exact local bundles and
# canonical signed predicates have passed verification.
expected_root="agent-control-plane-${VERSION}-${target}"
python3 "${verifier}" extract-archive \
    --archive "${artifact}" --destination "${tmp_dir}/extract" \
    --expected-top-level "${expected_root}" >/dev/null
release_dir="${tmp_dir}/extract/${expected_root}"

evidence_args=(
    --artifact "${artifact}" --sbom "${sbom}" --manifest "${manifest}"
    --slsa-bundle "${slsa_bundle}" --spdx-bundle "${spdx_bundle}"
    --manifest-bundle "${manifest_bundle}"
)
if [[ -e "${PREFIX}/bin/agent-control-plane" ]]; then
    bash "${release_dir}/upgrade.sh" --prefix "${PREFIX}" --data-dir "${DATA_DIR}" \
        "${evidence_args[@]}"
else
    bash "${release_dir}/install.sh" --prefix "${PREFIX}" --data-dir "${DATA_DIR}" \
        "${evidence_args[@]}"
fi
