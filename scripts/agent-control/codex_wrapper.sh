#!/usr/bin/env bash
#
# Centralized Codex CLI invocation wrapper for the agent orchestrator.
#
# Usage:
#   codex_wrapper.sh <worker-type> <prompt-file> <output-dir>
#
# Worker types:
#   implement   -- workspace-write sandbox, creates branches/PRs
#   ci-repair   -- workspace-write sandbox, bounded CI fixes
#   review      -- read-only sandbox, structured JSON verdict
#
# The wrapper:
#   1. Validates the environment (Codex CLI, auth, HOME, CODEX_HOME).
#   2. Checks the orchestrator gate (AGENT_ORCHESTRATOR_ENABLED).
#   3. Runs codex exec with supported flags only.
#   4. Captures exit code, JSONL output, and last message.
#   5. For review workers, validates the output against the JSON schema.
#   6. Returns structured results.
#
# Security:
#   - Never uses --no-sandbox or --dangerously-bypass-approvals-and-sandbox.
#   - Uses the narrowest sandbox for each worker type.
#   - Never prints or uploads authentication material.
#   - Fails closed on any unexpected condition.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKER_TYPE="${1:?Usage: codex_wrapper.sh <worker-type> <prompt-file> <output-dir>}"
PROMPT_FILE="${2:?Missing prompt file}"
OUTPUT_DIR="${3:?Missing output dir}"
WORKSPACE="${4:-$PWD}"

# ---- Gates ----
if [ "${AGENT_ORCHESTRATOR_ENABLED:-false}" != "true" ]; then
  echo "AGENT_ORCHESTRATOR_ENABLED is not true. Aborting Codex invocation."
  exit 0
fi

# ---- Preflight ----
if ! command -v codex &>/dev/null; then
  echo "FATAL: codex CLI not found in PATH" >&2
  exit 1
fi

CODEX_VERSION=$(codex --version 2>/dev/null || echo "unknown")
echo "codex_version=${CODEX_VERSION}"

if ! codex login status &>/dev/null; then
  echo "FATAL: codex is not authenticated. Run 'codex login' first." >&2
  exit 1
fi

echo "codex_auth=ok"

if [ -z "${HOME:-}" ]; then
  echo "FATAL: HOME is not set" >&2
  exit 1
fi

if [ ! -d "${CODEX_HOME:-$HOME/.codex}" ]; then
  echo "WARNING: CODEX_HOME ($CODEX_HOME) does not exist; defaulting to ~/.codex" >&2
fi

if [ ! -f "$PROMPT_FILE" ]; then
  echo "FATAL: prompt file not found: $PROMPT_FILE" >&2
  exit 1
fi

mkdir -p "$OUTPUT_DIR"

# ---- Select sandbox mode ----
SANDBOX_MODE="read-only"
case "$WORKER_TYPE" in
  implement|ci-repair)
    SANDBOX_MODE="workspace-write"
    ;;
  review)
    SANDBOX_MODE="read-only"
    ;;
esac

# ---- Output files ----
JSONL_OUTPUT="$OUTPUT_DIR/codex-events.jsonl"
LAST_MESSAGE_OUTPUT="$OUTPUT_DIR/codex-last-message.json"
EXIT_CODE_OUTPUT="$OUTPUT_DIR/codex-exit-code.txt"

# ---- Run Codex ----
echo "Running: codex exec --cd \"$WORKSPACE\" --sandbox $SANDBOX_MODE --ephemeral --json --output-last-message \"$LAST_MESSAGE_OUTPUT\" - < \"$PROMPT_FILE\""

set +e
codex exec \
  --cd "$WORKSPACE" \
  --sandbox "$SANDBOX_MODE" \
  --ephemeral \
  --json \
  --output-last-message "$LAST_MESSAGE_OUTPUT" \
  - < "$PROMPT_FILE" \
  > "$JSONL_OUTPUT" 2>&1

CODEX_EXIT=$?
set -e

echo "$CODEX_EXIT" > "$EXIT_CODE_OUTPUT"
echo "codex_exit=$CODEX_EXIT"

if [ "$CODEX_EXIT" -ne 0 ]; then
  echo "ERROR: codex exec failed with exit code $CODEX_EXIT" >&2
  tail -20 "$JSONL_OUTPUT" >&2
  exit "$CODEX_EXIT"
fi

echo "codex_output_file=$LAST_MESSAGE_OUTPUT"
echo "codex_events=$JSONL_OUTPUT"

# ---- Check last message exists ----
if [ ! -f "$LAST_MESSAGE_OUTPUT" ] || [ ! -s "$LAST_MESSAGE_OUTPUT" ]; then
  echo "ERROR: codex produced no last-message output" >&2
  exit 1
fi

# ---- For review workers, validate against JSON schema ----
if [ "$WORKER_TYPE" = "review" ]; then
  SCHEMA_FILE="$SCRIPT_DIR/review_schema.json"
  if python3 -c "
import json, sys
with open('$LAST_MESSAGE_OUTPUT') as f:
    data = json.load(f)
with open('$SCHEMA_FILE') as f:
    schema = json.load(f)

required = schema.get('required', [])
for field in required:
    if field not in data:
        print(f'Missing required field: {field}')
        sys.exit(1)

verdict = data.get('verdict', '')
if verdict not in ('PASS', 'PASS_WITH_NOTES', 'BLOCKED', 'FAIL'):
    print(f'Invalid verdict: {verdict}')
    sys.exit(1)

print('Review output valid')
" 2>&1; then
    echo "review_validation=ok"
    echo "review_verdict=$(python3 -c "import json; print(json.load(open('$LAST_MESSAGE_OUTPUT'))['verdict'])")"
  else
    echo "FATAL: review output validation failed" >&2
    exit 1
  fi
fi

echo "codex_wrapper=ok"
exit 0
