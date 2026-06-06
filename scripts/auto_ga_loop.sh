#!/usr/bin/env bash
# auto_ga_loop.sh — Autonomous GA hardening batch loop with ultracode
#
# Reads NEXT_DECISION.md to find the next incomplete GA batch,
# launches a Claude Code session in ultracode mode (multi-agent workflow),
# waits for CI to go green, then repeats until all batches are done or one fails.
#
# Usage:
#   ./scripts/auto_ga_loop.sh [--max-batches N]
#
# Requirements:
#   - claude CLI installed and authenticated
#   - gh CLI installed and authenticated
#   - Working tree clean before starting

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MAX_BATCHES="${MAX_BATCHES:-7}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --max-batches) MAX_BATCHES="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

cd "$REPO_ROOT"

# Verify clean working tree
if [[ -n "$(git status --porcelain)" ]]; then
    echo "ERROR: Working tree is not clean. Commit or stash changes first."
    exit 1
fi

wait_for_ci_green() {
    local max_wait=600  # 10 minutes max
    local elapsed=0

    echo "  Waiting for CI to start..."
    sleep 15  # give CI time to trigger

    while [[ $elapsed -lt $max_wait ]]; do
        local status
        status=$(gh run list --limit 1 --json status,conclusion --jq '.[0] | "\(.status) \(.conclusion)"' 2>/dev/null || echo "unknown")

        if [[ "$status" == "completed success" ]]; then
            echo "  CI passed."
            return 0
        elif [[ "$status" == "completed failure" ]]; then
            echo "  CI FAILED."
            return 1
        fi

        echo "  CI status: $status — waiting 30s... (${elapsed}s elapsed)"
        sleep 30
        elapsed=$((elapsed + 30))
    done

    echo "  CI wait timed out after ${max_wait}s"
    return 1
}

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
    echo "  Starting GA-${NEXT_BATCH} (batch ${i}/${MAX_BATCHES}) [ultracode]"
    echo "============================================================"
    echo ""

    # Build the ultracode prompt — includes "ultracode" keyword to activate
    # multi-agent workflow orchestration
    PROMPT="ultracode

You are continuing the GA hardening track for this repository (branch: feat/dashboard-ux-polish).

IMPORTANT: Read and follow these files IN ORDER before doing anything:
1. docs/SESSION_START_HERE.md
2. docs/CURRENT_STATUS.md
3. docs/NEXT_DECISION.md
4. docs/MODULE_MAP.md
5. AGENTS.md (hard boundaries, documentation maintenance rule, autonomous advancement protocol)
6. CLAUDE.md (code style, session log format, test commands)

Your task: implement GA-${NEXT_BATCH} as described in docs/NEXT_DECISION.md.

Documentation rules (from AGENTS.md):
- After every commit-sized change, update docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md, and CLAUDE.md session log if their facts changed.
- Do NOT create new roadmap, next-steps, closeout, status, or productization documents.
- Prefer editing/shortening existing docs over adding files.
- Commit messages in English, focus on why not what.
- After push, verify CI passes with gh run watch before considering the batch done.

Requirements:
1. Implement the GA-${NEXT_BATCH} scope with tests. Use multi-agent workflow orchestration to parallelize exploration and review.
2. Run cargo test -p engine, cargo fmt --check, cargo clippy -p engine --all-targets -- -D warnings
3. Run uv run --no-project python scripts/check_agent_handoff.py
4. Update docs/CURRENT_STATUS.md and docs/NEXT_DECISION.md to reflect completion
5. Update CLAUDE.md session log with a new entry for today's date
6. Commit and push
7. After push, run: gh run watch \$(gh run list --limit 1 --json databaseId -q '.[0].databaseId') --exit-status
8. Report: commit hash, test count, CI status, what was implemented, remaining risks

Do NOT start any work beyond GA-${NEXT_BATCH}. Stop after CI passes."

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

    # Wait for CI to go green — mandatory
    echo ""
    echo "  Verifying CI passes..."
    if ! wait_for_ci_green; then
        echo "  CI did not pass after GA-${NEXT_BATCH}. Stopping."
        FAILED=true
        break
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
    echo "  Status: ALL DONE — all GA batches complete, CI green"
    exit 0
fi
