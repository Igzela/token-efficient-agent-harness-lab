"""Build Codex prompts from Issue task specifications and current runtime evidence.
"""

import json
import os
import pathlib
import subprocess
import sys
import tempfile


PROMPT_DIR = pathlib.Path(__file__).resolve().parent / "prompts"
PROJECT_ROOT = pathlib.Path(__file__).resolve().parents[2]
PROJECT_CONTEXT_SCRIPT = PROJECT_ROOT / "scripts" / "project_context.py"

REPO_OWNER = os.environ.get("AGENT_REPO_OWNER", "Igzela")
REPO_NAME = os.environ.get("AGENT_REPO_NAME", "token-efficient-agent-harness-lab")
MAX_REVIEW_DIFF_CHARS = 100_000
MAX_CAPSULE_CHARS = 100_000


def _project_context_paths() -> tuple[pathlib.Path, pathlib.Path]:
    """Locate the checked-out context generator for this prompt invocation.

    Control scripts are sometimes copied into a temporary directory by tests or
    workflow tooling. Their ``__file__`` location is then not a repository
    boundary, so prefer an explicit root or the invoking working tree before
    falling back to the script's normal checked-in location.
    """
    configured_root = os.environ.get("AGENT_PROJECT_ROOT")
    candidates = []
    if configured_root:
        candidates.append(pathlib.Path(configured_root))
    candidates.extend((pathlib.Path.cwd(), PROJECT_ROOT))

    for root in candidates:
        root = root.resolve()
        script = root / "scripts" / "project_context.py"
        if script.is_file():
            return root, script
    raise ValueError("Context capsule generator is unavailable from this working tree")


def generate_fresh_capsule(
    *,
    offline: bool = False,
    required_pr_number: int | None = None,
    required_head_sha: str | None = None,
    expected_packet: str | None = None,
) -> str:
    """Generate and validate a fresh bounded context capsule.

    Regenerates on every invocation. Does not blindly reuse an artifact.
    Validates the requested PR/head/packet when provided and refuses prompt
    construction on mismatch.
    """
    project_root, project_context_script = _project_context_paths()
    command = [sys.executable, str(project_context_script), "--format", "json"]
    if offline:
        command.append("--offline")
    if required_pr_number is not None:
        command.extend(("--pr-number", str(required_pr_number)))
    if required_head_sha:
        command.extend(("--expected-head-sha", required_head_sha))
    result = subprocess.run(
        command,
        capture_output=True,
        text=True,
        timeout=30,
        cwd=project_root,
    )
    if result.returncode != 0:
        raise ValueError(f"Context capsule generation failed: {result.stderr}")
    raw_json = result.stdout
    if len(raw_json) > MAX_CAPSULE_CHARS:
        raise ValueError(
            f"Context capsule JSON exceeds {MAX_CAPSULE_CHARS} characters"
        )
    try:
        capsule = json.loads(raw_json)
    except json.JSONDecodeError as exc:
        raise ValueError("Context capsule is not valid JSON") from exc

    checkout_sha = capsule.get("local_checkout", {}).get("head_sha")
    pr_head_sha = capsule.get("binding", {}).get("pr_exact_head", {}).get("head_sha")
    workflow_sha = os.environ.get("GITHUB_SHA")
    active_pr_number = capsule.get("active_frontier", {}).get("number")
    canonical_packet = capsule.get("active_packet", {}).get("packet")

    if required_head_sha:
        # Prefer authoritative PR head from GitHub API, then workflow SHA, then
        # local checkout. Fail closed only when an authoritative value is
        # available and disagrees.
        authoritative_head = pr_head_sha or workflow_sha
        if authoritative_head and authoritative_head != required_head_sha:
            raise ValueError(
                f"Authoritative head {authoritative_head} does not match required head {required_head_sha}"
            )
        if (
            not authoritative_head
            and checkout_sha
            and checkout_sha != required_head_sha
            and os.environ.get("GITHUB_RUN_ID")
        ):
            raise ValueError(
                f"Checkout SHA {checkout_sha} does not match required head {required_head_sha}"
            )
    if required_pr_number is not None:
        frontier_available = (
            capsule.get("active_frontier", {}).get("availability") == "confirmed"
        )
        if (
            frontier_available
            and active_pr_number is not None
            and active_pr_number != required_pr_number
        ):
            raise ValueError(
                f"Active PR #{active_pr_number} does not match required PR #{required_pr_number}"
            )
    if expected_packet:
        if canonical_packet and canonical_packet != expected_packet:
            raise ValueError(
                f"Canonical routed packet {canonical_packet} does not match expected {expected_packet}"
            )

    with tempfile.TemporaryDirectory(prefix="context-capsule-") as temp_dir:
        snapshot_path = pathlib.Path(temp_dir) / "capsule.json"
        snapshot_path.write_text(raw_json, encoding="utf-8")
        command = [
            sys.executable,
            str(project_context_script),
            "--format",
            "markdown",
            "--capsule-json",
            str(snapshot_path),
        ]
        result = subprocess.run(
            command,
            capture_output=True,
            text=True,
            timeout=30,
            cwd=project_root,
        )
    if result.returncode != 0:
        raise ValueError(
            f"Context capsule markdown rendering failed: {result.stderr}"
        )
    return result.stdout[:MAX_CAPSULE_CHARS]


def _prepend_capsule(prompt: str, capsule: str, *, workflow_bound_note: str = "") -> str:
    """Prepend a bounded, non-authoritative capsule before task-specific content."""
    if len(capsule) > MAX_CAPSULE_CHARS:
        raise ValueError(
            f"Capsule exceeds {MAX_CAPSULE_CHARS} character bound"
        )
    note = ""
    if workflow_bound_note:
        note = f"\n\n> Current workflow/session-bound context: {workflow_bound_note}"
    return (
        "## Fresh Repository Context Capsule (non-authoritative transport context)\n\n"
        f"{capsule}"
        f"{note}\n\n"
        "---\n\n"
        f"{prompt}"
    )


def _gh(*args):
    cmd = ["gh"] + list(args)
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        return result.stdout.strip() if result.returncode == 0 else None
    except (subprocess.TimeoutExpired, OSError):
        return None


def fetch_bounded_failed_logs(run_id, max_chars=50000):
    if not run_id:
        return ""
    result = _gh("run", "view", str(run_id), "--log-failed")
    return (result or "")[:max_chars]


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


def _detect_task_requires_governance(issue_body):
    """Return True only when the Issue body references code, runtime, or authority owners."""
    indicators = (
        "src/", "tests/", "engine/", "dashboard/", ".github/", "migration",
        "schema", "release", "authority", "permission", "runtime",
        "scheduler", "storage", "provider", "evaluator", "budget",
        "cargo", "clippy", "bun test",
    )
    lower = (issue_body or "").lower().replace(" ", "")
    return any(indicator in lower for indicator in indicators)


def _build_task_context(issue_body, agents_md, current_status, next_decision, module_map):
    """Select task-relevant context only. Avoid drowning small tasks in governance."""
    if _detect_task_requires_governance(issue_body):
        parts = []
        if agents_md:
            parts.append("### Repository governance\n\n```\n" + agents_md[:6000] + "\n```")
        if current_status:
            parts.append("### Current capability status\n\n```\n" + current_status[:4000] + "\n```")
        if module_map:
            parts.append("### Module ownership\n\n```\n" + module_map[:2000] + "\n```")
        return "\n\n".join(parts) if parts else "No additional context."
    lines = []
    body_lower = (issue_body or "").lower()
    if agents_md and any(word in body_lower for word in ("agent", "orchestrator", "runner", "workflow", "control")):
        lines.append("From AGENTS.md: " + _extract_relevant_lines(agents_md, ["orchestrator", "worker", "runner", "artifact"]))
    if lines:
        return "\n".join(lines)
    return "No additional context required for this task."


def _extract_relevant_lines(text, keywords, max_lines=20):
    found = []
    for line in text.splitlines():
        if any(kw in line.lower() for kw in keywords):
            found.append(line)
    return "\n".join(found[:max_lines]) if found else ""


def build_implementation_prompt(issue_number, template="implementation.md"):
    ctx = build_context(issue_number)
    template_text = load_prompt_template(template)
    if not template_text:
        return None

    task_context = _build_task_context(
        ctx["issue"]["body"],
        ctx.get("AGENTS_md", ""),
        ctx.get("docs_CURRENT_STATUS_md", ""),
        ctx.get("docs_NEXT_DECISION_md", ""),
        ctx.get("docs_MODULE_MAP_md", ""),
    )

    prompt = template_text
    prompt = prompt.replace("{{ISSUE_NUMBER}}", str(issue_number))
    prompt = prompt.replace("{{ISSUE_TITLE}}", ctx["issue"]["title"])
    prompt = prompt.replace("{{ISSUE_BODY}}", ctx["issue"]["body"])
    prompt = prompt.replace("{{TASK_CONTEXT}}", task_context)
    prompt = prompt.replace("{{REPO_NAME}}", f"{REPO_OWNER}/{REPO_NAME}")
    prompt = prompt.replace("{{GIT_BRANCH}}", os.environ.get("AGENT_BRANCH", "main"))

    capsule = generate_fresh_capsule(offline=False)
    return _prepend_capsule(
        prompt,
        capsule,
        workflow_bound_note=f"Issue #{issue_number}",
    )


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

    capsule = generate_fresh_capsule(
        # CI-repair renders PR-head code on a self-hosted worker. Keep GitHub
        # credentials out of that renderer; the checked-out exact head and
        # dispatch-bound GITHUB_SHA remain the fail-closed binding evidence.
        offline=True,
        required_pr_number=pr_number,
        required_head_sha=head_sha,
    )
    return _prepend_capsule(
        prompt,
        capsule,
        workflow_bound_note=f"CI repair for PR #{pr_number} at {head_sha}",
    )


def build_ci_repair_prompt_from_evidence(pr_number, head_sha, evidence_path, repair_count):
    """Use only the GitHub-hosted preparation artifact on Vader."""
    try:
        evidence = json.loads(pathlib.Path(evidence_path).read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError("repair evidence artifact is invalid") from exc
    if evidence.get("schema_version") != 1 or not isinstance(evidence.get("failed_jobs"), list):
        raise ValueError("repair evidence artifact has an invalid schema")
    logs = evidence.get("logs", "")
    if not isinstance(logs, str):
        raise ValueError("repair evidence logs are invalid")
    return build_ci_repair_prompt(pr_number, head_sha, evidence["failed_jobs"], logs[:50000], repair_count)


def build_review_prompt(pr_number, head_sha, template="review.md"):
    ctx = build_context(0)
    ctx["pr_number"] = pr_number
    ctx["head_sha"] = head_sha

    diff = _gh("pr", "diff", str(pr_number))
    if diff is None:
        raise ValueError("complete PR diff is unavailable")
    if len(diff) > MAX_REVIEW_DIFF_CHARS:
        raise ValueError(
            f"complete PR diff exceeds the {MAX_REVIEW_DIFF_CHARS}-character review bound"
        )
    ctx["diff"] = diff

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
    prompt = prompt.replace("{{DIFF}}", ctx["diff"])
    prompt = prompt.replace("{{REPO_NAME}}", f"{REPO_OWNER}/{REPO_NAME}")
    prompt = prompt.replace("{{AGENTS_MD}}", ctx.get("AGENTS_md", ""))

    if ctx.get("review_schema"):
        prompt += "\n\n### Authoritative Schema\n\n```json\n" + ctx["review_schema"] + "\n```\n"

    capsule = generate_fresh_capsule(
        offline=False,
        required_pr_number=pr_number,
        required_head_sha=head_sha,
    )
    return _prepend_capsule(
        prompt,
        capsule,
        workflow_bound_note=f"Review of PR #{pr_number} at {head_sha}",
    )


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
        repair_count = int(os.environ.get("AGENT_REPAIR_COUNT", "0"))
        if len(sys.argv) == 5 and pathlib.Path(sys.argv[4]).is_file():
            prompt = build_ci_repair_prompt_from_evidence(number, sha, sys.argv[4], repair_count)
        else:
            failed_jobs = sys.argv[4] if len(sys.argv) > 4 else "[]"
            run_id = sys.argv[5] if len(sys.argv) > 5 else ""
            logs = fetch_bounded_failed_logs(run_id)
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
