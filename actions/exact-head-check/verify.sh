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
require_review="${INPUT_REQUIRE_REVIEW_RECEIPT:-false}"
proof_path="${INPUT_PROOF_PATH:-exact-head-proof.json}"

if [[ ! "${pr}" =~ ^[0-9]+$ ]]; then
  echo "pull-request must be a positive integer, got: ${pr}" >&2
  exit 1
fi
if [[ ! "${expected}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "expected-head must be a 40-char lowercase hex SHA, got: ${expected}" >&2
  exit 1
fi
if [[ "${require_review}" != "true" && "${require_review}" != "false" ]]; then
  echo "require-review-receipt must be true or false, got: ${require_review}" >&2
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
if [[ "${require_review}" == "true" ]] && ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required when review receipt validation is enabled" >&2
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
  base_sha: .base.sha,
  pr_author: .user.login,
  html_url: .html_url
}')"

live_head="$(echo "${raw}" | jq -r '.head_sha // empty')"
head_repo="$(echo "${raw}" | jq -r '.head_repo // empty')"
base_repo="$(echo "${raw}" | jq -r '.base_repo // empty')"
base_sha="$(echo "${raw}" | jq -r '.base_sha // empty')"
pr_author="$(echo "${raw}" | jq -r '.pr_author // empty')"
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

review_receipt_status="not_required"
if [[ "${status}" == "pass" && "${require_review}" == "true" ]]; then
  if [[ ! "${base_sha}" =~ ^[0-9a-f]{40}$ || -z "${pr_author}" ]]; then
    echo "PR base SHA or author identity missing; refusing review receipt validation" >&2
    exit 1
  fi
  review_tmp="$(mktemp -d)"
  trap 'rm -rf "${review_tmp}"' EXIT
  gh api --paginate --slurp "repos/${repo}/issues/${pr}/comments?per_page=100" > "${review_tmp}/issue-comments.json"
  gh api --paginate --slurp "repos/${repo}/pulls/${pr}/comments?per_page=100" > "${review_tmp}/review-comments.json"
  gh api --paginate --slurp "repos/${repo}/pulls/${pr}/reviews?per_page=100" > "${review_tmp}/reviews.json"
  repo_root="$(cd "${GITHUB_ACTION_PATH}/../.." && pwd)"
  python3 - "${repo_root}" "${review_tmp}" "${expected}" "${base_sha}" "${pr_author}" <<'PY'
import importlib.util
import json
from datetime import datetime, timezone
from pathlib import Path
import sys

repo_root = Path(sys.argv[1])
tmp = Path(sys.argv[2])
head = sys.argv[3]
base = sys.argv[4]
pr_author = sys.argv[5]
script = repo_root / "scripts" / "project_context.py"
spec = importlib.util.spec_from_file_location("trusted_project_context", script)
if spec is None or spec.loader is None:
    raise SystemExit("trusted review parser unavailable")
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)

def flatten(path: Path):
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, list):
        raise SystemExit(f"invalid GitHub pagination payload: {path.name}")
    result = []
    for page in value:
        if not isinstance(page, list):
            raise SystemExit(f"invalid GitHub pagination page: {path.name}")
        result.extend(page)
    return result

issue_comments = flatten(tmp / "issue-comments.json")
review_comments = flatten(tmp / "review-comments.json")
reviews = flatten(tmp / "reviews.json")
marker = module.REVIEW_RECEIPT_MARKER
receipt_marker_count = sum(
    str(item.get("body") or "").count(marker)
    for item in issue_comments + review_comments
)
if receipt_marker_count != 1:
    raise SystemExit(
        "exact-head review receipt invalid: expected exactly one receipt marker, "
        f"observed {receipt_marker_count}"
    )
states = {str(item.get("state") or "").upper() for item in reviews}
aggregate = "CHANGES_REQUESTED" if "CHANGES_REQUESTED" in states else "APPROVED" if "APPROVED" in states else "REVIEW_REQUIRED"
observation = module._build_review_observation(
    head_sha=head,
    base_sha=base,
    pr_author_identity=pr_author,
    aggregate_review=aggregate,
    reviews=reviews,
    comments=issue_comments + review_comments,
    observation_time=datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
)
receipt = observation.get("review_receipt") or {}
if observation.get("exact_head_review_state") != "confirmed" or receipt.get("state") != "valid":
    errors = receipt.get("errors") or [observation.get("unavailable_reason") or "review_receipt_not_confirmed"]
    raise SystemExit("exact-head review receipt invalid: " + ",".join(map(str, errors)))
if str(receipt.get("reviewer_authenticated_identity") or "").lower() != str(receipt.get("reviewer_author_identity") or "").lower():
    raise SystemExit("exact-head review authenticated identity does not match GitHub comment author")
print("trusted exact-head review receipt confirmed")
PY
  review_receipt_status="confirmed"
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
  --arg review_receipt_status "${review_receipt_status}" \
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
    review_receipt_status: $review_receipt_status,
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
    echo "| Review receipt | \`${review_receipt_status}\` |"
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

echo "exact-head-check status=${status} reason=${reason} review=${review_receipt_status} live=${live_head} expected=${expected}"

if [[ "${status}" != "pass" ]]; then
  exit 1
fi
