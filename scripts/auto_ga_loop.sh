#!/usr/bin/env bash
# auto_ga_loop.sh — Autonomous GA hardening batch loop
#
# Reads NEXT_DECISION.md to find the next incomplete GA batch,
# launches a Claude Code session to implement it, waits for CI,
# then repeats until all batches are done or one fails.
#
# Usage:
#   ./scripts/auto_ga_loop.sh [--max-batches N] [--skip-ci-wait]
#
# Requirements:
#   - claude CLI installed and authenticated
#   - gh CLI installed and authenticated
#   - Working tree clean before starting

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MAX_BATCHES="${MAX_BATCHES:-7}"
SKIP_CI_WAIT=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --max-batches) MAX_BATCHES="$2"; shift 2 ;;
        --skip-ci-wait) SKIP_CI_WAIT=true; shift ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

cd "$REPO_ROOT"

# Verify clean working tree
if [[ -n "$(git status --porcelain)" ]]; then
    echo "ERROR: Working tree is not clean. Commit or stash changes first."
    exit 1
fi

COMPLETED=0
FAILED=false

for i in $(seq 1 "$MAX_BATCHES"); do
    # Find next incomplete GA batch from NEXT_DECISION.md
    NEXT_BATCH=$(grep -oP 'GA-\K\d+(?=\. .* \| \*\*NEXT\*\*|\. .* \| Pending)' \
        docs/NEXT_DECISION.md | head -1)

    if [[ -z "$NEXT_BATCH" ]]; then
        echo "=== All GA batches complete or none found ==="
        break
    fi

    echo ""
    echo "============================================================"
    echo "  Starting GA-${NEXT_BATCH} (batch ${i}/${MAX_BATCHES})"
    echo "============================================================"
    echo ""

    # Launch Claude Code session
    PROMPT="You are continuing the GA hardening track for this repository.

Read docs/SESSION_START_HERE.md, docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md, and docs/MODULE_MAP.md.

Your task: implement GA-${NEXT_BATCH} as described in docs/NEXT_DECISION.md.

Requirements:
1. Implement the GA-${NEXT_BATCH} scope with tests
2. Run cargo test -p engine, cargo fmt --check, cargo clippy
3. Run uv run --no-project python scripts/check_agent_handoff.py
4. Update docs/CURRENT_STATUS.md and docs/NEXT_DECISION.md to reflect completion
5. Commit and push
6. Report: commit hash, test count, what was implemented, remaining risks

Do NOT start any work beyond GA-${NEXT_BATCH}. Stop after committing and pushing."

    if claude -p "$PROMPT" \
        --dangerously-skip-permissions \
        --output-format text \
        2>&1; then

        echo ""
        echo "  GA-${NEXT_BATCH} session completed successfully."
        COMPLETED=$((COMPLETED + 1))
    else
        echo ""
        echo "  GA-${NEXT_BATCH} session FAILED (exit code $?)."
        FAILED=true
        break
    fi

    # Wait for CI to pass before continuing
    if [[ "$SKIP_CI_WAIT" == "false" ]]; then
        echo "  Waiting for CI..."
        sleep 10
        RUN_ID=$(gh run list --limit 1 --json databaseId --jq '.[0].databaseId')
        if [[ -n "$RUN_ID" ]]; then
            if gh run watch "$RUN_ID" --exit-status 2>&1; then
                echo "  CI passed."
            else
                echo "  CI FAILED. Stopping loop."
                FAILED=true
                break
            fi
        fi
    fi

    # Verify working tree is clean
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "  WARNING: Working tree not clean after GA-${NEXT_BATCH}."
    fi
done

echo ""
echo "============================================================"
echo "  Auto-GA loop finished"
echo "  Completed: ${COMPLETED} batches"
if [[ "$FAILED" == "true" ]]; then
    echo "  Status: FAILED (see above)"
    exit 1
else
    echo "  Status: ALL DONE"
    exit 0
fi
