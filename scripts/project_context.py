#!/usr/bin/env python3
"""Generate a compact, fail-closed repository handoff capsule."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import subprocess
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REPOSITORY = "Igzela/token-efficient-agent-harness-lab"
PACKET_ID = r"(?:PE\d+|PR\d+|TOOL)(?:-[A-Z0-9]+)+"
REQUIRED_CI_CHECKS = (
    "python-tests",
    "rust-tests",
    "pg-integration-tests",
    "typescript-tests",
    "native-runtime",
    "docker-build",
    "rust-typescript-cutover",
    "exact-head-check",
)
FAILED_CONCLUSIONS = {
    "ACTION_REQUIRED",
    "CANCELLED",
    "FAILURE",
    "SKIPPED",
    "STALE",
    "STARTUP_FAILURE",
    "TIMED_OUT",
}
PENDING_STATES = {"EXPECTED", "IN_PROGRESS", "PENDING", "QUEUED", "REQUESTED", "WAITING"}


@dataclass(frozen=True)
class CommandResult:
    ok: bool
    stdout: str
    stderr: str
    returncode: int


def run_command(command: list[str], *, timeout: int = 15) -> CommandResult:
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return CommandResult(False, "", str(error), 127)
    return CommandResult(
        result.returncode == 0,
        result.stdout.strip(),
        result.stderr.strip(),
        result.returncode,
    )


def read_text(relative_path: str) -> str:
    path = ROOT / relative_path
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def git_show_text(ref: str, relative_path: str) -> str:
    result = run_command(["git", "show", f"{ref}:{relative_path}"])
    return result.stdout if result.ok else ""


def section(text: str, heading: str) -> str:
    start = text.find(heading)
    if start < 0:
        return ""
    start += len(heading)
    end = text.find("\n## ", start)
    return text[start:] if end < 0 else text[start:end]


def parse_first_routed_packet(next_text: str) -> dict[str, str | None]:
    routing = section(next_text, "## Active Routing")
    packet_match = re.search(PACKET_ID, routing)
    if not packet_match:
        return {"packet": None, "state": None, "pr_number": None}
    packet = packet_match.group(0)
    heading = re.search(
        rf"^#{{2,3}} Packet {re.escape(packet)}\b.*$",
        next_text,
        re.MULTILINE,
    )
    block = ""
    if heading:
        next_heading = re.search(r"^#{2,3} Packet ", next_text[heading.end() :], re.MULTILINE)
        end = heading.end() + next_heading.start() if next_heading else len(next_text)
        block = next_text[heading.start() : end]
    state_match = re.search(r"^\*\*State:\*\* `([A-Z_]+)`", block, re.MULTILINE)
    structured_pr = re.search(
        r"^\*\*(?:Owned PR|Review surface):\*\*\s*#(\d+)\s*$",
        block,
        re.MULTILINE | re.IGNORECASE,
    )
    fallback_pr = re.search(r"\bPR #(\d+)\b|(?<!\w)#(\d+)\b", block)
    pr_number = None
    if structured_pr:
        pr_number = structured_pr.group(1)
    elif fallback_pr:
        pr_number = fallback_pr.group(1) or fallback_pr.group(2)
    return {
        "packet": packet,
        "state": state_match.group(1) if state_match else None,
        "pr_number": pr_number,
    }


def parse_open_frontiers(status_text: str) -> list[dict[str, Any]]:
    block = section(status_text, "## Open Review Surfaces")
    frontiers: list[dict[str, Any]] = []
    for line in block.splitlines():
        match = re.match(r"^\|\s*#(\d+)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|$", line)
        if not match:
            continue
        frontiers.append(
            {
                "pr": int(match.group(1)),
                "purpose": match.group(2).strip(),
                "documented_status": match.group(3).strip(),
            }
        )
    return frontiers


def repository_from_git() -> str:
    result = run_command(["git", "remote", "get-url", "origin"])
    if not result.ok or not result.stdout:
        return DEFAULT_REPOSITORY
    raw = result.stdout.strip()
    ssh_match = re.match(r"git@[^:]+:(?P<path>.+?)(?:\.git)?$", raw)
    if ssh_match:
        return ssh_match.group("path").removesuffix(".git")
    url_match = re.match(
        r"^[A-Za-z][A-Za-z0-9+.-]*://[^/]+/(?P<path>[^?#]+?)(?:\.git)?/?$",
        raw,
    )
    if url_match:
        path = url_match.group("path").strip("/").removesuffix(".git")
        if path.count("/") == 1:
            return path
    return DEFAULT_REPOSITORY


def accepted_baseline(*, offline: bool) -> dict[str, Any]:
    if not offline:
        remote = run_command(["git", "ls-remote", "origin", "refs/heads/main"])
        if remote.ok and remote.stdout:
            sha = remote.stdout.split()[0]
            if re.fullmatch(r"[0-9a-f]{40}", sha):
                return {
                    "branch": "main",
                    "sha": sha,
                    "availability": "confirmed",
                    "source": "git ls-remote origin refs/heads/main",
                }
    for ref in ("origin/main", "main"):
        local = run_command(["git", "rev-parse", "--verify", ref])
        if local.ok and re.fullmatch(r"[0-9a-f]{40}", local.stdout):
            return {
                "branch": "main",
                "sha": local.stdout,
                "availability": "local_only" if offline or ref != "origin/main" else "confirmed",
                "source": f"git rev-parse {ref}",
            }
    return {
        "branch": "main",
        "sha": None,
        "availability": "unavailable",
        "source": None,
    }


def ensure_commit_available(sha: str, *, offline: bool) -> bool:
    present = run_command(["git", "cat-file", "-e", f"{sha}^{{commit}}"])
    if present.ok:
        return True
    if offline:
        return False
    fetched = run_command(
        ["git", "fetch", "--no-tags", "--depth=1", "origin", sha],
        timeout=30,
    )
    if not fetched.ok:
        return False
    return run_command(["git", "cat-file", "-e", f"{sha}^{{commit}}"]).ok


def canonical_documents(baseline: dict[str, Any], *, offline: bool) -> dict[str, Any]:
    sha = baseline.get("sha")
    unavailable = {
        "availability": "unavailable",
        "source_sha": sha,
        "current_status": "",
        "next_decision": "",
    }
    if not isinstance(sha, str) or not re.fullmatch(r"[0-9a-f]{40}", sha):
        return unavailable
    if not ensure_commit_available(sha, offline=offline):
        return unavailable
    status = git_show_text(sha, "docs/CURRENT_STATUS.md")
    next_text = git_show_text(sha, "docs/NEXT_DECISION.md")
    if not status or not next_text:
        return unavailable
    return {
        "availability": baseline.get("availability", "local_only"),
        "source_sha": sha,
        "current_status": status,
        "next_decision": next_text,
    }


def _required_check_name(name: str) -> str | None:
    normalized = name.strip()
    for required in REQUIRED_CI_CHECKS:
        if normalized == required or normalized.endswith(f" / {required}"):
            return required
    return None


def load_pr(repository: str, pr_number: int, *, offline: bool) -> dict[str, Any]:
    unavailable = {
        "number": pr_number,
        "availability": "unavailable",
        "head_sha": None,
        "head_branch": None,
        "base_branch": None,
        "title": None,
        "url": None,
        "draft": None,
        "merge_state": None,
        "review_decision": None,
        "exact_head_review": {
            "state": "unavailable",
            "reason": "remote_review_state_unavailable",
        },
        "ci": {
            "state": "unavailable",
            "successful": [],
            "failed": [],
            "pending": [],
            "missing_required": list(REQUIRED_CI_CHECKS),
        },
    }
    if offline:
        return unavailable
    fields = ",".join(
        [
            "number",
            "title",
            "headRefName",
            "headRefOid",
            "baseRefName",
            "isDraft",
            "mergeStateStatus",
            "reviewDecision",
            "statusCheckRollup",
            "url",
        ]
    )
    result = run_command(
        ["gh", "pr", "view", str(pr_number), "--repo", repository, "--json", fields],
        timeout=20,
    )
    if not result.ok:
        unavailable["unavailable_reason"] = f"gh_pr_view_failed_exit_{result.returncode}"
        return unavailable
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        unavailable["unavailable_reason"] = "gh_pr_view_invalid_json"
        return unavailable
    aggregate_review = payload.get("reviewDecision") or "REVIEW_REQUIRED"
    return {
        "number": payload.get("number", pr_number),
        "availability": "confirmed",
        "head_sha": payload.get("headRefOid"),
        "head_branch": payload.get("headRefName"),
        "base_branch": payload.get("baseRefName"),
        "title": payload.get("title"),
        "url": payload.get("url"),
        "draft": payload.get("isDraft"),
        "merge_state": payload.get("mergeStateStatus"),
        "review_decision": aggregate_review,
        "exact_head_review": {
            "state": "unverified",
            "reason": "aggregate_review_decision_is_not_exact_head_bound",
        },
        "ci": summarize_checks(payload.get("statusCheckRollup") or []),
    }


def summarize_checks(checks: list[dict[str, Any]]) -> dict[str, Any]:
    successful: list[str] = []
    failed: list[str] = []
    pending: list[str] = []
    observed_required: set[str] = set()
    successful_required: set[str] = set()
    for check in checks:
        name = str(
            check.get("name")
            or check.get("context")
            or check.get("workflowName")
            or "unnamed-check"
        )
        conclusion = str(check.get("conclusion") or "").upper()
        status = str(check.get("status") or check.get("state") or "").upper()
        required_name = _required_check_name(name)
        if required_name:
            observed_required.add(required_name)
        if conclusion == "SUCCESS":
            successful.append(name)
            if required_name:
                successful_required.add(required_name)
        elif conclusion in FAILED_CONCLUSIONS:
            failed.append(name)
        elif status in PENDING_STATES or not conclusion:
            pending.append(name)
        else:
            pending.append(name)
    missing_required = sorted(set(REQUIRED_CI_CHECKS) - observed_required)
    incomplete_required = sorted(observed_required - successful_required)
    if failed:
        state = "failed"
    elif pending or incomplete_required:
        state = "pending"
    elif missing_required:
        state = "incomplete"
    elif set(REQUIRED_CI_CHECKS).issubset(successful_required):
        state = "success"
    else:
        state = "unavailable"
    return {
        "state": state,
        "successful": sorted(set(successful)),
        "failed": sorted(set(failed)),
        "pending": sorted(set(pending)),
        "missing_required": missing_required,
    }


def local_checkout_state() -> dict[str, Any]:
    head = run_command(["git", "rev-parse", "--verify", "HEAD"])
    branch = run_command(["git", "symbolic-ref", "--short", "-q", "HEAD"])
    status = run_command(["git", "status", "--porcelain"])
    changes = status.stdout.splitlines() if status.ok and status.stdout else []
    return {
        "head_sha": head.stdout if head.ok and re.fullmatch(r"[0-9a-f]{40}", head.stdout) else None,
        "branch": branch.stdout if branch.ok and branch.stdout else None,
        "detached": not branch.ok or not branch.stdout,
        "dirty": bool(changes),
        "change_count": len(changes),
    }


def next_permitted_action(packet: dict[str, Any], active_pr: dict[str, Any] | None) -> str:
    packet_id = packet.get("packet") or "the earliest eligible packet"
    state = packet.get("state")
    if state == "BLOCKED_PREREQUISITE":
        return f"resolve the named prerequisite for {packet_id}; do not implement the blocked packet"
    if not active_pr:
        return f"inspect {packet_id}, confirm ownership, and create or continue one focused PR"
    number = active_pr.get("number")
    if active_pr.get("availability") != "confirmed":
        return f"refresh PR #{number} exact head, CI, and review state before acting"
    ci = active_pr.get("ci", {})
    ci_state = ci.get("state")
    if ci_state == "failed":
        return f"repair the failing exact-head checks for PR #{number} without weakening guards"
    if ci_state == "incomplete":
        missing = ", ".join(ci.get("missing_required") or [])
        return f"obtain the missing required exact-head checks for PR #{number}: {missing}"
    if ci_state in {"pending", "unavailable"}:
        return f"complete or verify all required exact-head CI for PR #{number}, then obtain independent review"
    exact_review = active_pr.get("exact_head_review", {})
    if exact_review.get("state") != "confirmed":
        return (
            f"obtain independent acceptance for PR #{number} at exact head "
            f"{active_pr.get('head_sha')} and verify unresolved objections"
        )
    return (
        f"confirm explicit merge authority and full merge eligibility for PR #{number}; "
        "do not merge automatically"
    )


def build_capsule(*, offline: bool, repository: str | None = None) -> dict[str, Any]:
    repository = repository or repository_from_git()
    baseline = accepted_baseline(offline=offline)
    documents = canonical_documents(baseline, offline=offline)
    next_text = documents.get("next_decision", "")
    status_text = documents.get("current_status", "")
    packet = parse_first_routed_packet(next_text)
    frontiers = parse_open_frontiers(status_text)

    pr_number = int(packet["pr_number"]) if packet.get("pr_number") else None
    active_pr = load_pr(repository, pr_number, offline=offline) if pr_number else None
    blocked_frontiers = [
        frontier
        for frontier in frontiers
        if pr_number is None or frontier["pr"] != pr_number
    ]
    checkout = local_checkout_state()
    checkout["matches_accepted_baseline"] = bool(
        checkout.get("head_sha")
        and baseline.get("sha")
        and checkout.get("head_sha") == baseline.get("sha")
    )
    checkout["matches_active_frontier"] = bool(
        checkout.get("head_sha")
        and active_pr
        and active_pr.get("head_sha")
        and checkout.get("head_sha") == active_pr.get("head_sha")
    )

    if documents.get("availability") == "unavailable":
        action = "obtain the accepted-main canonical documents before selecting or advancing work"
    else:
        action = next_permitted_action(packet, active_pr)

    return {
        "schema_version": "project_context.v1",
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "repository": repository,
        "accepted_baseline": baseline,
        "canonical_document_source": {
            "availability": documents.get("availability"),
            "source_sha": documents.get("source_sha"),
        },
        "local_checkout": checkout,
        "active_packet": packet,
        "active_frontier": active_pr,
        "blocked_or_other_frontiers": blocked_frontiers,
        "next_permitted_action": action,
        "required_reading": [
            "START_HERE.md",
            "AGENTS.md when implementing or repairing code",
            "docs/CURRENT_STATUS.md from the accepted baseline",
            "docs/NEXT_DECISION.md from the accepted baseline",
            "docs/MODULE_MAP.md for ownership",
            "relevant ARCHITECTURE_BOOK or REAL_WORLD_TESTING_PLAYBOOK sections",
            "relevant code and tests",
        ],
        "hard_stops": [
            "no stale-head CI or review claims",
            "no success when a required CI check is failed, pending, skipped, or missing",
            "no aggregate approval treated as exact-head acceptance",
            "no downstream packet before its prerequisite is accepted",
            "no provider call, merge, release, deploy, or protected-branch write without explicit authority",
            "no caller-asserted authority, secret exposure, invented evidence, or weakened fail-closed behavior",
            "no second runtime, scheduler, store, evaluator, budget, approval, output, audit, or rollback owner",
        ],
        "notes": [
            "This capsule is a generated transport view, not an authority owner.",
            "Current status and routing are read from the accepted baseline, not branch-local prose.",
            "Unavailable remote facts are reported as unavailable rather than inferred.",
            "The generator is on-demand; CI does not yet inject the capsule into later Agent sessions.",
        ],
    }


def markdown(capsule: dict[str, Any]) -> str:
    baseline = capsule["accepted_baseline"]
    document_source = capsule.get("canonical_document_source", {})
    checkout = capsule.get("local_checkout", {})
    packet = capsule["active_packet"]
    frontier = capsule.get("active_frontier")
    lines = [
        "# Project Context Capsule",
        "",
        f"- Repository: `{capsule['repository']}`",
        (
            f"- Accepted baseline: `{baseline.get('sha') or 'unavailable'}` "
            f"({baseline.get('availability')}, {baseline.get('source') or 'no source'})"
        ),
        (
            f"- Canonical documents: `{document_source.get('source_sha') or 'unavailable'}` "
            f"availability=`{document_source.get('availability') or 'unavailable'}`"
        ),
        (
            f"- Local checkout: head=`{checkout.get('head_sha') or 'unavailable'}` "
            f"branch=`{checkout.get('branch') or 'detached'}` dirty=`{checkout.get('dirty')}`"
        ),
        (
            f"- Active packet: `{packet.get('packet') or 'unavailable'}` "
            f"state=`{packet.get('state') or 'unavailable'}`"
        ),
    ]
    if frontier:
        ci = frontier.get("ci", {})
        exact_review = frontier.get("exact_head_review", {})
        lines.extend(
            [
                (
                    f"- Active PR: `#{frontier.get('number')}` "
                    f"head=`{frontier.get('head_sha') or 'unavailable'}` "
                    f"availability=`{frontier.get('availability')}`"
                ),
                (
                    f"- CI: `{ci.get('state', 'unavailable')}`; "
                    f"missing_required=`{','.join(ci.get('missing_required') or []) or 'none'}`"
                ),
                (
                    f"- Review: aggregate=`{frontier.get('review_decision') or 'unavailable'}`; "
                    f"exact_head=`{exact_review.get('state') or 'unavailable'}`"
                ),
            ]
        )
    else:
        lines.append("- Active PR: `unavailable`")
    lines.extend(
        [
            f"- Next permitted action: {capsule['next_permitted_action']}",
            "",
            "## Required reading",
        ]
    )
    lines.extend(f"- {item}" for item in capsule["required_reading"])
    lines.extend(["", "## Hard stops"])
    lines.extend(f"- {item}" for item in capsule["hard_stops"])
    lines.extend(["", "## Other documented frontiers"])
    if capsule["blocked_or_other_frontiers"]:
        for item in capsule["blocked_or_other_frontiers"]:
            lines.append(
                f"- PR #{item['pr']}: {item['purpose']} — {item['documented_status']}"
            )
    else:
        lines.append("- None discovered in accepted `docs/CURRENT_STATUS.md`.")
    lines.extend(["", *[f"> {note}" for note in capsule["notes"]]])
    return "\n".join(lines) + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--format", choices=("markdown", "json"), default="markdown")
    parser.add_argument(
        "--offline",
        action="store_true",
        help="Do not contact the remote Git repository or GitHub CLI.",
    )
    parser.add_argument("--repo", help="Override owner/repository for GitHub lookups.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    capsule = build_capsule(offline=args.offline, repository=args.repo)
    if args.format == "json":
        print(json.dumps(capsule, indent=2, sort_keys=True))
    else:
        print(markdown(capsule), end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
