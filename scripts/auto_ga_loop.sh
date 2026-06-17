#!/usr/bin/env bash
# auto_ga_loop.sh — Autonomous GA hardening batch loop
#
# Launches interactive Claude Code sessions in tmux to implement GA batches.
# Each session runs in its own tmux window, executes the full GA batch
# (implement + test + commit + push + CI verify), then signals completion.
#
# Usage:
#   ./scripts/auto_ga_loop.sh [--max-batches N]
#
# Requirements:
#   - claude CLI installed and authenticated
#   - gh CLI installed and authenticated
#   - tmux installed
#   - Working tree clean before starting

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MAX_BATCHES="${MAX_BATCHES:-7}"
SESSION_NAME="auto-ga-$$"

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
    local max_wait=600
    local elapsed=0

    echo "  Waiting for CI to start..."
    sleep 15

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

# Signal file for completion
SIGNAL_DIR="/tmp/auto-ga-$$"
mkdir -p "$SIGNAL_DIR"

launch_ga_session() {
    local batch_num="$1"
    local signal_file="$SIGNAL_DIR/ga-${batch_num}.done"

    local PROMPT="ultracode

You are continuing the GA hardening track for this repository (branch: feat/dashboard-ux-polish).

IMPORTANT: Read and follow these files IN ORDER before doing anything:
1. AGENTS.md (hard boundaries, documentation maintenance rule, autonomous advancement protocol)
2. CLAUDE.md (code style, session log format, test commands)
3. docs/NEXT_DECISION.md
4. docs/MODULE_MAP.md
5. docs/CURRENT_STATUS.md
6. docs/ARCHITECTURE_BOOK.md

Your task: implement GA-${batch_num} as described in docs/NEXT_DECISION.md.

You MUST execute all steps immediately. Do NOT present a plan and wait for approval. Read files, write code, run tests, commit, push, and verify CI — all in this single session. There is no human reviewer; you have full autonomous authority for this GA batch.

Documentation rules (from AGENTS.md):
- After every commit-sized change, update docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md, and CLAUDE.md session log if their facts changed.
- Do NOT create new roadmap, next-steps, closeout, status, or productization documents.
- Prefer editing/shortening existing docs over adding files.
- Commit messages in English, focus on why not what.
- After push, verify CI passes with gh run watch before considering the batch done.

Requirements:
1. Implement the GA-${batch_num} scope with tests
2. Run cargo test -p engine, cargo fmt --check, cargo clippy -p engine --all-targets -- -D warnings
3. Run uv run --no-project python scripts/check_agent_handoff.py
4. Update docs/CURRENT_STATUS.md and docs/NEXT_DECISION.md to reflect completion
5. Update CLAUDE.md session log with a new entry for today's date
6. Commit and push
7. After push, run: gh run watch \$(gh run list --limit 1 --json databaseId -q '.[0].databaseId') --exit-status
8. Report: commit hash, test count, CI status, what was implemented, remaining risks

Do NOT start any work beyond GA-${batch_num}.

When all steps are complete and CI is green, write 'GA-${batch_num}_DONE' to ${signal_file}, then run 'exit' to close this terminal session. Do NOT wait for user input."

    # Launch in a new tmux window
    tmux new-window -t "$SESSION_NAME" -n "ga-${batch_num}" \
        "cd $REPO_ROOT && claude --dangerously-skip-permissions <<< '$PROMPT'; echo 'GA-${batch_num}_DONE' > '$signal_file'; echo '=== GA-${batch_num} session finished ==='; read -p 'Press Enter to close...'"
}

cleanup() {
    tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true
    rm -rf "$SIGNAL_DIR"
}
trap cleanup EXIT

# Create tmux session
tmux new-session -d -s "$SESSION_NAME" -c "$REPO_ROOT" "echo 'Auto-GA loop started. Waiting for first batch...'; sleep infinity"

COMPLETED=0
FAILED=false

for i in $(seq 1 "$MAX_BATCHES"); do
    NEXT_BATCH=$(grep -oP 'GA-\K\d+(?=\. .* \| \*\*NEXT\*\*|\. .* \| Pending)' \
        docs/NEXT_DECISION.md | head -1)

    if [[ -z "$NEXT_BATCH" ]]; then
        echo "=== All GA batches complete or none found ==="
        break
    fi

    echo ""
    echo "============================================================"
    echo "  Starting GA-${NEXT_BATCH} (batch ${i}/${MAX_BATCHES}) [ultracode, tmux]"
    echo "  tmux session: $SESSION_NAME, window: ga-${NEXT_BATCH}"
    echo "============================================================"
    echo ""

    SIGNAL_FILE="$SIGNAL_DIR/ga-${NEXT_BATCH}.done"
    rm -f "$SIGNAL_FILE"

    launch_ga_session "$NEXT_BATCH"

    # Wait for session to signal completion
    echo "  Waiting for GA-${NEXT_BATCH} to complete..."
    echo "  (Attach with: tmux attach -t $SESSION_NAME -w ga-${NEXT_BATCH})"

    PRE_COMMIT=$(git rev-parse HEAD)
    WAIT_ELAPSED=0
    WAIT_MAX=1200  # 20 minutes max per batch

    while [[ ! -f "$SIGNAL_FILE" ]]; do
        sleep 10
        WAIT_ELAPSED=$((WAIT_ELAPSED + 10))

        # Check if a new commit appeared (fallback if signal file wasn't written)
        CURRENT_COMMIT=$(git rev-parse HEAD)
        if [[ "$CURRENT_COMMIT" != "$PRE_COMMIT" ]]; then
            echo "  New commit detected: $(git log --oneline -1). Assuming GA-${NEXT_BATCH} done."
            break
        fi

        # Check if claude process is still running
        if ! tmux list-windows -t "$SESSION_NAME" 2>/dev/null | grep -q "ga-${NEXT_BATCH}"; then
            echo "  GA-${NEXT_BATCH} window closed."
            break
        fi

        # Timeout
        if [[ $WAIT_ELAPSED -ge $WAIT_MAX ]]; then
            echo "  Timeout waiting for GA-${NEXT_BATCH} (${WAIT_MAX}s)."
            FAILED=true
            break 2
        fi
    done

    if [[ "$FAILED" == "true" ]]; then
        break
    fi

    echo "  GA-${NEXT_BATCH} session completed."
    COMPLETED=$((COMPLETED + 1))

    # Wait for CI to go green — mandatory
    echo ""
    echo "  Verifying CI passes..."
    if ! wait_for_ci_green; then
        echo "  CI did not pass after GA-${NEXT_BATCH}. Stopping."
        FAILED=true
        break
    fi
done

echo ""
echo "============================================================"
echo "  Auto-GA loop finished"
echo "  Completed: ${COMPLETED} batches"
if [[ "$FAILED" == "true" ]]; then
    echo "  Status: FAILED (see above)"
    echo "  Attach to tmux session to inspect: tmux attach -t $SESSION_NAME"
    exit 1
else
    echo "  Status: ALL DONE"
    tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true
    exit 0
fi
