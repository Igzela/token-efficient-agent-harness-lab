#!/usr/bin/env bash
set -euo pipefail

# This wrapper is the only provider-backed transport for production WorkCards.
# It never commits, pushes, merges, or creates PRs; the parent Steward owns all
# repository and GitHub effects. Provider output is transient and is never
# copied into journal evidence.
WORKER_TIMEOUT_SECONDS="${AGENT_CODEX_TIMEOUT_SECONDS:-1800}"

WORKER_TYPE="${1:?Usage: codex_wrapper.sh <worker-type> <prompt-file> <output-dir> [workspace]}"
PROMPT_FILE="${2:?Missing prompt file}"
OUTPUT_DIR="${3:?Missing output dir}"
WORKSPACE="${4:-$PWD}"

MODEL_TIER="${AGENT_CODEX_MODEL_TIER:-T1}"
case "$MODEL_TIER" in
  # The authenticated Codex CLI owns the account-appropriate default model.
  # An explicit model is accepted only as an operator-provided override; the
  # production path deliberately does not force an API model id that may not
  # be exposed by the ChatGPT-backed CLI account.
  T0|T1|T2) CODEX_MODEL="${AGENT_CODEX_MODEL:-}" ;;
  *)
    mkdir -p "$OUTPUT_DIR"
    printf '%s\n' '{"kind":"agent-orchestrator-failure","reason":"environment_invalid"}' > "$OUTPUT_DIR/failure_reason.json"
    echo "FATAL: unsupported model tier" >&2
    exit 2
    ;;
esac

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
  /usr/bin/python3 - "$OUTPUT_DIR/failure_reason.json" "$reason" "$detail" <<'PY'
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

WRAPPER_DIR="$(cd -- "$(dirname -- "$0")" && pwd -P)"
CODEX_BIN="$WRAPPER_DIR/codex"
[ ! -L "$CODEX_BIN" ] || fail_closed "cli_missing" "Codex CLI path must not be a symlink"
[ -x "$CODEX_BIN" ] || fail_closed "cli_missing" "Codex CLI is not executable"
[ -n "${CODEX_HOME:-}" ] || fail_closed "authentication_failure" "Codex isolated authentication home is unavailable"
[ -d "$WORKSPACE" ] || fail_closed "workspace_invalid" "workspace directory not found"
command -v timeout >/dev/null 2>&1 || fail_closed "timeout_unavailable" "bounded execution utility is unavailable"
if ! [[ "$WORKER_TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || [ "$WORKER_TIMEOUT_SECONDS" -lt 1 ] || [ "$WORKER_TIMEOUT_SECONDS" -gt 3600 ]; then
  fail_closed "timeout_invalid" "Codex execution timeout is outside the bounded range"
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

INVOKE_TMP="$(mktemp -d "${PARENT_TMPDIR%/}/agent-codex.XXXXXX")" || fail_closed "environment_invalid" "invocation temp root is unavailable"
cleanup_invoke() {
  rm -rf -- "$INVOKE_TMP"
}
trap cleanup_invoke EXIT
CLAIM_PROMPT="$INVOKE_TMP/claim-prompt.txt"
cp -f -- "$PROMPT_ABS" "$CLAIM_PROMPT"

# Construct the child environment from an explicit allowlist. CODEX_HOME is
# supplied by the parent; the executable is the fixed read-only sibling above.
# No host config or credential-shaped variable is forwarded.
PATH="${PATH:-/usr/bin:/bin}"
LANG="${LANG:-C}"
LC_ALL="${LC_ALL:-C}"
LC_CTYPE="${LC_CTYPE:-C}"
TMPDIR="$INVOKE_TMP"
TMP="$INVOKE_TMP"
TEMP="$INVOKE_TMP"
TERM="${TERM:-dumb}"
SANITIZED_ENV=(
  "HOME=${HOME:-/nonexistent}"
  "PATH=$PATH"
  "LANG=$LANG"
  "LC_ALL=$LC_ALL"
  "LC_CTYPE=$LC_CTYPE"
  "TMPDIR=$TMPDIR"
  "TMP=$TMP"
  "TEMP=$TEMP"
  "TERM=$TERM"
  "CODEX_HOME=$CODEX_HOME"
  "AGENT_CODEX_MODEL_TIER=$MODEL_TIER"
)
for optional_name in USER LOGNAME SHELL HTTP_PROXY HTTPS_PROXY ALL_PROXY; do
  if [[ -v "$optional_name" && -n "${!optional_name}" ]]; then
    SANITIZED_ENV+=("$optional_name=${!optional_name}")
  fi
done

run_codex() {
  env -i "${SANITIZED_ENV[@]}" "$CODEX_BIN" "$@"
}

run_codex_bounded() {
  timeout --signal=TERM --kill-after=5s "$WORKER_TIMEOUT_SECONDS" \
    env -i "${SANITIZED_ENV[@]}" "$CODEX_BIN" "$@"
}

if ! CODEX_VERSION=$(run_codex --version 2>/dev/null); then
  fail_closed "cli_missing" "Codex version query failed"
fi
echo "codex_version=$CODEX_VERSION"

HELP_OUTPUT="$INVOKE_TMP/codex-exec-help.txt"
if ! run_codex exec --help >"$HELP_OUTPUT" 2>&1; then
  fail_closed "unsupported_flags" "Codex exec help is unavailable"
fi
for flag in --json --ephemeral --ignore-user-config --skip-git-repo-check --sandbox --model --cd --output-last-message; do
  grep -Fq -- "$flag" "$HELP_OUTPUT" || fail_closed "unsupported_flags" "Codex exec does not advertise required flags"
done

JSONL_OUTPUT="$INVOKE_TMP/codex-events.jsonl"
STDERR_OUTPUT="$INVOKE_TMP/codex-stderr.txt"
LAST_MESSAGE_OUTPUT="$OUTPUT_DIR/codex-last-message.txt"
LAST_MESSAGE_METADATA="$OUTPUT_DIR/codex-last-message.metadata.json"
EXIT_CODE_OUTPUT="$OUTPUT_DIR/codex-exit-code.txt"
SANDBOX_MODE="read-only"
APPROVAL_ARGS=()
if [ "$WORKER_TYPE" != "review" ]; then
  # Codex rejects --sandbox together with --approve-for-me. The approval flag
  # itself selects the bounded workspace-write policy for implementation and
  # repair calls; reviews explicitly select read-only below.
  APPROVAL_ARGS=(--approve-for-me)
fi
CODEX_EXEC_ARGS=(
  --json
  --ephemeral
  --ignore-user-config
  --skip-git-repo-check
  "${APPROVAL_ARGS[@]}"
)
if [ "$WORKER_TYPE" = "review" ]; then
  CODEX_EXEC_ARGS+=(--sandbox "$SANDBOX_MODE")
fi
if [ -n "$CODEX_MODEL" ]; then
  CODEX_EXEC_ARGS+=(--model "$CODEX_MODEL")
fi
CODEX_EXEC_ARGS+=(--cd "$WORKSPACE" --output-last-message "$LAST_MESSAGE_OUTPUT" -)

set +e
run_codex_bounded exec "${CODEX_EXEC_ARGS[@]}" < "$CLAIM_PROMPT" > "$JSONL_OUTPUT" 2> "$STDERR_OUTPUT"
CODEX_EXIT=$?
set -e
echo "$CODEX_EXIT" > "$EXIT_CODE_OUTPUT"

if [ "$CODEX_EXIT" -eq 0 ] && [ "$WORKER_TYPE" != "review" ]; then
  if [ -z "$(git -C "$WORKSPACE" status --porcelain 2>/dev/null)" ]; then
    printf '%s\n' '{"key":"no_workspace_changes","detail":"worker executed successfully but produced no file changes"}' \
      > "$OUTPUT_DIR/workspace_empty.json"
  else
    rm -f "$OUTPUT_DIR/workspace_empty.json"
  fi
fi

if [ "$CODEX_EXIT" -ne 0 ]; then
  FAILURE_CLASS=$(/usr/bin/python3 - "$STDERR_OUTPUT" "$JSONL_OUTPUT" <<'PY'
import json
import re
from pathlib import Path
import sys

texts = []
def collect(value):
    if isinstance(value, dict):
        for item in value.values():
            collect(item)
    elif isinstance(value, list):
        for item in value:
            collect(item)
    elif isinstance(value, str):
        texts.append(value.lower())

stderr_path = Path(sys.argv[1])
events_path = Path(sys.argv[2])
if stderr_path.is_file():
    texts.append(stderr_path.read_text(encoding="utf-8", errors="replace").lower())
if events_path.is_file():
    for line in events_path.read_text(encoding="utf-8", errors="replace").splitlines()[-40:]:
        try:
            collect(json.loads(line))
        except json.JSONDecodeError:
            continue
joined = "\n".join(texts)
if re.search(r"credit|quota|rate[ -]?limit|usage[ -]?limit|insufficient balance|billing", joined):
    print("usage_or_credit_exhaustion")
elif re.search(r"auth|login|unauthorized|forbidden", joined):
    print("authentication_failure")
else:
    print("model_execution_failure")
PY
  )
  : > "$JSONL_OUTPUT"
  : > "$STDERR_OUTPUT"
  if [ "$CODEX_EXIT" -eq 124 ] || [ "$CODEX_EXIT" -eq 137 ]; then
    fail_closed "model_execution_timeout" "Codex execution exceeded its bounded timeout"
  fi
  case "$FAILURE_CLASS" in
    usage_or_credit_exhaustion)
      fail_closed "usage_or_credit_exhaustion" "Codex usage or credit limit rejected execution" ;;
    authentication_failure)
      fail_closed "authentication_failure" "Codex authentication rejected execution" ;;
    *)
      fail_closed "model_execution_failure" "Codex execution failed" ;;
  esac
fi

if ! /usr/bin/python3 - "$LAST_MESSAGE_OUTPUT" "$LAST_MESSAGE_METADATA" "$WORKER_TYPE" <<'PY'
import hashlib
import json
from pathlib import Path
import sys

message_path = Path(sys.argv[1])
metadata_path = Path(sys.argv[2])
worker_type = sys.argv[3]
max_bytes = 64 * 1024
if message_path.is_symlink() or not message_path.is_file():
    raise SystemExit("last message is not a regular file")
raw = message_path.read_bytes()
if not raw or len(raw) > max_bytes:
    raise SystemExit("last message size is outside the bounded range")
raw.decode("utf-8")
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

rm -f -- "$JSONL_OUTPUT" "$STDERR_OUTPUT"
if [ "$WORKER_TYPE" != "review" ]; then
  rm -f -- "$LAST_MESSAGE_OUTPUT"
else
  echo "codex_output_file=$LAST_MESSAGE_OUTPUT"
fi
echo "codex_output_metadata=$LAST_MESSAGE_METADATA"
echo "codex_exit=0"
echo "codex_wrapper=ok"
