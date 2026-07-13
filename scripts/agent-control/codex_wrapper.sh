#!/usr/bin/env bash
set -euo pipefail

# Never commits, pushes, merges, or creates PRs. The surrounding workflow owns
# every GitHub and Git write and performs its own runtime stop checks.
CODEX_HOME="${CODEX_HOME:-}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKER_TYPE="${1:?Usage: codex_wrapper.sh <worker-type> <prompt-file> <output-dir> [workspace]}"
PROMPT_FILE="${2:?Missing prompt file}"
OUTPUT_DIR="${3:?Missing output dir}"
WORKSPACE="${4:-$PWD}"

case "$WORKER_TYPE" in
  implement|ci-repair|review) ;;
  *)
    mkdir -p "$OUTPUT_DIR"
    printf '%s\n' '{"kind":"agent-orchestrator-failure","reason":"unsupported_worker_type"}' > "$OUTPUT_DIR/failure_reason.json"
    echo "FATAL: unsupported worker type" >&2
    exit 2
    ;;
esac

record_failure() {
  local reason="$1"
  local detail="${2:-}"
  mkdir -p "$OUTPUT_DIR"
  python3 - "$OUTPUT_DIR/failure_reason.json" "$reason" "$detail" <<'PY'
import json
import sys
path, reason, detail = sys.argv[1:]
with open(path, "w", encoding="utf-8") as handle:
    json.dump({
        "kind": "agent-orchestrator-failure",
        "reason": reason,
        "detail": detail[:240],
    }, handle, sort_keys=True)
    handle.write("\n")
PY
}

fail_closed() {
  local reason="$1"
  local message="$2"
  record_failure "$reason" "$message"
  echo "FATAL: $message" >&2
  exit 1
}

mkdir -p "$OUTPUT_DIR"

CODEX_BIN="$(command -v codex || true)"
[ -n "$CODEX_BIN" ] || fail_closed "cli_missing" "codex CLI not found in PATH"
[ -n "${HOME:-}" ] || fail_closed "environment_invalid" "HOME is not set"
[ -f "$PROMPT_FILE" ] || fail_closed "prompt_missing" "prompt file not found"
[ -d "$WORKSPACE" ] || fail_closed "workspace_invalid" "workspace directory not found"

# Construct the child environment from an explicit allowlist.  In particular,
# do not rely on a denylist: runner images and provider CLIs add new secret-
# shaped variables over time.  The cached interactive login remains available
# through HOME/CODEX_HOME, while API-key or GitHub-token authentication cannot
# silently reach Codex.
PATH="${PATH:-/usr/bin:/bin}"
LANG="${LANG:-C}"
LC_ALL="${LC_ALL:-C}"
LC_CTYPE="${LC_CTYPE:-C}"
TMPDIR="${TMPDIR:-/tmp}"
TMP="${TMP:-$TMPDIR}"
TEMP="${TEMP:-$TMPDIR}"
TERM="${TERM:-dumb}"
SANITIZED_ENV=(
  "HOME=$HOME"
  "PATH=$PATH"
  "LANG=$LANG"
  "LC_ALL=$LC_ALL"
  "LC_CTYPE=$LC_CTYPE"
  "TMPDIR=$TMPDIR"
  "TMP=$TMP"
  "TEMP=$TEMP"
  "TERM=$TERM"
)
for optional_name in CODEX_HOME USER LOGNAME SHELL; do
  if [[ -v "$optional_name" && -n "${!optional_name}" ]]; then
    SANITIZED_ENV+=("$optional_name=${!optional_name}")
  fi
done

run_codex() {
  env -i "${SANITIZED_ENV[@]}" "$CODEX_BIN" "$@"
}

if ! CODEX_VERSION=$(run_codex --version 2>/dev/null); then
  fail_closed "cli_missing" "codex version query failed"
fi
echo "codex_version=$CODEX_VERSION"
if ! run_codex login status >/dev/null 2>&1; then
  fail_closed "authentication_failure" "codex authentication is unavailable"
fi
if ! run_codex exec --help >"$OUTPUT_DIR/codex-exec-help.txt" 2>/dev/null; then
  fail_closed "unsupported_flags" "codex exec help is unavailable"
fi
for flag in --cd --sandbox --ephemeral --json --output-last-message; do
  grep -Fq -- "$flag" "$OUTPUT_DIR/codex-exec-help.txt" || fail_closed "unsupported_flags" "codex exec does not advertise required flags"
done

SANDBOX_MODE="read-only"
if [ "$WORKER_TYPE" = "implement" ] || [ "$WORKER_TYPE" = "ci-repair" ]; then
  SANDBOX_MODE="workspace-write"
fi
JSONL_OUTPUT="$OUTPUT_DIR/codex-events.jsonl"
LAST_MESSAGE_OUTPUT="$OUTPUT_DIR/codex-last-message.json"
EXIT_CODE_OUTPUT="$OUTPUT_DIR/codex-exit-code.txt"
RAW_OUTPUT="$(mktemp "$TMPDIR/agent-codex-output.XXXXXX")"
trap 'rm -f "$RAW_OUTPUT"' EXIT

set +e
run_codex exec \
  --cd "$WORKSPACE" \
  --sandbox "$SANDBOX_MODE" \
  --ephemeral \
  --json \
  --output-last-message "$LAST_MESSAGE_OUTPUT" \
  - < "$PROMPT_FILE" \
  > "$RAW_OUTPUT" 2>&1
CODEX_EXIT=$?
set -e
echo "$CODEX_EXIT" > "$EXIT_CODE_OUTPUT"

if [ "$CODEX_EXIT" -ne 0 ]; then
  LOWER_OUTPUT=$(tr '[:upper:]' '[:lower:]' < "$RAW_OUTPUT" | tail -40 || true)
  # Never upload arbitrary provider/model stderr.  It may contain a secret,
  # prompt content, or an access token even though the child environment is
  # sanitized.
  printf '%s\n' '{"type":"error","message":"codex execution failed"}' > "$JSONL_OUTPUT"
  if grep -Eq 'credit|usage|quota|rate limit' <<<"$LOWER_OUTPUT"; then
    fail_closed "usage_or_credit_exhaustion" "Codex usage or credit limit rejected execution"
  fi
  if grep -Eq 'auth|login|unauthorized|forbidden' <<<"$LOWER_OUTPUT"; then
    fail_closed "authentication_failure" "Codex authentication rejected execution"
  fi
  fail_closed "model_execution_failure" "Codex execution failed"
fi

cat "$RAW_OUTPUT" > "$JSONL_OUTPUT"

if [ ! -s "$LAST_MESSAGE_OUTPUT" ]; then
  fail_closed "malformed_output" "Codex produced no structured last-message output"
fi
if ! python3 - "$LAST_MESSAGE_OUTPUT" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as handle:
    value = json.load(handle)
if not isinstance(value, dict):
    raise ValueError("last message is not a JSON object")
PY
then
  fail_closed "malformed_output" "Codex last-message output is not valid JSON"
fi

echo "codex_output_file=$LAST_MESSAGE_OUTPUT"
echo "codex_events=$JSONL_OUTPUT"
echo "codex_exit=0"
echo "codex_wrapper=ok"
