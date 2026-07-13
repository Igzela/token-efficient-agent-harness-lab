#!/usr/bin/env bash
set -euo pipefail

# Agent Control Plane — Curl Installer
# Detects OS/arch, downloads the latest release from GitHub, and installs.
# Usage: curl -fsSL https://raw.githubusercontent.com/Igzela/token-efficient-agent-harness-lab/main/scripts/install-from-release.sh | bash

REPO="Igzela/token-efficient-agent-harness-lab"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
DATA_DIR="${HOME}/.agent-control-plane"

detect_os() {
    local os
    os="$(uname -s)"
    case "${os}" in
        Linux*)  echo "unknown-linux-gnu" ;;
        Darwin*) echo "apple-darwin" ;;
        *)       echo "unsupported" ;;
    esac
}

detect_arch() {
    local arch
    arch="$(uname -m)"
    case "${arch}" in
        x86_64|amd64)   echo "x86_64" ;;
        aarch64|arm64)   echo "aarch64" ;;
        *)               echo "unsupported" ;;
    esac
}

main() {
    echo "Agent Control Plane — Installer"
    echo ""

    local os arch target version archive_name tarball_url checksum_url sbom_url attestation_url provenance_url tmp_dir
    os="$(detect_os)"
    arch="$(detect_arch)"

    if [[ "${os}" == "unsupported" ]]; then
        echo "Error: unsupported OS '$(uname -s)'"
        echo "Supported: Linux, macOS"
        exit 1
    fi
    if [[ "${arch}" == "unsupported" ]]; then
        echo "Error: unsupported architecture '$(uname -m)'"
        echo "Supported: x86_64, aarch64"
        exit 1
    fi

    target="${arch}-${os}"
    echo "  OS:   ${os}"
    echo "  Arch: ${arch}"
    echo "  Target: ${target}"
    echo ""

    version="${VERSION:-}"
    if [[ -z "${version}" ]]; then
        echo "Fetching latest release..."
        version="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')"
    fi
    if [[ -z "${version}" ]]; then
        echo "Error: could not determine latest release"
        echo "Tip: you can set VERSION manually:"
        echo "  VERSION=v0.1.0 bash $0"
        exit 1
    fi
    if [[ ! "${version}" =~ ^v[0-9][0-9A-Za-z._-]*$ ]]; then
        echo "Error: release version is not a safe signed tag: ${version}" >&2
        exit 1
    fi
    echo "  Version: ${version}"

    archive_name="agent-control-plane-${version}-${target}.tar.gz"
    tarball_url="https://github.com/${REPO}/releases/download/${version}/${archive_name}"
    checksum_url="${tarball_url}.sha256"
    sbom_url="${tarball_url}.spdx.json"
    attestation_url="${tarball_url}.attestation.json"
    provenance_url="${tarball_url}.provenance.json"
    echo "  URL: ${tarball_url}"
    echo ""

    # Download and extract
    tmp_dir="$(mktemp -d)"
    trap "rm -rf '${tmp_dir}'" EXIT

    echo "Downloading..."
    curl -fsSL "${tarball_url}" -o "${tmp_dir}/${archive_name}"
    curl -fsSL "${checksum_url}" -o "${tmp_dir}/${archive_name}.sha256"
    curl -fsSL "${sbom_url}" -o "${tmp_dir}/${archive_name}.spdx.json"
    curl -fsSL "${attestation_url}" -o "${tmp_dir}/${archive_name}.attestation.json"
    curl -fsSL "${provenance_url}" -o "${tmp_dir}/${archive_name}.provenance.json"

    echo "Verifying checksum..."
    (
        cd "${tmp_dir}"
        if command -v sha256sum >/dev/null 2>&1; then
            sha256sum -c "${archive_name}.sha256"
        elif command -v shasum >/dev/null 2>&1; then
            shasum -a 256 -c "${archive_name}.sha256"
        else
            echo "Error: sha256sum or shasum is required to verify the release"
            exit 1
        fi
    )

    # GitHub's verifier is the production attestation authority.  A missing
    # CLI or failed verification is unsupported/failure, never a warning-only
    # continuation.  This command is read-only and does not publish anything.
    if ! command -v gh >/dev/null 2>&1; then
        echo "Error: GitHub CLI with artifact-attestation support is required; refusing activation." >&2
        exit 1
    fi
    if ! command -v python3 >/dev/null 2>&1; then
        echo "Error: Python 3 is required to bind the verification transcript; refusing activation." >&2
        exit 1
    fi
    external_verification="${tmp_dir}/${archive_name}.external-verification.json"
    external_provenance="${tmp_dir}/${archive_name}.external-provenance.json"
    external_sbom="${tmp_dir}/${archive_name}.external-sbom.json"
    if ! gh attestation verify "${tmp_dir}/${archive_name}" \
        --repo "${REPO}" \
        --signer-repo "${REPO}" \
        --signer-workflow "${REPO}/.github/workflows/release.yml" \
        --source-ref "refs/tags/${version}" \
        --cert-oidc-issuer "https://token.actions.githubusercontent.com" \
        --deny-self-hosted-runners \
        --format json > "${external_provenance}"; then
        echo "Error: external artifact attestation verification failed; refusing activation." >&2
        exit 1
    fi
    if ! gh attestation verify "${tmp_dir}/${archive_name}" \
        --repo "${REPO}" \
        --signer-repo "${REPO}" \
        --signer-workflow "${REPO}/.github/workflows/release.yml" \
        --source-ref "refs/tags/${version}" \
        --cert-oidc-issuer "https://token.actions.githubusercontent.com" \
        --deny-self-hosted-runners \
        --predicate-type "https://spdx.dev/Document/v2.3" \
        --format json > "${external_sbom}"; then
        echo "Error: external SPDX SBOM attestation verification failed; refusing activation." >&2
        exit 1
    fi
    python3 -c 'import json, sys; json.dump({"provenance": json.load(open(sys.argv[1], encoding="utf-8")), "sbom": json.load(open(sys.argv[2], encoding="utf-8"))}, open(sys.argv[3], "w", encoding="utf-8"), sort_keys=True, separators=(",", ":"))' \
        "${external_provenance}" "${external_sbom}" "${external_verification}"

    echo "Checking archive members..."
    python3 - "${tmp_dir}/${archive_name}" "agent-control-plane-${version}-${target}" "${tmp_dir}" <<'PY'
import sys
import tarfile
from pathlib import Path, PurePosixPath

archive = Path(sys.argv[1])
expected_root = sys.argv[2]
extract_root = Path(sys.argv[3]).resolve()

with tarfile.open(archive, "r:gz") as bundle:
    members = bundle.getmembers()
    if not members:
        raise SystemExit("release archive is empty")
    for member in members:
        name = PurePosixPath(member.name)
        if (
            not member.name
            or member.name.startswith("/")
            or "\\" in member.name
            or ".." in name.parts
            or not name.parts
            or name.parts[0] != expected_root
            or not (member.isdir() or member.isreg())
        ):
            raise SystemExit(f"unsafe release archive member: {member.name!r}")
        destination = (extract_root.joinpath(*name.parts)).resolve()
        if extract_root not in destination.parents:
            raise SystemExit(f"release archive member escapes temporary root: {member.name!r}")
PY
    echo "  Archive member contract passed."
    echo "Extracting..."
    tar -xzf "${tmp_dir}/${archive_name}" -C "${tmp_dir}"

    local extracted_dir="${tmp_dir}/agent-control-plane-${version}-${target}"
    if [[ ! -d "${extracted_dir}" ]]; then
        echo "Error: could not find extracted directory"
        exit 1
    fi

    verifier="${extracted_dir}/release_provenance.py"
    if [[ ! -f "${verifier}" ]]; then
        echo "Error: release provenance verifier missing from the signed bundle; refusing activation." >&2
        exit 1
    fi
    verification="${tmp_dir}/${archive_name}.verification.json"
    python3 "${verifier}" verify \
        --artifact "${tmp_dir}/${archive_name}" \
        --sbom "${tmp_dir}/${archive_name}.spdx.json" \
        --attestation "${tmp_dir}/${archive_name}.attestation.json" \
        --provenance "${tmp_dir}/${archive_name}.provenance.json" \
        --mode production \
        --external-verification "${external_verification}" \
        --output "${verification}" >/dev/null

    # Route activation through the existing atomic owners.  An existing
    # installation is never overwritten directly; upgrade.sh retains the
    # prior binary/dashboard until health verification succeeds.
    if [[ -e "${INSTALL_DIR}/agent-control-plane" ]]; then
        echo "Upgrading existing installation through upgrade.sh..."
        sudo env ACP_DATA_DIR="${DATA_DIR}" bash "${extracted_dir}/upgrade.sh" \
            --prefix "${INSTALL_DIR}" \
            --data-dir "${DATA_DIR}" \
            --artifact "${tmp_dir}/${archive_name}" \
            --sbom "${tmp_dir}/${archive_name}.spdx.json" \
            --attestation "${tmp_dir}/${archive_name}.attestation.json" \
            --provenance "${tmp_dir}/${archive_name}.provenance.json" \
            --external-verification "${external_verification}"
    else
        echo "Installing new release through install.sh..."
        ACP_DATA_DIR="${DATA_DIR}" bash "${extracted_dir}/install.sh" \
            --prefix "${INSTALL_DIR}" \
            --data-dir "${DATA_DIR}" \
            --artifact "${tmp_dir}/${archive_name}" \
            --sbom "${tmp_dir}/${archive_name}.spdx.json" \
            --attestation "${tmp_dir}/${archive_name}.attestation.json" \
            --provenance "${tmp_dir}/${archive_name}.provenance.json" \
            --external-verification "${external_verification}"
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
}

main "$@"
