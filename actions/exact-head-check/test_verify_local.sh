#!/usr/bin/env bash
# Offline contract tests for verify.sh input validation (no network).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERIFY="${ROOT}/verify.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# Missing expected-head shape
set +e
INPUT_PULL_REQUEST=1 INPUT_EXPECTED_HEAD=abc INPUT_REPOSITORY=o/r \
  GITHUB_REPOSITORY=o/r bash "${VERIFY}" >"${TMP}/out" 2>"${TMP}/err"
code=$?
set -e
[[ "${code}" -ne 0 ]] || fail "expected invalid SHA to fail"
grep -q "40-char" "${TMP}/err" || fail "expected SHA validation message"

# Non-numeric PR
set +e
INPUT_PULL_REQUEST=nope INPUT_EXPECTED_HEAD="$(printf 'a%.0s' {1..40})" INPUT_REPOSITORY=o/r \
  bash "${VERIFY}" >"${TMP}/out" 2>"${TMP}/err"
code=$?
set -e
[[ "${code}" -ne 0 ]] || fail "expected non-numeric PR to fail"

echo "exact-head-check local validation tests passed"
