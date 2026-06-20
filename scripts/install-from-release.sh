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

    local os arch target version archive_name tarball_url checksum_url tmp_dir
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
    echo "  Version: ${version}"

    archive_name="agent-control-plane-${version}-${target}.tar.gz"
    tarball_url="https://github.com/${REPO}/releases/download/${version}/${archive_name}"
    checksum_url="${tarball_url}.sha256"
    echo "  URL: ${tarball_url}"
    echo ""

    # Download and extract
    tmp_dir="$(mktemp -d)"
    trap "rm -rf '${tmp_dir}'" EXIT

    echo "Downloading..."
    curl -fsSL "${tarball_url}" -o "${tmp_dir}/${archive_name}"
    curl -fsSL "${checksum_url}" -o "${tmp_dir}/${archive_name}.sha256"

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

    echo "Extracting..."
    tar -xzf "${tmp_dir}/${archive_name}" -C "${tmp_dir}"

    local extracted_dir="${tmp_dir}/agent-control-plane-${version}-${target}"
    if [[ ! -d "${extracted_dir}" ]]; then
        # Try alternate naming
        extracted_dir="$(find "${tmp_dir}" -maxdepth 1 -type d -name 'agent-control-plane-*' | head -1)"
    fi
    if [[ ! -d "${extracted_dir}" ]]; then
        echo "Error: could not find extracted directory"
        exit 1
    fi

    # Install binary
    echo "Installing binary to ${INSTALL_DIR}/..."
    sudo mkdir -p "${INSTALL_DIR}"
    sudo cp "${extracted_dir}/engine" "${INSTALL_DIR}/agent-control-plane"
    sudo chmod +x "${INSTALL_DIR}/agent-control-plane"
    echo "  -> ${INSTALL_DIR}/agent-control-plane"

    # Setup data directory
    echo "Setting up data directory..."
    mkdir -p "${DATA_DIR}/backups"

    # Install dashboard assets
    if [[ -d "${extracted_dir}/dashboard" ]]; then
        mkdir -p "${DATA_DIR}/dashboard"
        cp -r "${extracted_dir}/dashboard/"* "${DATA_DIR}/dashboard/" 2>/dev/null || true
        echo "  -> ${DATA_DIR}/dashboard/"
    fi

    # Install .env.example
    if [[ -f "${extracted_dir}/.env.example" ]] && [[ ! -f "${DATA_DIR}/.env.example" ]]; then
        cp "${extracted_dir}/.env.example" "${DATA_DIR}/.env.example"
        echo "  -> ${DATA_DIR}/.env.example"
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
