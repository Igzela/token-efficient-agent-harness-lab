"""CI event processing for the agent orchestrator.

Determines the next action when a workflow run completes:
- success + final-review label -> ready to merge
- success + no final-review -> trigger final review
- failure + retries < MAX -> trigger CI repair
- failure + retries >= MAX -> mark blocked
"""

import json
import os
import sys

import state_manager as sm

GH_REPO = os.environ.get("GITHUB_REPOSITORY", "")
MAX_REPAIR_ATTEMPTS = int(os.environ.get("AGENT_MAX_REPAIR_ATTEMPTS", "2"))


def parse_workflow_run_event(event_path):
    with open(event_path) as f:
        event = json.load(f)

    workflow_run = event.get("workflow_run", {})
    conclusion = workflow_run.get("conclusion")
    status = workflow_run.get("status")
    head_branch = workflow_run.get("head_branch", "")
    head_sha = workflow_run.get("head_sha", "")
    run_id = workflow_run.get("id")
    run_url = workflow_run.get("html_url", "")
    triggering_actor = workflow_run.get("triggering_actor", {}).get("login", "")

    pull_requests = workflow_run.get("pull_requests", [])
    pr_data = pull_requests[0] if pull_requests else {}

    return {
        "conclusion": conclusion,
        "status": status,
        "head_branch": head_branch,
        "head_sha": head_sha,
        "run_id": run_id,
        "run_url": run_url,
        "triggering_actor": triggering_actor,
        "pr_number": pr_data.get("number"),
        "pr_head_sha": pr_data.get("head", {}).get("sha") if isinstance(pr_data, dict) else None,
    }


def get_failed_jobs(run_id):
    result = sm._gh("run", "view", str(run_id), "--json", "jobs", "--log")
    if not result:
        return []
    try:
        data = json.loads(result)
        failed = []
        for job in data.get("jobs", []):
            if job.get("conclusion") == "failure":
                steps = [s.get("name", "") for s in job.get("steps", []) if s.get("conclusion") == "failure"]
                failed.append({"name": job.get("name", ""), "failed_steps": steps})
        return failed
    except (json.JSONDecodeError, KeyError):
        return []


def get_failed_job_logs(run_id, max_chars=100000):
    log = sm._gh("run", "view", str(run_id), "--log-failed")
    if not log:
        return ""
    return log[:max_chars]


def process_ci_completion(event_path):
    info = parse_workflow_run_event(event_path)

    pr_number = info["pr_number"]
    if not pr_number:
        print("no PR associated with this workflow run")
        return {"action": "noop", "reason": "no_pr"}

    pr_info = sm.get_pr_info(pr_number)
    if not pr_info:
        print(f"cannot find PR #{pr_number}")
        return {"action": "noop", "reason": "pr_not_found"}

    if pr_info.get("state") != "OPEN":
        print(f"PR #{pr_number} is not open ({pr_info.get('state')})")
        return {"action": "noop", "reason": "pr_not_open"}

    current_head = pr_info.get("headRefOid", "")
    expected_head = info["pr_head_sha"] or info["head_sha"]

    if current_head != expected_head:
        print(f"head SHA mismatch: current={current_head[:12]} expected={expected_head[:12]}")
        return {"action": "stale", "reason": "head_sha_mismatch"}

    print(f"PR #{pr_number}, head={current_head[:12]}, conclusion={info['conclusion']}")

    issue_number = _find_issue_for_pr(pr_number)
    if not issue_number:
        print(f"no associated issue found for PR #{pr_number}")
        return {"action": "noop", "reason": "no_associated_issue"}

    issue_labels = sm.get_issue_labels(issue_number)

    if info["conclusion"] == "success":
        if sm.LABEL_FINAL_REVIEW in issue_labels:
            return {
                "action": "merge_ready",
                "pr_number": pr_number,
                "issue_number": issue_number,
                "head_sha": current_head,
                "reason": "ci_green_with_review",
            }
        else:
            return {
                "action": "trigger_review",
                "pr_number": pr_number,
                "issue_number": issue_number,
                "head_sha": current_head,
                "reason": "ci_green_needs_review",
            }

    else:
        state = sm.read_ci_state(issue_number)
        repair_count = 0
        if state and state.get("status") == "failure":
            repair_count = state.get("extra", {}).get("repair_count", 0)
        repair_count += 1

        if repair_count > MAX_REPAIR_ATTEMPTS:
            return {
                "action": "blocked",
                "pr_number": pr_number,
                "issue_number": issue_number,
                "head_sha": current_head,
                "repair_count": repair_count,
                "reason": f"max_repairs_exceeded ({repair_count}/{MAX_REPAIR_ATTEMPTS})",
            }

        failed_jobs = get_failed_jobs(info["run_id"])
        logs = get_failed_job_logs(info["run_id"])

        return {
            "action": "trigger_repair",
            "pr_number": pr_number,
            "issue_number": issue_number,
            "head_sha": current_head,
            "ci_run_id": info["run_id"],
            "failed_jobs": failed_jobs,
            "logs": logs,
            "repair_count": repair_count,
            "reason": "ci_failure",
        }


def _find_issue_for_pr(pr_number):
    pr_info = sm.get_pr_info(pr_number)
    if not pr_info:
        return None

    result = sm._gh("pr", "view", str(pr_number), "--json", "body")
    if not result:
        return None

    import re
    try:
        body = json.loads(result).get("body", "")
    except json.JSONDecodeError:
        return None

    for match in re.finditer(r"(?:Closes|Fixes|Resolves|Implements|for)\s+#(\d+)", body, re.IGNORECASE):
        return int(match.group(1))

    branch = pr_info.get("headRefName", "")
    branch_match = re.search(r"issue[_-](\d+)", branch)
    if branch_match:
        return int(branch_match.group(1))

    return None


def main():
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    if not event_path:
        print("GITHUB_EVENT_PATH not set", file=sys.stderr)
        sys.exit(1)

    result = process_ci_completion(event_path)
    print(json.dumps(result))

    if result.get("action") in ("stale", "noop"):
        sys.exit(0)

    output_file = os.environ.get("GITHUB_OUTPUT")
    if output_file:
        with open(output_file, "a") as f:
            for key, value in result.items():
                if isinstance(value, (dict, list)):
                    value_str = json.dumps(value)
                else:
                    value_str = str(value)
                f.write(f"{key}={value_str}\n")


if __name__ == "__main__":
    import json as json_mod
    json = json_mod
    main()
