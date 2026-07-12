"""Build Codex prompts from Issue task specifications and current runtime evidence.
"""

import json
import os
import pathlib
import subprocess
import sys


PROMPT_DIR = pathlib.Path(__file__).resolve().parent / "prompts"

REPO_OWNER = os.environ.get("AGENT_REPO_OWNER", "Igzela")
REPO_NAME = os.environ.get("AGENT_REPO_NAME", "token-efficient-agent-harness-lab")


def _gh(*args):
    cmd = ["gh"] + list(args)
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        return result.stdout.strip() if result.returncode == 0 else None
    except (subprocess.TimeoutExpired, OSError):
        return None


def _read_repo_file(path):
    result = _gh("repo", "view", "--json", "files", "--jq", f'.files[] | select(.path=="{path}") | .text')
    if result:
        try:
            return json.loads(result)
        except json.JSONDecodeError:
            return result
    content = _gh("api", f"/repos/{REPO_OWNER}/{REPO_NAME}/contents/{path}")
    if content:
        try:
            return json.loads(content).get("content", "")
        except (json.JSONDecodeError, KeyError):
            pass
    return None


def _cat(filepath):
    try:
        return pathlib.Path(filepath).read_text()
    except OSError:
        return None


def build_context(issue_number):
    ctx = {
        "issue": {"number": issue_number, "title": "", "body": "", "labels": []},
        "governance": {},
        "verification": [],
        "modules": {},
    }

    issue_json = _gh("issue", "view", str(issue_number), "--json", "title,body,labels")
    if issue_json:
        try:
            parsed = json.loads(issue_json)
            ctx["issue"]["title"] = parsed.get("title", "")
            ctx["issue"]["body"] = parsed.get("body", "")
            ctx["issue"]["labels"] = [l["name"] for l in parsed.get("labels", [])]
        except json.JSONDecodeError:
            pass

    for path in ["AGENTS.md", "docs/CURRENT_STATUS.md", "docs/NEXT_DECISION.md", "docs/MODULE_MAP.md"]:
        content = _cat(path) or _read_repo_file(path) or ""
        ctx[path.replace("/", "_").replace(".", "_")] = content

    return ctx


def load_prompt_template(template_name):
    template_path = PROMPT_DIR / template_name
    if not template_path.exists():
        alt = PROMPT_DIR / f"{template_name}.md"
        if alt.exists():
            template_path = alt
        else:
            print(f"Template not found: {template_name}", file=sys.stderr)
            return None
    return template_path.read_text()


def build_implementation_prompt(issue_number, template="implementation.md"):
    ctx = build_context(issue_number)
    template_text = load_prompt_template(template)
    if not template_text:
        return None

    prompt = template_text
    prompt = prompt.replace("{{ISSUE_NUMBER}}", str(issue_number))
    prompt = prompt.replace("{{ISSUE_TITLE}}", ctx["issue"]["title"])
    prompt = prompt.replace("{{ISSUE_BODY}}", ctx["issue"]["body"])
    prompt = prompt.replace("{{AGENTS_MD}}", ctx.get("AGENTS_md", ""))
    prompt = prompt.replace("{{CURRENT_STATUS}}", ctx.get("docs_CURRENT_STATUS_md", ""))
    prompt = prompt.replace("{{NEXT_DECISION}}", ctx.get("docs_NEXT_DECISION_md", ""))
    prompt = prompt.replace("{{MODULE_MAP}}", ctx.get("docs_MODULE_MAP_md", ""))
    prompt = prompt.replace("{{REPO_NAME}}", f"{REPO_OWNER}/{REPO_NAME}")
    prompt = prompt.replace("{{GIT_BRANCH}}", os.environ.get("AGENT_BRANCH", "main"))

    return prompt


def build_ci_repair_prompt(pr_number, head_sha, failed_jobs_json, logs, repair_count, template="ci_repair.md"):
    ctx = build_context(0)
    ctx["pr_number"] = pr_number
    ctx["head_sha"] = head_sha
    ctx["failed_jobs"] = failed_jobs_json
    ctx["logs"] = logs[:50000] if logs else ""
    ctx["repair_count"] = repair_count

    template_text = load_prompt_template(template)
    if not template_text:
        return None

    prompt = template_text
    prompt = prompt.replace("{{PR_NUMBER}}", str(pr_number))
    prompt = prompt.replace("{{HEAD_SHA}}", head_sha)
    prompt = prompt.replace("{{FAILED_JOBS}}", failed_jobs_json if isinstance(failed_jobs_json, str) else json.dumps(failed_jobs_json))
    prompt = prompt.replace("{{LOGS}}", ctx["logs"])
    prompt = prompt.replace("{{REPAIR_COUNT}}", str(repair_count))
    prompt = prompt.replace("{{REPO_NAME}}", f"{REPO_OWNER}/{REPO_NAME}")
    prompt = prompt.replace("{{AGENTS_MD}}", ctx.get("AGENTS_md", ""))

    return prompt


def build_review_prompt(pr_number, head_sha, template="review.md"):
    ctx = build_context(0)
    ctx["pr_number"] = pr_number
    ctx["head_sha"] = head_sha

    diff = _gh("pr", "diff", str(pr_number))
    ctx["diff"] = diff or ""

    pr_json = _gh("pr", "view", str(pr_number), "--json", "title,body,files,reviews,comments")
    if pr_json:
        try:
            parsed = json.loads(pr_json)
            ctx["pr_title"] = parsed.get("title", "")
            ctx["pr_body"] = parsed.get("body", "")
            ctx["pr_files"] = parsed.get("files", [])
            ctx["pr_reviews"] = parsed.get("reviews", [])
            ctx["pr_comments"] = parsed.get("comments", [])
        except json.JSONDecodeError:
            pass

    schema_path = pathlib.Path(__file__).resolve().parent / "review_schema.json"
    if schema_path.exists():
        ctx["review_schema"] = schema_path.read_text()

    template_text = load_prompt_template(template)
    if not template_text:
        return None

    prompt = template_text
    prompt = prompt.replace("{{PR_NUMBER}}", str(pr_number))
    prompt = prompt.replace("{{HEAD_SHA}}", head_sha)
    prompt = prompt.replace("{{DIFF}}", ctx["diff"][:100000])
    prompt = prompt.replace("{{REPO_NAME}}", f"{REPO_OWNER}/{REPO_NAME}")
    prompt = prompt.replace("{{AGENTS_MD}}", ctx.get("AGENTS_md", ""))

    if ctx.get("review_schema"):
        prompt += "\n\n### Authoritative Schema\n\n```json\n" + ctx["review_schema"] + "\n```\n"

    return prompt


def main():
    if len(sys.argv) < 3:
        print("Usage: prompt_builder.py <implementation|ci-repair|review> <issue_or_pr> [sha] [failed_jobs_json]",
              file=sys.stderr)
        sys.exit(1)

    command = sys.argv[1]
    number = int(sys.argv[2])

    if command == "implementation":
        prompt = build_implementation_prompt(number)
        if prompt:
            print(prompt)
        else:
            sys.exit(1)

    elif command == "ci-repair":
        sha = sys.argv[3] if len(sys.argv) > 3 else ""
        failed_jobs = sys.argv[4] if len(sys.argv) > 4 else "[]"
        repair_count = int(os.environ.get("AGENT_REPAIR_COUNT", "0"))
        logs = os.environ.get("AGENT_FAILED_LOGS", "")
        prompt = build_ci_repair_prompt(number, sha, failed_jobs, logs, repair_count)
        if prompt:
            print(prompt)
        else:
            sys.exit(1)

    elif command == "review":
        sha = sys.argv[3] if len(sys.argv) > 3 else ""
        prompt = build_review_prompt(number, sha)
        if prompt:
            print(prompt)
        else:
            sys.exit(1)

    else:
        print(f"Unknown prompt type: {command}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
