#!/usr/bin/env bash
# Usable local harness entry — CLI and Linux desktop.
#
# Usage:
#   ./scripts/harness.sh              # start operator desktop (engine + UI)
#   ./scripts/harness.sh start        # same
#   ./scripts/harness.sh work         # finish one real local-folder job
#   ./scripts/harness.sh demo         # no-provider evidence demo
#   ./scripts/harness.sh doctor       # local readiness checks
#   ./scripts/harness.sh self-test    # full provider-free matrix (+ live if gated)
#   ./scripts/harness.sh stop         # stop desktop/work/demo sessions
#   ./scripts/harness.sh install-desktop   # install a Linux .desktop launcher
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

CMD="${1:-start}"
shift || true

ensure_engine() {
  local bin="${ROOT}/target/debug/agent-control-plane"
  if [[ -x "${ROOT}/target/release/agent-control-plane" ]]; then
    bin="${ROOT}/target/release/agent-control-plane"
  fi
  if [[ ! -x "${bin}" ]]; then
    echo "Building engine (cargo build -p engine)…"
    cargo build -p engine
  fi
}

ensure_dashboard() {
  if [[ -f "${ROOT}/dashboard/out/index.html" ]]; then
    return 0
  fi
  if ! command -v bun >/dev/null 2>&1; then
    echo "bun is required to build the operator dashboard." >&2
    echo "Install: https://bun.sh/  then re-run ./scripts/harness.sh start" >&2
    exit 2
  fi
  echo "Building static dashboard…"
  (cd dashboard && bun install --frozen-lockfile && bun run build:static)
}

case "${CMD}" in
  start|desktop|ui)
    ensure_engine
    ensure_dashboard
    exec uv run --no-project python scripts/start_operator_desktop.py "$@"
    ;;
  work)
    ensure_engine
    if command -v bun >/dev/null 2>&1 && [[ ! -f "${ROOT}/dashboard/out/index.html" ]]; then
      ensure_dashboard
    fi
    exec ./scripts/start_local_work.sh "$@"
    ;;
  demo)
    ensure_engine
    if command -v bun >/dev/null 2>&1 && [[ ! -f "${ROOT}/dashboard/out/index.html" ]]; then
      ensure_dashboard
    fi
    exec ./scripts/demo.sh "$@"
    ;;
  doctor)
    exec uv run --no-project python scripts/acp_local_doctor.py "$@"
    ;;
  self-test|selftest)
    ensure_engine
    exec uv run --no-project python scripts/harness_self_test.py "$@"
    ;;
  stop|cleanup)
    uv run --no-project python scripts/start_operator_desktop.py --cleanup || true
    uv run --no-project python scripts/start_local_work.py --cleanup || true
    uv run --no-project python scripts/demo_no_provider.py --cleanup || true
    echo "All local harness sessions stopped."
    ;;
  install-desktop)
    APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
    mkdir -p "${APP_DIR}"
    DESKTOP="${APP_DIR}/agent-control-plane.desktop"
    ICON_SRC="${ROOT}/dashboard/src/app/icon.svg"
    ICON_DST="${XDG_DATA_HOME:-$HOME/.local/share}/icons/agent-control-plane.svg"
    mkdir -p "$(dirname "${ICON_DST}")"
    if [[ -f "${ICON_SRC}" ]]; then
      cp -f "${ICON_SRC}" "${ICON_DST}"
    fi
    cat > "${DESKTOP}" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=Agent Control Plane
Comment=Local operator harness dashboard
Exec=${ROOT}/scripts/harness.sh start
Icon=${ICON_DST}
Terminal=true
Categories=Development;Utility;
StartupNotify=true
Path=${ROOT}
EOF
    chmod 644 "${DESKTOP}"
    if command -v update-desktop-database >/dev/null 2>&1; then
      update-desktop-database "${APP_DIR}" >/dev/null 2>&1 || true
    fi
    echo "Installed ${DESKTOP}"
    echo "Launch from your app menu as “Agent Control Plane”, or run: ./scripts/harness.sh start"
    ;;
  -h|--help|help)
    sed -n '2,12p' "$0"
    ;;
  *)
    echo "Unknown command: ${CMD}" >&2
    echo "Usage: $0 [start|work|demo|doctor|self-test|stop|install-desktop]" >&2
    exit 2
    ;;
esac
