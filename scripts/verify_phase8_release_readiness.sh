#!/usr/bin/env bash
set -euo pipefail

# Phase 8 release readiness verification.
# Checks all release artifacts, scripts, Dockerfiles, and CI workflows are present.

FAIL=0
check() {
    if [ -e "$1" ]; then
        echo "  OK   $1"
    else
        echo "  FAIL $1"
        FAIL=1
    fi
}

check_exec() {
    if [ -x "$1" ]; then
        echo "  OK   $1 (executable)"
    else
        echo "  FAIL $1 (not executable)"
        FAIL=1
    fi
}

echo "=== Release Scripts ==="
check_exec scripts/install.sh
check_exec scripts/upgrade.sh
check_exec scripts/package-release.sh
check_exec scripts/smoke_release.sh
check_exec scripts/smoke_native_runtime.py
check_exec scripts/ga_release_checklist.py
check_exec scripts/ga_rollback_drill.py
check_exec scripts/acp_restore_smoke.py
check_exec scripts/install-from-release.sh

echo ""
echo "=== Docker ==="
check docker-compose.yml
check deploy/Dockerfile.engine
check deploy/Dockerfile.dashboard
check deploy/Dockerfile.combined

echo ""
echo "=== CI Workflows ==="
check .github/workflows/tests.yml
check .github/workflows/release.yml

echo ""
echo "=== Dashboard ==="
check dashboard/package.json
check dashboard/next.config.ts

echo ""
echo "=== Engine ==="
check engine/Cargo.toml
check engine/src/main.rs

echo ""
echo "=== Environment ==="
check .env.example

echo ""
echo "=== Docs ==="
check docs/CURRENT_STATUS.md
check docs/NEXT_DECISION.md
check docs/archive/phase-closeouts/PHASE8_FINAL_COMPLETION_PLAN.md
check docs/RUNBOOK.md

echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "ALL CHECKS PASSED"
else
    echo "SOME CHECKS FAILED"
    exit 1
fi
