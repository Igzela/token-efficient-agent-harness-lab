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
LAST_MESSAGE_OUTPUT="$OUTPUT_DIR/codex-last-message.txt"
LAST_MESSAGE_METADATA="$OUTPUT_DIR/codex-last-message.metadata.json"
EXIT_CODE_OUTPUT="$OUTPUT_DIR/codex-exit-code.txt"
STDERR_OUTPUT="$(mktemp "$TMPDIR/agent-codex-stderr.XXXXXX")"
trap 'rm -f "$STDERR_OUTPUT"' EXIT

set +e
run_codex exec \
  --cd "$WORKSPACE" \
  --sandbox "$SANDBOX_MODE" \
  --ephemeral \
  --json \
  --output-last-message "$LAST_MESSAGE_OUTPUT" \
  - < "$PROMPT_FILE" \
  > "$JSONL_OUTPUT" 2> "$STDERR_OUTPUT"
CODEX_EXIT=$?
set -e
echo "$CODEX_EXIT" > "$EXIT_CODE_OUTPUT"

if [ "$CODEX_EXIT" -ne 0 ]; then
  LOWER_OUTPUT=$(
    {
      tail -40 "$STDERR_OUTPUT"
      tail -40 "$JSONL_OUTPUT"
    } | tr '[:upper:]' '[:lower:]' | head -c 16384 || true
  )
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

if ! python3 - "$LAST_MESSAGE_OUTPUT" "$LAST_MESSAGE_METADATA" "$WORKER_TYPE" <<'PY'
import hashlib
import json
import pathlib
import sys

last_message_path = pathlib.Path(sys.argv[1])
metadata_path = pathlib.Path(sys.argv[2])
worker_type = sys.argv[3]
max_bytes = 64 * 1024

if last_message_path.is_symlink() or not last_message_path.is_file():
    raise ValueError("last message is not a regular file")
size = last_message_path.stat().st_size
if size <= 0 or size > max_bytes:
    raise ValueError("last message size is outside the bounded range")
raw = last_message_path.read_bytes()
if len(raw) != size:
    raise ValueError("last message changed while being read")
try:
    raw.decode("utf-8")
except UnicodeDecodeError as error:
    raise ValueError("last message is not UTF-8") from error

metadata = {
    "worker_type": worker_type,
    "format": "text",
    "byte_count": len(raw),
    "sha256": hashlib.sha256(raw).hexdigest(),
}
temporary = metadata_path.with_name(f".{metadata_path.name}.tmp")
with temporary.open("w", encoding="utf-8") as handle:
    json.dump(metadata, handle, sort_keys=True, separators=(",", ":"))
    handle.write("\n")
temporary.replace(metadata_path)
PY
then
  fail_closed "malformed_output" "Codex produced an invalid bounded UTF-8 last message"
fi

if [ "$WORKER_TYPE" != "review" ]; then
  rm -f -- "$LAST_MESSAGE_OUTPUT"
else
  echo "codex_output_file=$LAST_MESSAGE_OUTPUT"
fi
echo "codex_output_metadata=$LAST_MESSAGE_METADATA"
echo "codex_events=$JSONL_OUTPUT"
echo "codex_exit=0"
echo "codex_wrapper=ok"
