#!/usr/bin/env bash
# auto_adapt_loop.sh — Autonomous dormant-module adaptation loop
#
# Launches interactive Claude Code sessions in tmux to implement adaptation phases.
# Each session runs in its own tmux window, implements one phase
# (implement + test + commit + push + CI verify), then signals completion.
#
# Usage:
#   ./scripts/auto_adapt_loop.sh [--max-phases N]
#
# Requirements:
#   - claude CLI installed and authenticated
#   - gh CLI installed and authenticated
#   - tmux installed
#   - Working tree clean before starting

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MAX_PHASES="${MAX_PHASES:-4}"
START_PHASE="${START_PHASE:-1}"
SESSION_NAME="auto-adapt-$$"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --max-phases) MAX_PHASES="$2"; shift 2 ;;
        --start-phase) START_PHASE="$2"; shift 2 ;;
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
SIGNAL_DIR="/tmp/auto-adapt-$$"
mkdir -p "$SIGNAL_DIR"

PHASE_DESCRIPTIONS=(
    ""
    "Interface Unification: Extract trait Evaluator from EvaluationStub; unify ModelProvider→Provider adapter; extract trait GraphOperations from dag_manager + dependency_resolver."
    "Zero-Conflict Activation: Activate workflow/dag_manager mutations in planner; activate workflow/context_pack rules in task_analyzer; activate orchestration/work_queue replacing inline state machine; activate routing/auto_policies in scheduler tick; activate orchestration/result_aggregator."
    "Adapted Activation: Activate harness/advisor as dispatch advisory layer; activate routing/feedback_integrator driving adaptive routing; activate quality/ gate chain replacing EvaluationStub; activate orchestration/workflow_engine as scheduler concurrency accelerator; activate orchestration/conflict_resolver + human_approval_gate."
    "Dead Code Cleanup: Remove app_layer/ (fully duplicated); remove dispatch/manual/ (superseded); remove harness/{sandbox,supervisor,batch_runner,sampling,model_eval}; remove storage/{durable_store,health_checker,storage_migrator}; tag event_source/+errors+event_schema as reference-only."
)

launch_phase_session() {
    local phase_num="$1"
    local signal_file="$SIGNAL_DIR/phase-${phase_num}.done"
    local desc="${PHASE_DESCRIPTIONS[$phase_num]}"

    local PROMPT="ultracode

You are continuing the Dormant Module Adaptation track for this repository (branch: feat/dashboard-ux-polish).

IMPORTANT: Read and follow these files IN ORDER before doing anything:
1. AGENTS.md (hard boundaries, documentation maintenance rule, autonomous advancement protocol)
2. CLAUDE.md (code style, session log format, test commands)
3. docs/NEXT_DECISION.md
4. docs/MODULE_MAP.md
5. docs/CURRENT_STATUS.md
6. docs/ARCHITECTURE_BOOK.md

Your task: implement Dormant Adaptation Phase ${phase_num}.

Phase description: ${desc}

You MUST execute all steps immediately. Do NOT present a plan and wait for approval. Read files, write code, run tests, commit, push, and verify CI — all in this single session. There is no human reviewer; you have full autonomous authority for this adaptation phase.

Key constraints:
- Do NOT create parallel runtime/DAG/scheduler kernels — extend existing modules only
- Provider execution remains bounded behind a ready trusted-local profile or standalone legacy gate
- Target repo writes remain forbidden
- No sandbox/process/container/VM execution beyond existing CLI executor path
- R-series file splitting remains sealed at R7
- All 1286+ existing tests must continue to pass
- New code must have tests

Documentation rules (from AGENTS.md):
- After every commit-sized change, update docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md, and CLAUDE.md session log if their facts changed.
- Do NOT create new roadmap, next-steps, closeout, status, or productization documents.
- Prefer editing/shortening existing docs over adding files.
- Commit messages in English, focus on why not what.
- After push, verify CI passes with gh run watch before considering the phase done.

Requirements:
1. Implement Phase ${phase_num} scope with tests
2. Run cargo test -p engine, cargo fmt --check, cargo clippy -p engine --all-targets -- -D warnings
3. Run uv run --no-project python scripts/check_agent_handoff.py
4. Update docs/CURRENT_STATUS.md and docs/NEXT_DECISION.md to reflect completion
5. Update CLAUDE.md session log with a new entry for today's date
6. Commit and push
7. After push, run: gh run watch \$(gh run list --limit 1 --json databaseId -q '.[0].databaseId') --exit-status
8. Report: commit hash, test count, CI status, what was implemented, remaining risks

Do NOT start any work beyond Phase ${phase_num}.

When all steps are complete and CI is green, write 'PHASE-${phase_num}_DONE' to ${signal_file}, then run 'exit' to close this terminal session. Do NOT wait for user input."

    # Launch in a new tmux window
    tmux new-window -t "$SESSION_NAME" -n "phase-${phase_num}" \
        "cd $REPO_ROOT && claude --dangerously-skip-permissions <<< '$PROMPT'; echo 'PHASE-${phase_num}_DONE' > '$signal_file'; echo '=== Phase ${phase_num} session finished ==='; read -p 'Press Enter to close...'"
}

cleanup() {
    tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true
    rm -rf "$SIGNAL_DIR"
}
trap cleanup EXIT

# Create tmux session
tmux new-session -d -s "$SESSION_NAME" -c "$REPO_ROOT" "echo 'Auto-adapt loop started. Waiting for first phase...'; sleep infinity"

COMPLETED=0
FAILED=false

for i in $(seq "$START_PHASE" "$MAX_PHASES"); do
    NEXT_PHASE=$i

    if [[ $NEXT_PHASE -gt $MAX_PHASES ]]; then
        echo "=== All adaptation phases complete ==="
        break
    fi

    echo ""
    echo "============================================================"
    echo "  Starting Phase ${NEXT_PHASE} (phase ${i}/${MAX_PHASES}) [ultracode, tmux]"
    echo "  Description: ${PHASE_DESCRIPTIONS[$NEXT_PHASE]}"
    echo "  tmux session: $SESSION_NAME, window: phase-${NEXT_PHASE}"
    echo "============================================================"
    echo ""

    SIGNAL_FILE="$SIGNAL_DIR/phase-${NEXT_PHASE}.done"
    rm -f "$SIGNAL_FILE"

    launch_phase_session "$NEXT_PHASE"

    # Wait for session to signal completion
    echo "  Waiting for Phase ${NEXT_PHASE} to complete..."
    echo "  (Attach with: tmux attach -t $SESSION_NAME -w phase-${NEXT_PHASE})"

    PRE_COMMIT=$(git rev-parse HEAD)
    WAIT_ELAPSED=0
    WAIT_MAX=1800  # 30 minutes max per phase (larger than GA batches)

    while [[ ! -f "$SIGNAL_FILE" ]]; do
        sleep 10
        WAIT_ELAPSED=$((WAIT_ELAPSED + 10))

        # Check if a new commit appeared (fallback if signal file wasn't written)
        CURRENT_COMMIT=$(git rev-parse HEAD)
        if [[ "$CURRENT_COMMIT" != "$PRE_COMMIT" ]]; then
            echo "  New commit detected: $(git log --oneline -1). Assuming Phase ${NEXT_PHASE} done."
            break
        fi

        # Check if claude process is still running
        if ! tmux list-windows -t "$SESSION_NAME" 2>/dev/null | grep -q "phase-${NEXT_PHASE}"; then
            echo "  Phase ${NEXT_PHASE} window closed."
            break
        fi

        # Timeout
        if [[ $WAIT_ELAPSED -ge $WAIT_MAX ]]; then
            echo "  Timeout waiting for Phase ${NEXT_PHASE} (${WAIT_MAX}s)."
            FAILED=true
            break 2
        fi
    done

    if [[ "$FAILED" == "true" ]]; then
        break
    fi

    echo "  Phase ${NEXT_PHASE} session completed."
    COMPLETED=$((COMPLETED + 1))

    # Only wait for CI if there was a new commit (phase actually changed code)
    CURRENT_COMMIT=$(git rev-parse HEAD)
    if [[ "$CURRENT_COMMIT" != "$PRE_COMMIT" ]]; then
        echo ""
        echo "  Verifying CI passes..."
        if ! wait_for_ci_green; then
            echo "  CI did not pass after Phase ${NEXT_PHASE}. Stopping."
            FAILED=true
            break
        fi
    else
        echo "  No new commit — skipping CI wait (Phase ${NEXT_PHASE} verified existing code)."
    fi
done

echo ""
echo "============================================================"
echo "  Auto-adapt loop finished"
echo "  Completed: ${COMPLETED} phases"
if [[ "$FAILED" == "true" ]]; then
    echo "  Status: FAILED (see above)"
    echo "  Attach to tmux session to inspect: tmux attach -t $SESSION_NAME"
    exit 1
else
    echo "  Status: ALL DONE"
    tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true
    exit 0
fi
