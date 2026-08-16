#!/usr/bin/env bash
set -euo pipefail

# Never commits, pushes, merges, or creates PRs. The surrounding workflow owns
# every GitHub and Git write and performs its own runtime stop checks.
# Transport is local OpenCode only. There is no Codex fallback.
WORKER_TIMEOUT_SECONDS="${AGENT_CODEX_TIMEOUT_SECONDS:-1800}"

WORKER_TYPE="${1:?Usage: codex_wrapper.sh <worker-type> <prompt-file> <output-dir> [workspace]}"
PROMPT_FILE="${2:?Missing prompt file}"
OUTPUT_DIR="${3:?Missing output dir}"
WORKSPACE="${4:-$PWD}"

case "$WORKER_TYPE" in
  implement|ci-repair) OPENCODE_MODEL="deepseek/deepseek-v4-flash" ;;
  review) OPENCODE_MODEL="deepseek/deepseek-v4-pro" ;;
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

OPENCODE_BIN="$(command -v opencode || true)"
[ -n "$OPENCODE_BIN" ] || fail_closed "cli_missing" "opencode CLI not found in PATH"
[ -n "${HOME:-}" ] || fail_closed "environment_invalid" "HOME is not set"
[ -d "$WORKSPACE" ] || fail_closed "workspace_invalid" "workspace directory not found"
command -v timeout >/dev/null 2>&1 || fail_closed "timeout_unavailable" "bounded execution utility is unavailable"
if ! [[ "$WORKER_TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || [ "$WORKER_TIMEOUT_SECONDS" -lt 1 ] || [ "$WORKER_TIMEOUT_SECONDS" -gt 3600 ]; then
  fail_closed "timeout_invalid" "OpenCode execution timeout is outside the bounded range"
fi
if [ -L "$PROMPT_FILE" ] || [ ! -f "$PROMPT_FILE" ]; then
  fail_closed "prompt_missing" "prompt file not found"
fi

PARENT_TMPDIR="${TMPDIR:-/tmp}"
PROMPT_ABS="$(cd "$(dirname -- "$PROMPT_FILE")" && pwd)/$(basename -- "$PROMPT_FILE")"
case "$PROMPT_ABS" in
  "${PARENT_TMPDIR%/}"/*) ;;
  *) fail_closed "prompt_missing" "prompt file not found" ;;
esac

INVOKE_TMP="$(mktemp -d "${PARENT_TMPDIR%/}/agent-opencode.XXXXXX")" || fail_closed "environment_invalid" "invocation temp root is unavailable"
cleanup_invoke() {
  rm -rf -- "$INVOKE_TMP"
}
trap cleanup_invoke EXIT
CLAIM_PROMPT="$INVOKE_TMP/claim-prompt.txt"
cp -f -- "$PROMPT_ABS" "$CLAIM_PROMPT"
FIXED_RUN_MESSAGE="Execute the attached claim-bound task."

# Construct the child environment from an explicit allowlist.  In particular,
# do not rely on a denylist: runner images and provider CLIs add new secret-
# shaped variables over time.  The cached interactive login remains available
# through HOME.  The controller never reads, copies, or forwards credentials.
PATH="${PATH:-/usr/bin:/bin}"
LANG="${LANG:-C}"
LC_ALL="${LC_ALL:-C}"
LC_CTYPE="${LC_CTYPE:-C}"
TMPDIR="$INVOKE_TMP"
TMP="$INVOKE_TMP"
TEMP="$INVOKE_TMP"
TERM="${TERM:-dumb}"
if [ "$WORKER_TYPE" = "review" ]; then
  OPENCODE_PERMISSION='{"external_directory":"deny","doom_loop":"deny","edit":"deny"}'
else
  OPENCODE_PERMISSION='{"external_directory":"deny","doom_loop":"deny"}'
fi
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
  "OPENCODE_PERMISSION=$OPENCODE_PERMISSION"
)
for optional_name in \
  USER LOGNAME SHELL \
  HTTP_PROXY HTTPS_PROXY ALL_PROXY NO_PROXY \
  http_proxy https_proxy all_proxy no_proxy \
  SSL_CERT_FILE SSL_CERT_DIR REQUESTS_CA_BUNDLE CURL_CA_BUNDLE \
  XDG_CONFIG_HOME XDG_CACHE_HOME XDG_RUNTIME_DIR
do
  if [[ -v "$optional_name" && -n "${!optional_name}" ]]; then
    SANITIZED_ENV+=("$optional_name=${!optional_name}")
  fi
done

run_opencode() {
  env -i "${SANITIZED_ENV[@]}" "$OPENCODE_BIN" "$@"
}

run_opencode_bounded() {
  timeout --signal=TERM --kill-after=5s "$WORKER_TIMEOUT_SECONDS" \
    env -i "${SANITIZED_ENV[@]}" "$OPENCODE_BIN" "$@"
}

delete_observed_sessions() {
  local events="$1"
  local ids="$INVOKE_TMP/session-ids.txt"
  rm -f -- "$ids"
  [ -f "$events" ] || return 0
  python3 - "$events" "$ids" <<'PY' || true
import json
import pathlib
import sys

events_path = pathlib.Path(sys.argv[1])
ids_path = pathlib.Path(sys.argv[2])
if events_path.is_symlink() or not events_path.is_file():
    raise SystemExit(0)
session_ids = []
try:
    lines = events_path.read_text(encoding="utf-8").splitlines()
except (OSError, UnicodeDecodeError):
    raise SystemExit(0)
for raw_line in lines:
    if not raw_line.strip():
        continue
    try:
        payload = json.loads(raw_line)
    except json.JSONDecodeError:
        continue
    if not isinstance(payload, dict):
        continue
    session_id = payload.get("sessionID")
    if (
        isinstance(session_id, str)
        and session_id.startswith("ses_")
        and session_id not in session_ids
    ):
        session_ids.append(session_id)
if session_ids:
    ids_path.write_text("\n".join(session_ids) + "\n", encoding="utf-8")
PY
  [ -f "$ids" ] || return 0
  while IFS= read -r session_id; do
    [ -n "$session_id" ] || continue
    run_opencode session delete "$session_id" >/dev/null 2>&1 || true
  done < "$ids"
  rm -f -- "$ids"
}

if ! OPENCODE_VERSION=$(run_opencode --version 2>/dev/null); then
  fail_closed "cli_missing" "opencode version query failed"
fi
echo "opencode_version=$OPENCODE_VERSION"
if ! run_opencode auth list >/dev/null 2>&1; then
  fail_closed "authentication_failure" "opencode authentication is unavailable"
fi
HELP_OUTPUT="$INVOKE_TMP/opencode-run-help.txt"
if ! run_opencode run --help >"$HELP_OUTPUT" 2>&1; then
  fail_closed "unsupported_flags" "opencode run help is unavailable"
fi
for flag in --format --dir --file --model; do
  grep -Fq -- "$flag" "$HELP_OUTPUT" || fail_closed "unsupported_flags" "opencode run does not advertise required flags"
done

JSONL_OUTPUT="$INVOKE_TMP/opencode-events.jsonl"
STDERR_OUTPUT="$INVOKE_TMP/opencode-stderr.txt"
LAST_MESSAGE_OUTPUT="$OUTPUT_DIR/codex-last-message.txt"
LAST_MESSAGE_METADATA="$OUTPUT_DIR/codex-last-message.metadata.json"
EXIT_CODE_OUTPUT="$OUTPUT_DIR/codex-exit-code.txt"

set +e
run_opencode_bounded run \
  --format json \
  --model "$OPENCODE_MODEL" \
  --dir "$WORKSPACE" \
  "$FIXED_RUN_MESSAGE" \
  --file "$CLAIM_PROMPT" \
  > "$JSONL_OUTPUT" 2> "$STDERR_OUTPUT"
OPENCODE_EXIT=$?
set -e
echo "$OPENCODE_EXIT" > "$EXIT_CODE_OUTPUT"

if [ "$OPENCODE_EXIT" -eq 0 ] && [ "$WORKER_TYPE" != "review" ]; then
  if [ -z "$(git -C "$WORKSPACE" status --porcelain 2>/dev/null)" ]; then
    printf '%s\n' '{"key":"no_workspace_changes","detail":"worker executed successfully but produced no file changes"}' \
      > "$OUTPUT_DIR/workspace_empty.json"
  else
    rm -f "$OUTPUT_DIR/workspace_empty.json"
  fi
fi

if [ "$OPENCODE_EXIT" -ne 0 ]; then
  LOWER_OUTPUT=$(
    {
      tail -40 "$STDERR_OUTPUT" 2>/dev/null || true
      tail -40 "$JSONL_OUTPUT" 2>/dev/null || true
    } | tr '[:upper:]' '[:lower:]' | head -c 16384 || true
  )
  delete_observed_sessions "$JSONL_OUTPUT"
  : > "$JSONL_OUTPUT"
  : > "$STDERR_OUTPUT"
  if [ "$OPENCODE_EXIT" -eq 124 ] || [ "$OPENCODE_EXIT" -eq 137 ]; then
    fail_closed "model_execution_timeout" "OpenCode execution exceeded its bounded timeout"
  fi
  if grep -Eiq 'http[^[:alnum:]]*402([^[:alnum:]]|$)|status(code)?[^0-9]{0,8}402([^0-9]|$)|payment required|insufficient balance|insufficient funds|credit|usage|quota|rate limit' <<<"$LOWER_OUTPUT"; then
    fail_closed "usage_or_credit_exhaustion" "OpenCode usage or credit limit rejected execution"
  fi
  if grep -Eq 'auth|login|unauthorized|forbidden' <<<"$LOWER_OUTPUT"; then
    fail_closed "authentication_failure" "OpenCode authentication rejected execution"
  fi
  fail_closed "model_execution_failure" "OpenCode execution failed"
fi

SESSION_IDS="$INVOKE_TMP/session-ids.txt"
if ! python3 - "$JSONL_OUTPUT" "$LAST_MESSAGE_OUTPUT" "$LAST_MESSAGE_METADATA" "$WORKER_TYPE" "$SESSION_IDS" <<'PY'
import hashlib
import json
import pathlib
import sys

events_path = pathlib.Path(sys.argv[1])
last_message_path = pathlib.Path(sys.argv[2])
metadata_path = pathlib.Path(sys.argv[3])
worker_type = sys.argv[4]
session_ids_path = pathlib.Path(sys.argv[5])
max_bytes = 64 * 1024

if events_path.is_symlink() or not events_path.is_file():
    raise ValueError("events are not a regular file")
chunks: list[str] = []
session_ids: list[str] = []
for raw_line in events_path.read_text(encoding="utf-8").splitlines():
    if not raw_line.strip():
        continue
    try:
        payload = json.loads(raw_line)
    except json.JSONDecodeError as error:
        raise ValueError("events are not JSONL") from error
    if not isinstance(payload, dict):
        continue
    session_id = payload.get("sessionID")
    if (
        isinstance(session_id, str)
        and session_id.startswith("ses_")
        and session_id not in session_ids
    ):
        session_ids.append(session_id)
    if payload.get("type") != "text":
        continue
    part = payload.get("part")
    text = part.get("text") if isinstance(part, dict) else None
    if isinstance(text, str) and text:
        chunks.append(text)
if session_ids:
    session_ids_path.write_text("\n".join(session_ids) + "\n", encoding="utf-8")
raw = "".join(chunks).encode("utf-8")
if not raw or len(raw) > max_bytes:
    raise ValueError("last message size is outside the bounded range")
try:
    raw.decode("utf-8")
except UnicodeDecodeError as error:
    raise ValueError("last message is not UTF-8") from error
last_message_path.write_bytes(raw)
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
  delete_observed_sessions "$JSONL_OUTPUT"
  fail_closed "malformed_output" "OpenCode produced an invalid bounded UTF-8 last message"
fi

delete_observed_sessions "$JSONL_OUTPUT"
rm -f -- "$JSONL_OUTPUT" "$STDERR_OUTPUT" "$SESSION_IDS"
if [ "$WORKER_TYPE" != "review" ]; then
  rm -f -- "$LAST_MESSAGE_OUTPUT"
else
  echo "codex_output_file=$LAST_MESSAGE_OUTPUT"
fi
echo "codex_output_metadata=$LAST_MESSAGE_METADATA"
echo "codex_exit=0"
echo "codex_wrapper=ok"
