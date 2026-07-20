#!/usr/bin/env bash
# Fail-closed exact PR head verification for the composite action.
set -euo pipefail

repo="${INPUT_REPOSITORY:-}"
if [[ -z "${repo}" ]]; then
  repo="${GITHUB_REPOSITORY:-}"
fi
if [[ -z "${repo}" ]]; then
  echo "repository is required (input repository or GITHUB_REPOSITORY)" >&2
  exit 1
fi

pr="${INPUT_PULL_REQUEST:-}"
expected="${INPUT_EXPECTED_HEAD:-}"
allow_fork="${INPUT_ALLOW_FORK_HEAD:-false}"
proof_path="${INPUT_PROOF_PATH:-exact-head-proof.json}"

if [[ ! "${pr}" =~ ^[0-9]+$ ]]; then
  echo "pull-request must be a positive integer, got: ${pr}" >&2
  exit 1
fi
if [[ ! "${expected}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "expected-head must be a 40-char lowercase hex SHA, got: ${expected}" >&2
  exit 1
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "gh CLI is required on the runner" >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required on the runner" >&2
  exit 1
fi

api_path="repos/${repo}/pulls/${pr}"
raw="$(gh api "${api_path}" --jq '{
  number: .number,
  state: .state,
  head_sha: .head.sha,
  head_ref: .head.ref,
  head_repo: .head.repo.full_name,
  base_repo: .base.repo.full_name,
  base_ref: .base.ref,
  html_url: .html_url
}')"

live_head="$(echo "${raw}" | jq -r '.head_sha // empty')"
head_repo="$(echo "${raw}" | jq -r '.head_repo // empty')"
base_repo="$(echo "${raw}" | jq -r '.base_repo // empty')"
state="$(echo "${raw}" | jq -r '.state // empty')"
number="$(echo "${raw}" | jq -r '.number // empty')"

if [[ -z "${live_head}" || ! "${live_head}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "live head SHA missing or invalid from ${api_path}" >&2
  exit 1
fi
if [[ "${number}" != "${pr}" ]]; then
  echo "PR number mismatch: expected ${pr}, API returned ${number}" >&2
  exit 1
fi
if [[ "${state}" != "open" ]]; then
  echo "PR #${pr} is not open (state=${state}); refusing exact-head pass" >&2
  exit 1
fi
if [[ "${allow_fork}" != "true" && -n "${head_repo}" && -n "${base_repo}" && "${head_repo}" != "${base_repo}" ]]; then
  echo "fork head repository ${head_repo} differs from base ${base_repo}; set allow-fork-head=true to permit" >&2
  exit 1
fi

status="pass"
reason="exact_head_match"
if [[ "${live_head}" != "${expected}" ]]; then
  status="fail"
  reason="head_moved"
fi

proof="$(jq -n \
  --arg kind "exact-head-check-proof.v1" \
  --arg status "${status}" \
  --arg reason "${reason}" \
  --arg repository "${repo}" \
  --argjson pull_request "${pr}" \
  --arg expected_head "${expected}" \
  --arg live_head "${live_head}" \
  --arg head_repository "${head_repo}" \
  --arg base_repository "${base_repo}" \
  --arg pr_state "${state}" \
  --arg workflow "${GITHUB_WORKFLOW:-}" \
  --arg run_id "${GITHUB_RUN_ID:-}" \
  --arg run_attempt "${GITHUB_RUN_ATTEMPT:-}" \
  --arg event_name "${GITHUB_EVENT_NAME:-}" \
  --arg github_sha "${GITHUB_SHA:-}" \
  '{
    kind: $kind,
    status: $status,
    reason: $reason,
    repository: $repository,
    pull_request: $pull_request,
    expected_head: $expected_head,
    live_head: $live_head,
    head_repository: $head_repository,
    base_repository: $base_repository,
    pr_state: $pr_state,
    workflow: $workflow,
    run_id: $run_id,
    run_attempt: $run_attempt,
    event_name: $event_name,
    github_sha: $github_sha,
    merges: false,
    model_calls: false
  }')"

printf '%s\n' "${proof}" > "${proof_path}"

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    echo "## Exact-Head CI Check"
    echo ""
    echo "| Field | Value |"
    echo "|---|---|"
    echo "| Status | \`${status}\` |"
    echo "| Reason | \`${reason}\` |"
    echo "| Repository | \`${repo}\` |"
    echo "| Pull request | \`#${pr}\` |"
    echo "| Expected head | \`${expected}\` |"
    echo "| Live head | \`${live_head}\` |"
    echo "| Head repository | \`${head_repo}\` |"
    echo "| Proof | \`${proof_path}\` |"
    echo ""
    if [[ "${status}" == "pass" ]]; then
      echo "Live PR head matches the expected commit."
    else
      echo "**Fail closed:** PR head moved. Prior CI for \`${expected}\` must not authorize this head."
    fi
  } >> "${GITHUB_STEP_SUMMARY}"
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "live-head=${live_head}"
    echo "status=${status}"
    echo "proof-path=${proof_path}"
  } >> "${GITHUB_OUTPUT}"
fi

echo "exact-head-check status=${status} reason=${reason} live=${live_head} expected=${expected}"

if [[ "${status}" != "pass" ]]; then
  exit 1
fi
