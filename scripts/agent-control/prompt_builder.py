"""Build Codex prompts from Issue task specifications and current runtime evidence.
"""

import hashlib
import json
import os
import pathlib
import re
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
    require_local_checkout: bool = False,
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

    local_checkout = capsule.get("local_checkout")
    binding = capsule.get("binding")
    workflow_frontier = capsule.get("workflow_frontier")
    active_packet = capsule.get("active_packet")
    checkout_sha = (local_checkout if isinstance(local_checkout, dict) else {}).get("head_sha")
    pr_exact_head = (binding if isinstance(binding, dict) else {}).get("pr_exact_head")
    pr_head_sha = (pr_exact_head if isinstance(pr_exact_head, dict) else {}).get("head_sha")
    workflow_sha = os.environ.get("GITHUB_SHA")
    workflow_bound_sha = os.environ.get("AGENT_CONTEXT_EXPECTED_HEAD_SHA")
    workflow_pr_number = (
        workflow_frontier if isinstance(workflow_frontier, dict) else {}
    ).get("number")
    canonical_packet = (active_packet if isinstance(active_packet, dict) else {}).get("packet")

    if required_head_sha:
        if workflow_bound_sha and workflow_bound_sha != required_head_sha:
            raise ValueError(
                f"Workflow-bound head {workflow_bound_sha} does not match required head {required_head_sha}"
            )
        if pr_head_sha and pr_head_sha != required_head_sha:
            raise ValueError(
                f"Authoritative PR head {pr_head_sha} does not match required head {required_head_sha}"
            )
        if require_local_checkout:
            if checkout_sha != required_head_sha:
                raise ValueError(
                    f"Checked-out SHA {checkout_sha or 'unavailable'} does not match required head {required_head_sha}"
                )
        elif not pr_head_sha and workflow_sha and workflow_sha != required_head_sha:
            raise ValueError(
                f"Workflow SHA {workflow_sha} does not match required head {required_head_sha}"
            )
        elif (
            not workflow_sha
            and not pr_head_sha
            and checkout_sha
            and checkout_sha != required_head_sha
            and os.environ.get("GITHUB_RUN_ID")
        ):
            raise ValueError(
                f"Checkout SHA {checkout_sha} does not match required head {required_head_sha}"
            )
    if required_pr_number is not None:
        frontier_available = (
            (workflow_frontier if isinstance(workflow_frontier, dict) else {}).get("availability")
            == "confirmed"
        )
        if (
            frontier_available
            and workflow_pr_number is not None
            and workflow_pr_number != required_pr_number
        ):
            raise ValueError(
                f"Workflow PR #{workflow_pr_number} does not match required PR #{required_pr_number}"
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

    for path in [
        "AGENTS.md",
        "docs/ARCHITECTURE.md",
        "docs/AUTONOMY.md",
        "docs/ROADMAP.md",
        "docs/RUNBOOK.md",
    ]:
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


def _build_task_context(issue_body, agents_md, architecture, autonomy, roadmap):
    """Select task-relevant context only. Avoid drowning small tasks in governance."""
    if _detect_task_requires_governance(issue_body):
        parts = []
        if agents_md:
            parts.append(
                "### Repository governance excerpt\n\n"
                "Read the full `AGENTS.md` before editing.\n\n```\n"
                + agents_md[:2500]
                + "\n```"
            )
        if architecture:
            parts.append("### Architecture excerpt\n\n```\n" + architecture[:2000] + "\n```")
        if autonomy:
            parts.append("### Autonomy contract excerpt\n\n```\n" + autonomy[:2500] + "\n```")
        if roadmap:
            parts.append("### Roadmap excerpt\n\n```\n" + roadmap[:1500] + "\n```")
        return "\n\n".join(parts) if parts else "No additional context."
    lines = []
    body_lower = (issue_body or "").lower()
    if agents_md and any(word in body_lower for word in ("agent", "orchestrator", "runner", "workflow", "control")):
        lines.append("From AGENTS.md: " + _extract_relevant_lines(agents_md, ["orchestrator", "worker", "runner", "artifact"]))
    if lines:
        return "\n".join(lines)
    return "No additional context required for this task."


def _active_canonical_context(autonomy, roadmap, max_chars=6000):
    """Return bounded current governance context without inventing a task."""
    parts = []
    if autonomy:
        parts.append("## Autonomy contract\n" + autonomy)
    if roadmap:
        parts.append("## Roadmap\n" + roadmap)
    return "\n\n".join(parts)[:max_chars]


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
        ctx.get("docs_ARCHITECTURE_md", ""),
        ctx.get("docs_AUTONOMY_md", ""),
        ctx.get("docs_ROADMAP_md", ""),
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


def build_claim_bound_implementation_prompt(
    issue_number,
    issue_title,
    issue_body,
    allowed_paths,
    accepted_main_sha,
    branch,
    *,
    template="implementation.md",
    repo_root=None,
):
    """Build an implementation prompt from one already-bound Issue snapshot.

    Local execution must not re-read mutable Issue text while constructing a
    prompt.  The caller supplies the snapshot obtained after the trusted claim
    and the binding fields are repeated in the prompt as non-authoritative
    context for the worker.  Repository documents are read locally and are
    bounded through the same task-context selector as the hosted workflow.
    """

    if not isinstance(issue_number, int) or issue_number <= 0:
        raise ValueError("issue_number is invalid")
    if not isinstance(issue_title, str) or not isinstance(issue_body, str):
        raise ValueError("Issue snapshot is invalid")
    if not isinstance(allowed_paths, list) or not allowed_paths:
        raise ValueError("claim-bound allowed_paths are invalid")
    if not isinstance(accepted_main_sha, str) or len(accepted_main_sha) != 40:
        raise ValueError("accepted_main_sha is invalid")
    if not isinstance(branch, str) or not branch:
        raise ValueError("branch is invalid")
    template_text = load_prompt_template(template)
    if not template_text:
        raise ValueError("implementation prompt template is unavailable")

    root = pathlib.Path(repo_root).resolve() if repo_root is not None else None
    read_local = lambda name: _cat(root / name) if root is not None else _cat(name)
    task_context = _build_task_context(
        issue_body,
        read_local("AGENTS.md") or "",
        read_local("docs/ARCHITECTURE.md") or "",
        read_local("docs/AUTONOMY.md") or "",
        read_local("docs/ROADMAP.md") or "",
    )
    prompt = template_text
    prompt = prompt.replace("{{ISSUE_NUMBER}}", str(issue_number))
    prompt = prompt.replace("{{ISSUE_TITLE}}", issue_title)
    prompt = prompt.replace("{{ISSUE_BODY}}", issue_body)
    prompt = prompt.replace("{{TASK_CONTEXT}}", task_context)
    prompt = prompt.replace("{{REPO_NAME}}", f"{REPO_OWNER}/{REPO_NAME}")
    prompt = prompt.replace("{{GIT_BRANCH}}", branch)
    prompt += (
        "\n\n## Claim-bound execution context\n"
        f"- accepted_main_sha: `{accepted_main_sha}`\n"
        f"- canonical branch: `{branch}`\n"
        f"- allowed_paths: `{json.dumps(allowed_paths, separators=(',', ':'))}`\n"
        "Treat the trusted claim and repository checkout as the authority; do not "
        "expand the allowed paths or perform GitHub mutations.\n"
    )
    prompt += "\n" + cws_session_projection_block(
        accepted_main_sha=accepted_main_sha,
        head_sha=accepted_main_sha,
        packet_id="claim-bound",
        mode="fresh",
        documents={
            "docs/ARCHITECTURE.md": read_local("docs/ARCHITECTURE.md") or "",
            "docs/AUTONOMY.md": read_local("docs/AUTONOMY.md") or "",
            "docs/ROADMAP.md": read_local("docs/ROADMAP.md") or "",
        },
    )
    return prompt


def cws_session_projection_block(
    *,
    accepted_main_sha,
    head_sha,
    packet_id,
    mode,
    documents,
):
    """Compact PINNED handles for canonical docs. Not a second authority owner."""

    if not isinstance(accepted_main_sha, str) or len(accepted_main_sha) != 40:
        raise ValueError("accepted_main_sha is invalid")
    if not isinstance(head_sha, str) or len(head_sha) != 40:
        raise ValueError("head_sha is invalid")
    if mode == "fresh" and accepted_main_sha != head_sha:
        raise ValueError("changed_head")
    if not isinstance(packet_id, str) or not packet_id.strip():
        raise ValueError("packet_id is invalid")
    rows = []
    seen = set()
    for path, body in documents.items():
        if path in seen:
            continue
        seen.add(path)
        digest = hashlib.sha256((body or "").encode("utf-8")).hexdigest()
        rows.append(f"- `{path}` sha256 `{digest}` residency PINNED")
    return (
        "## CWS repository session projection\n"
        f"- accepted_main_sha: `{accepted_main_sha}`\n"
        f"- head_sha: `{head_sha}`\n"
        f"- packet_id: `{packet_id}`\n"
        f"- mode: `{mode}`\n"
        "Canonical documents are bound by identity/hash; do not re-expand full copies.\n"
        + "\n".join(rows)
        + "\n"
    )


def build_claim_bound_plan_implementation_prompt(
    packet_id,
    goal,
    allowed_paths,
    source_main_sha,
    branch,
    *,
    prerequisites,
    forbidden_changes,
    verification,
    rollback,
    repo_root=None,
):
    """Build a prompt from an already validated plan candidate capsule."""

    if not isinstance(packet_id, str) or not packet_id:
        raise ValueError("plan packet id is invalid")
    if not isinstance(goal, str) or not goal or len(goal) > 8192:
        raise ValueError("plan goal is invalid")
    if not isinstance(allowed_paths, list) or not allowed_paths:
        raise ValueError("plan allowed_paths are invalid")
    if not isinstance(source_main_sha, str) or len(source_main_sha) != 40:
        raise ValueError("plan source main SHA is invalid")
    if not isinstance(branch, str) or not branch:
        raise ValueError("plan branch is invalid")
    contract_fields = {
        "prerequisites": (prerequisites, True),
        "forbidden_changes": (forbidden_changes, False),
        "verification": (verification, False),
        "rollback": (rollback, False),
    }
    for field, (value, allow_empty) in contract_fields.items():
        if (
            not isinstance(value, list)
            or len(value) > 50
            or (not allow_empty and not value)
            or any(
                not isinstance(item, str)
                or not item.strip()
                or len(item) > 8192
                for item in value
            )
            or len(value) != len(set(value))
        ):
            raise ValueError(f"plan {field} are invalid")
    template_text = load_prompt_template("implementation.md")
    if not template_text:
        raise ValueError("implementation prompt template is unavailable")
    root = pathlib.Path(repo_root).resolve() if repo_root is not None else None
    read_local = lambda name: _cat(root / name) if root is not None else _cat(name)
    task_body = (
        "<!-- agent-orchestrator-plan-execution:v1 -->\n"
        f"Packet: {packet_id}\n\n{goal}"
    )
    task_context = _build_task_context(
        task_body,
        read_local("AGENTS.md") or "",
        read_local("docs/ARCHITECTURE.md") or "",
        read_local("docs/AUTONOMY.md") or "",
        read_local("docs/ROADMAP.md") or "",
    )
    prompt = template_text
    prompt = prompt.replace("{{ISSUE_NUMBER}}", f"plan packet {packet_id}")
    prompt = prompt.replace("{{ISSUE_TITLE}}", f"Plan packet {packet_id}")
    prompt = prompt.replace("{{ISSUE_BODY}}", task_body)
    prompt = prompt.replace("{{TASK_CONTEXT}}", task_context)
    prompt = prompt.replace("{{REPO_NAME}}", f"{REPO_OWNER}/{REPO_NAME}")
    prompt = prompt.replace("{{GIT_BRANCH}}", branch)
    prompt += (
        "\n\n## Plan claim-bound execution context\n"
        f"- source_main_sha: `{source_main_sha}`\n"
        f"- packet_id: `{packet_id}`\n"
        f"- canonical branch: `{branch}`\n"
        f"- allowed_paths: `{json.dumps(allowed_paths, separators=(',', ':'))}`\n"
        f"- prerequisites: `{json.dumps(prerequisites, separators=(',', ':'))}`\n"
        f"- forbidden_changes: `{json.dumps(forbidden_changes, separators=(',', ':'))}`\n"
        f"- verification: `{json.dumps(verification, separators=(',', ':'))}`\n"
        f"- rollback: `{json.dumps(rollback, separators=(',', ':'))}`\n"
        "The canonical plan document and trusted ledger claim are authority; the "
        "transport capsule is not. Re-prove prerequisites before editing, stay within "
        "allowed paths and forbidden changes, run every verification command, and "
        "use the recorded rollback on a stop condition. Do not expand scope or "
        "perform GitHub mutations.\n"
    )
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

    capsule = generate_fresh_capsule(
        # CI-repair renders PR-head code on a self-hosted worker. Keep GitHub
        # credentials out of that renderer and independently bind the prompt
        # to the actual checked-out exact head.
        offline=True,
        required_pr_number=pr_number,
        required_head_sha=head_sha,
        require_local_checkout=True,
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


def build_review_prompt(pr_number, head_sha, issue_number=None, template="review.md"):
    """Build the exact-head independent-review prompt.

    When ``issue_number`` is known, the durable review state (Review
    Convergence Protocol) is read to derive review_mode / review_round / prior
    ledger, so R1 is `full` and R2 is `repair_verification` with delta-first
    focus.  A retry that exceeds the substantive-round budget or follows
    DECISION_REQUIRED fails closed instead of silently dispatching.
    """
    repository = os.environ.get("AGENT_REPO") or f"{REPO_OWNER}/{REPO_NAME}"
    pr_json = _gh("pr", "view", str(pr_number), "--json", "number,headRefOid,baseRefOid")
    if not pr_json:
        raise ValueError("review binding rejected: live_metadata_unavailable")
    try:
        pr_meta = json.loads(pr_json)
    except Exception:
        raise ValueError("review binding rejected: live_metadata_unavailable")
    if pr_meta.get("headRefOid") != head_sha:
        raise ValueError("review binding rejected: head_mismatch")
    live_base = pr_meta.get("baseRefOid") or ""
    live_binding = {
        "pr_number": int(pr_number),
        "base_sha": live_base,
        "head_sha": head_sha,
        "reviewed_range": f"{live_base}...{head_sha}",
    }

    ctx = build_context(0)
    ctx["pr_number"] = pr_number
    ctx["head_sha"] = live_binding["head_sha"]
    ctx["base_sha"] = live_binding["base_sha"]

    diff = _gh("pr", "diff", str(pr_number))
    if diff is None:
        raise ValueError("complete PR diff is unavailable")
    if len(diff) > MAX_REVIEW_DIFF_CHARS:
        raise ValueError(
            f"complete PR diff exceeds the {MAX_REVIEW_DIFF_CHARS}-character review bound"
        )
    ctx["diff"] = diff

    pr_detail_json = _gh("pr", "view", str(pr_number), "--json", "title,body,files,reviews,comments")
    if pr_detail_json:
        try:
            parsed = json.loads(pr_detail_json)
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

    review_mode = "full"
    review_round = 1
    prior_blockers_text = ""
    mode_context = (
        "Controller-resolved live review range: `"
        f"{live_binding['reviewed_range']}`. This is an **R1 first full review**: "
        "`review_mode=full`, complete `base...head` attestation."
    )
    if issue_number:
        import review_convergence as rc
        attempt = rc.derive_next_review_attempt(None, head_sha)
        if not attempt.get("allowed"):
            raise ValueError(
                f"review dispatch refused: {attempt.get('deny_reason') or 'not_allowed'}"
            )
        review_mode = str(attempt["review_mode"])
        review_round = int(attempt["review_round"])
        prior_ledger_digest = str(attempt.get("finding_ledger_digest") or "")
        prior_head = str(attempt.get("prior_reviewed_head") or "")
        if review_mode == "repair_verification":
            mode_context = (
                "Controller-resolved live review range: `"
                f"{live_binding['reviewed_range']}`. This is the **R2 repair verification** "
                "review: `review_mode=repair_verification`. "
                f"Round {review_round}, prior reviewed head {prior_head[:12]}..."
            )
            if prior_ledger_digest:
                mode_context += (
                    f", finding ledger digest `{prior_ledger_digest}`. "
                )
            mode_context += (
                "Attest the COMPLETE new `base...head` range first, then verify "
                "prior blockers are resolved (or still open with evidence), then "
                "check repair regressions, then a hard-stop scan of the full new "
                "head. New non-blocking nits must be deferred; a new "
                "block_current_head requires admission_reason in "
                "{repair_regression, prior_evidence_unavailable, hard_stop_miss}."
            )
            prior_ids = attempt.get("open_blocker_ids") or []
            if prior_ids:
                prior_blockers_text = (
                    "### Prior open blockers to verify at R2\n\n"
                    + "".join(f"- `{fid}`\n" for fid in prior_ids)
                )
        else:
            mode_context += (
                f" Round {review_round}. There is no prior review ledger to continue."
            )

    prompt = template_text
    prompt = prompt.replace("{{PR_NUMBER}}", str(pr_number))
    prompt = prompt.replace("{{HEAD_SHA}}", head_sha)
    prompt = prompt.replace("{{REVIEW_MODE}}", review_mode)
    prompt = prompt.replace("{{REVIEW_ROUND}}", str(review_round))
    prompt = prompt.replace("{{REVIEW_MODE_CONTEXT}}", mode_context)
    prompt = prompt.replace("{{PRIOR_BLOCKERS_DETAIL}}", prior_blockers_text)
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
        issue_number = sys.argv[4] if len(sys.argv) > 4 else None
        prompt = build_review_prompt(number, sha, issue_number=issue_number)
        if prompt:
            print(prompt)
        else:
            sys.exit(1)

    else:
        print(f"Unknown prompt type: {command}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
