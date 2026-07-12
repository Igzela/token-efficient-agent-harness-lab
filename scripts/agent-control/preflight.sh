#!/usr/bin/env bash
#
# Pre-flight check for the agent orchestrator.
#
# Runs on the Vader self-hosted runner to verify the environment
# is properly configured BEFORE attempting any Codex invocation.
#
# Reports only non-secret status information.
#
# Usage:
#   preflight.sh [--verbose]
#
# Exit code:
#   0 = all checks pass
#   1 = one or more checks fail

set -euo pipefail

VERBOSE=false
if [ "${1:-}" = "--verbose" ]; then
  VERBOSE=true
fi

PASS=0
FAIL=0

check() {
  local name="$1"
  shift
  if "$@" &>/dev/null; then
    echo "  [PASS] $name"
    PASS=$((PASS + 1))
  else
    echo "  [FAIL] $name"
    FAIL=$((FAIL + 1))
  fi
}

echo "=== Agent Orchestrator Preflight ==="
echo ""

echo "--- System ---"
check "whoami" whoami
check "HOME is set" test -n "${HOME:-}"
check "TMPDIR writable" touch /tmp/agent-preflight-test && rm -f /tmp/agent-preflight-test

echo ""
echo "--- Git ---"
check "git is installed" command -v git
check "git version" git --version

echo ""
echo "--- Python ---"
check "python3 is installed" command -v python3

echo ""
echo "--- GitHub CLI ---"
check "gh is installed" command -v gh
check "gh auth status" gh auth status
GH_SCOPES=$(gh auth status 2>&1 | grep "Token scopes" || true)
$VERBOSE && echo "  Scopes: $GH_SCOPES"

echo ""
echo "--- Codex CLI ---"
if command -v codex &>/dev/null; then
  echo "  [PASS] codex is installed"
  PASS=$((PASS + 1))
  CODEX_VER=$(codex --version 2>/dev/null || echo "unknown")
  echo "  Version: $CODEX_VER"

  if codex login status &>/dev/null; then
    echo "  [PASS] codex is authenticated"
    PASS=$((PASS + 1))
  else
    echo "  [FAIL] codex is not authenticated"
    FAIL=$((FAIL + 1))
  fi

  if [ -n "${CODEX_HOME:-}" ]; then
    echo "  CODEX_HOME=$CODEX_HOME"
  else
    echo "  CODEX_HOME not set (using default ~/.codex)"
  fi

  if [ -d "${CODEX_HOME:-$HOME/.codex}" ]; then
    echo "  [PASS] codex home directory exists"
    PASS=$((PASS + 1))
  else
    echo "  [FAIL] codex home directory not found"
    FAIL=$((FAIL + 1))
  fi
else
  echo "  [FAIL] codex is not installed"
  FAIL=$((FAIL + 1))
fi

echo ""
echo "--- Environment ---"
check "AGENT_ORCHESTRATOR_ENABLED is set" test -n "${AGENT_ORCHESTRATOR_ENABLED:-}"
if [ "${AGENT_ORCHESTRATOR_ENABLED:-false}" = "true" ]; then
  echo "  Orchestrator: ENABLED"
else
  echo "  Orchestrator: DISABLED (default)"
fi
check "AGENT_AUTO_MERGE_ENABLED is set" test -n "${AGENT_AUTO_MERGE_ENABLED:-}"
if [ "${AGENT_AUTO_MERGE_ENABLED:-false}" = "true" ]; then
  echo "  Auto-merge: ENABLED"
else
  echo "  Auto-merge: DISABLED (default)"
fi

echo ""
echo "--- Runner Labels ---"
if [ -n "${RUNNER_NAME:-}" ]; then
  echo "  Runner: $RUNNER_NAME"
else
  echo "  Runner: not running as GitHub Actions runner (stdalone mode)"
fi
if [ -n "${RUNNER_LABELS:-}" ]; then
  echo "  Labels: $RUNNER_LABELS"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
exit 0
