#!/usr/bin/env python3
"""Shared, harness-neutral GPT-5.6 Sol investigation escalation tool.

Launches a temporary, independent, read-only GPT-5.6 Sol investigation through
the locally installed and authenticated Codex CLI when an ordinary worker
encounters genuine uncertainty, contradictory evidence, or high-risk architectural
questions.

The worker remains the task owner and executor. Sol inspects the exact current
worktree in a read-only sandbox and returns evidence-grounded findings.
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any

SCHEMA_VERSION = "ask_sol_result.v1"
DEFAULT_MODEL = "gpt-5.6-sol"
DEFAULT_REASONING_EFFORT = "max"
DEFAULT_SANDBOX = "read-only"
MAX_CONSULTATIONS_PER_ATTEMPT = 2
DEFAULT_TIMEOUT_SECONDS = 180
MAX_OUTPUT_CHARS = 100_000

ENV_ACTIVE_FLAG = "ASK_SOL_ACTIVE"
ENV_DEPTH_COUNT = "ASK_SOL_DEPTH"
MAX_DEPTH = 1

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent
SCHEMA_PATH = SCRIPT_DIR / "agent-control" / "ask_sol_schema.json"
MODEL_SCHEMA_PATH = SCRIPT_DIR / "agent-control" / "ask_sol_model_schema.json"

# Patterns for redacting credentials and sensitive tokens from findings
SECRET_PATTERNS = [
    re.compile(r"ghp_[A-Za-z0-9_]{20,}"),
    re.compile(r"github_pat_[A-Za-z0-9_]{20,}"),
    re.compile(r"sk-[A-Za-z0-9_-]{20,}"),
    re.compile(r"(?i)bearer\s+[A-Za-z0-9._~+/-]{15,}=*"),
    re.compile(r"-----BEGIN [A-Z ]+ PRIVATE KEY-----"),
    re.compile(r"(?i)(?:password|secret|api_key|access_token)\s*[:=]\s*['\"][^'\"]{8,}['\"]"),
]


class AskSolError(Exception):
    """Base exception for ask_sol errors."""


class AskSolRecursionError(AskSolError):
    """Raised when a recursive ask_sol call is detected."""


class AskSolCapabilityError(AskSolError):
    """Raised when local Codex CLI does not meet capability requirements."""


class AskSolBudgetExceededError(AskSolError):
    """Raised when consultation limit for a state is reached."""


class AskSolMutationError(AskSolError):
    """Raised when Sol modifies the caller's working tree."""


class AskSolValidationError(AskSolError):
    """Raised when model response or envelope fails schema validation."""


def sanitize_text(text: str) -> str:
    """Redact secret-shaped patterns from text."""
    if not text:
        return text
    sanitized = text
    for pattern in SECRET_PATTERNS:
        sanitized = pattern.sub("[REDACTED_SECRET]", sanitized)
    return sanitized


def sanitize_data(data: Any) -> Any:
    """Recursively sanitize strings in structured data."""
    if isinstance(data, str):
        return sanitize_text(data)
    if isinstance(data, list):
        return [sanitize_data(item) for item in data]
    if isinstance(data, dict):
        return {key: sanitize_data(value) for key, value in data.items()}
    return data


def validate_schema(data: dict[str, Any], schema: dict[str, Any]) -> list[str]:
    """Validate data against a subset of JSON Schema draft 2020-12."""
    errors: list[str] = []
    required = schema.get("required", [])
    for field in required:
        if field not in data:
            errors.append(f"Missing required field: '{field}'")

    properties = schema.get("properties", {})
    for key, val in data.items():
        if key not in properties and schema.get("additionalProperties") is False:
            errors.append(f"Unexpected property: '{key}'")
            continue
        prop_schema = properties.get(key)
        if not prop_schema:
            continue
        expected_type = prop_schema.get("type")
        if expected_type:
            types = expected_type if isinstance(expected_type, list) else [expected_type]
            matched = False
            for t in types:
                if t == "string" and isinstance(val, str):
                    matched = True
                    min_len = prop_schema.get("minLength")
                    max_len = prop_schema.get("maxLength")
                    if min_len is not None and len(val) < min_len:
                        errors.append(f"Field '{key}' length {len(val)} < minLength {min_len}")
                    if max_len is not None and len(val) > max_len:
                        errors.append(f"Field '{key}' length {len(val)} > maxLength {max_len}")
                    pattern = prop_schema.get("pattern")
                    if pattern and not re.search(pattern, val):
                        errors.append(f"Field '{key}' does not match pattern: {pattern}")
                    enum_vals = prop_schema.get("enum")
                    if enum_vals and val not in enum_vals:
                        errors.append(f"Field '{key}' value '{val}' not in enum {enum_vals}")
                elif t == "null" and val is None:
                    matched = True
                elif t == "integer" and isinstance(val, int) and not isinstance(val, bool):
                    matched = True
                elif t == "number" and isinstance(val, (int, float)) and not isinstance(val, bool):
                    matched = True
                elif t == "boolean" and isinstance(val, bool):
                    matched = True
                elif t == "array" and isinstance(val, list):
                    matched = True
                    max_items = prop_schema.get("maxItems")
                    if max_items is not None and len(val) > max_items:
                        errors.append(f"Field '{key}' items count {len(val)} > maxItems {max_items}")
                    items_schema = prop_schema.get("items")
                    if items_schema and isinstance(items_schema, dict):
                        for idx, item in enumerate(val):
                            item_type = items_schema.get("type")
                            if item_type == "string" and not isinstance(item, str):
                                errors.append(f"Field '{key}[{idx}]' must be string")
                            elif item_type == "object" and isinstance(item, dict):
                                item_errors = validate_schema(item, items_schema)
                                for ie in item_errors:
                                    errors.append(f"Field '{key}[{idx}]': {ie}")
                elif t == "object" and isinstance(val, dict):
                    matched = True
                    obj_errors = validate_schema(val, prop_schema)
                    for oe in obj_errors:
                        errors.append(f"Field '{key}': {oe}")
            if not matched:
                errors.append(f"Field '{key}' has invalid type '{type(val).__name__}', expected '{expected_type}'")

    return errors


def get_git_context(worktree: pathlib.Path) -> dict[str, str]:
    """Capture exact git root, HEAD SHA, and dirty-state digest."""
    try:
        root_res = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
            cwd=worktree,
        )
        repo_root = root_res.stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        repo_root = str(worktree.resolve())

    try:
        head_res = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
            cwd=worktree,
        )
        head_sha = head_res.stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        head_sha = "0" * 40

    try:
        status_res = subprocess.run(
            ["git", "status", "--porcelain=v1"],
            capture_output=True,
            text=True,
            check=True,
            cwd=worktree,
        )
        status_out = status_res.stdout
    except (subprocess.CalledProcessError, FileNotFoundError):
        status_out = ""

    try:
        diff_res = subprocess.run(
            ["git", "diff", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
            cwd=worktree,
        )
        diff_out = diff_res.stdout
    except (subprocess.CalledProcessError, FileNotFoundError):
        diff_out = ""

    try:
        cached_diff_res = subprocess.run(
            ["git", "diff", "--cached", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
            cwd=worktree,
        )
        cached_diff_out = cached_diff_res.stdout
    except (subprocess.CalledProcessError, FileNotFoundError):
        cached_diff_out = ""

    if not status_out.strip():
        dirty_digest = "clean"
    else:
        combined = f"{status_out}\n---\n{diff_out}\n---\n{cached_diff_out}".encode("utf-8")
        dirty_digest = f"dirty:{hashlib.sha256(combined).hexdigest()[:16]}"

    return {
        "repo_root": repo_root,
        "worktree": str(worktree.resolve()),
        "head_sha": head_sha,
        "dirty_digest": dirty_digest,
    }


def verify_unmutated_worktree(
    worktree: pathlib.Path, pre_context: dict[str, str]
) -> tuple[bool, str]:
    """Verify that caller worktree has not been modified during consultation."""
    post_context = get_git_context(worktree)
    if post_context["head_sha"] != pre_context["head_sha"]:
        return (
            False,
            f"HEAD commit mutated during consultation: was {pre_context['head_sha']}, now {post_context['head_sha']}",
        )
    if post_context["dirty_digest"] != pre_context["dirty_digest"]:
        return (
            False,
            f"Worktree state mutated during consultation: was {pre_context['dirty_digest']}, now {post_context['dirty_digest']}",
        )
    return True, "Worktree unmutated"


def check_recursion_guards() -> None:
    """Fail closed if ask_sol is invoked recursively."""
    if os.environ.get(ENV_ACTIVE_FLAG) == "1":
        raise AskSolRecursionError(
            "Recursive ask_sol invocation rejected: Sol investigation cannot invoke ask_sol."
        )
    try:
        depth = int(os.environ.get(ENV_DEPTH_COUNT, "0"))
    except ValueError:
        depth = 0
    if depth >= MAX_DEPTH:
        raise AskSolRecursionError(
            f"Maximum consultation depth ({MAX_DEPTH}) reached. Recursive invocation blocked."
        )


def _get_budget_file(worktree: pathlib.Path, tracker_override: pathlib.Path | None = None) -> pathlib.Path:
    """Get location of lightweight local consultation budget tracker."""
    if tracker_override is not None:
        return tracker_override
    path_hash = hashlib.sha256(str(worktree.resolve()).encode("utf-8")).hexdigest()[:16]
    return pathlib.Path(tempfile.gettempdir()) / f"ask_sol_budget_{path_hash}.json"


def check_and_record_budget(
    worktree: pathlib.Path,
    task_id: str | None,
    context: dict[str, str],
    max_consultations: int = MAX_CONSULTATIONS_PER_ATTEMPT,
    force: bool = False,
    tracker_override: pathlib.Path | None = None,
) -> tuple[bool, int, str]:
    """Enforce and update consultation budget for current exact state."""
    if force:
        return True, 0, "Consultation forced by caller override"

    budget_file = _get_budget_file(worktree, tracker_override)
    state: dict[str, Any] = {}
    if budget_file.is_file():
        try:
            state = json.loads(budget_file.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            state = {}

    current_task = task_id or "default"
    saved_task = state.get("task_id")
    saved_head = state.get("head_sha")
    saved_dirty = state.get("dirty_digest")
    saved_count = int(state.get("count", 0))

    # If state matches, check count
    if (
        saved_task == current_task
        and saved_head == context["head_sha"]
        and saved_dirty == context["dirty_digest"]
    ):
        if saved_count >= max_consultations:
            return (
                False,
                saved_count,
                f"Consultation budget exhausted ({saved_count}/{max_consultations}) for state (HEAD={context['head_sha'][:8]}, {context['dirty_digest']}). Modify code, test, or update hypothesis before requesting another consultation.",
            )
        new_count = saved_count + 1
    else:
        # New state reset
        new_count = 1

    # Record updated state
    new_state = {
        "task_id": current_task,
        "head_sha": context["head_sha"],
        "dirty_digest": context["dirty_digest"],
        "count": new_count,
        "updated_at": time.time(),
    }
    try:
        budget_file.write_text(json.dumps(new_state, indent=2), encoding="utf-8")
    except OSError:
        pass  # Non-fatal if temp dir write fails

    return True, new_count, f"Consultation {new_count}/{max_consultations} permitted"


def check_codex_capability(codex_bin: str = "codex") -> tuple[bool, str]:
    """Verify installed Codex CLI binary and required capability flags."""
    bin_path = shutil.which(codex_bin)
    if not bin_path:
        return False, f"Codex CLI binary '{codex_bin}' not found in PATH."

    try:
        ver_res = subprocess.run(
            [codex_bin, "--version"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if ver_res.returncode != 0:
            return False, f"Codex version check failed: {ver_res.stderr.strip()}"
    except (subprocess.SubprocessError, OSError) as exc:
        return False, f"Failed to execute '{codex_bin} --version': {exc}"

    try:
        help_res = subprocess.run(
            [codex_bin, "exec", "--help"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if help_res.returncode != 0:
            return False, f"Codex exec subcommand unavailable: {help_res.stderr.strip()}"
        help_text = help_res.stdout
        required_flags = ["--output-schema", "--ephemeral", "-s", "-m", "-c", "-C", "-o"]
        for flag in required_flags:
            if flag not in help_text:
                return False, f"Required Codex CLI flag '{flag}' not found in 'codex exec --help'."
    except (subprocess.SubprocessError, OSError) as exc:
        return False, f"Failed to execute '{codex_bin} exec --help': {exc}"

    return True, f"Codex CLI ready: {ver_res.stdout.strip()}"


def build_sol_prompt(
    goal: str,
    hypothesis: str | None,
    context: dict[str, str],
    task_id: str | None,
) -> str:
    """Build clear, structured prompt for GPT-5.6 Sol investigation."""
    hypo_text = hypothesis.strip() if hypothesis else "(None provided)"
    task_text = task_id.strip() if task_id else "Unspecified"

    return f"""You are GPT-5.6 Sol, an expert independent repository investigator for token-efficient-agent-harness-lab.
Your role is to independently investigate the caller's uncertainty and provide evidence-grounded findings.

## Investigation Goal
{goal.strip()}

## Caller Hypothesis (UNTRUSTED - verify independently)
{hypo_text}

## Bound Investigation Context
- Repository Root: {context['repo_root']}
- Worktree Root: {context['worktree']}
- HEAD Commit: {context['head_sha']}
- Worktree State: {context['dirty_digest']}
- Caller Task Identity: {task_text}

## Investigation Rules
1. Treat the caller's diagnosis or hypothesis as UNTRUSTED. Do not assume it is correct.
2. Independently inspect the repository: read and search relevant files, inspect git state, diffs, commits, callers, tests, schemas, contracts, and module maps.
3. Prefer first-party repository evidence (code, tests, git history, docs) over summaries or assumptions.
4. DO NOT mutate any repository files, stage changes, commit, push, or execute state-modifying actions. This is a strictly READ-ONLY investigation.
5. Explicitly distinguish:
   - Confirmed evidence (exact files, lines, tests, observations)
   - Inferences (logical deductions from confirmed evidence)
   - Unresolved uncertainties (what remains unproven or unknown)
6. Test and reject plausible alternative explanations where useful.
7. Return your structured findings according to the required schema.
"""


def execute_sol_investigation(
    goal: str,
    hypothesis: str | None = None,
    task_id: str | None = None,
    worktree: pathlib.Path | None = None,
    codex_bin: str = "codex",
    timeout_seconds: int = DEFAULT_TIMEOUT_SECONDS,
    max_consultations: int = MAX_CONSULTATIONS_PER_ATTEMPT,
    force: bool = False,
    budget_tracker_path: pathlib.Path | None = None,
    dry_run: bool = False,
) -> dict[str, Any]:
    """Execute bounded Sol investigation and return structured result envelope."""
    # 1. Check recursion
    check_recursion_guards()

    # 2. Context binding
    target_worktree = (worktree or pathlib.Path.cwd()).resolve()
    pre_context = get_git_context(target_worktree)

    # 3. Budget enforcement
    permitted, count, budget_msg = check_and_record_budget(
        target_worktree,
        task_id,
        pre_context,
        max_consultations=max_consultations,
        force=force,
        tracker_override=budget_tracker_path,
    )
    if not permitted:
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "REJECTED",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": pre_context,
            "finding": f"Consultation rejected: {budget_msg}",
            "evidence": [],
            "rejected_alternatives": [],
            "confidence": "LOW",
            "unresolved": [budget_msg],
            "recommended_next_action": "Perform local investigation and make code/test progress before re-escalating.",
        }
        return envelope

    # 4. Capability verification
    cap_ok, cap_msg = check_codex_capability(codex_bin)
    if not cap_ok:
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "FAILED",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": pre_context,
            "finding": f"Codex CLI capability preflight failed: {cap_msg}",
            "evidence": [],
            "rejected_alternatives": [],
            "confidence": "LOW",
            "unresolved": [cap_msg],
            "recommended_next_action": "Verify Codex CLI installation and capabilities or proceed with local investigation.",
        }
        return envelope

    if dry_run:
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "SUCCESS",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": pre_context,
            "finding": f"[DRY RUN] Preflight passed for Sol investigation. Codex: {cap_msg}",
            "evidence": [
                {
                    "path": "scripts/ask_sol.py",
                    "line_range": None,
                    "observation": "Dry run invocation verified context binding and capability preflight.",
                }
            ],
            "rejected_alternatives": [],
            "confidence": "HIGH",
            "unresolved": [],
            "recommended_next_action": "Execute ask_sol without --dry-run when ready.",
        }
        return envelope

    # 5. Build prompt
    prompt = build_sol_prompt(goal, hypothesis, pre_context, task_id)

    # 6. Execute codex exec
    with tempfile.TemporaryDirectory(prefix="ask_sol_") as tmpdir:
        tmp_path = pathlib.Path(tmpdir)
        output_file = tmp_path / "sol_output.json"

        # Model schema file
        model_schema_file = MODEL_SCHEMA_PATH
        if not model_schema_file.is_file():
            # Fallback inline schema if path not resolved
            model_schema_file = tmp_path / "model_schema.json"
            model_schema_file.write_text(
                json.dumps({
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "required": ["finding", "evidence", "confidence", "unresolved", "recommended_next_action"],
                    "properties": {
                        "finding": {"type": "string"},
                        "evidence": {"type": "array"},
                        "rejected_alternatives": {"type": "array"},
                        "confidence": {"type": "string", "enum": ["HIGH", "MEDIUM", "LOW"]},
                        "unresolved": {"type": "array"},
                        "recommended_next_action": {"type": "string"},
                    },
                }),
                encoding="utf-8",
            )

        cmd = [
            codex_bin,
            "exec",
            "-m",
            DEFAULT_MODEL,
            "-c",
            f'model_reasoning_effort="{DEFAULT_REASONING_EFFORT}"',
            "-s",
            DEFAULT_SANDBOX,
            "--ephemeral",
            "-C",
            str(target_worktree),
            "--output-schema",
            str(model_schema_file),
            "-o",
            str(output_file),
            prompt,
        ]

        child_env = os.environ.copy()
        child_env[ENV_ACTIVE_FLAG] = "1"
        try:
            cur_depth = int(child_env.get(ENV_DEPTH_COUNT, "0"))
        except ValueError:
            cur_depth = 0
        child_env[ENV_DEPTH_COUNT] = str(cur_depth + 1)

        try:
            exec_res = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=timeout_seconds,
                env=child_env,
                cwd=target_worktree,
            )
            raw_stdout = exec_res.stdout
            raw_stderr = exec_res.stderr
            returncode = exec_res.returncode
        except subprocess.TimeoutExpired:
            returncode = -1
            raw_stdout = ""
            raw_stderr = f"Sol investigation timed out after {timeout_seconds} seconds."
        except Exception as exc:
            returncode = -1
            raw_stdout = ""
            raw_stderr = f"Subprocess execution error: {exc}"

        # 7. Post-consultation worktree non-mutation verification
        unmutated, mutation_reason = verify_unmutated_worktree(target_worktree, pre_context)
        if not unmutated:
            envelope = {
                "schema_version": SCHEMA_VERSION,
                "status": "MUTATION_DETECTED",
                "investigation_goal": goal,
                "caller_hypothesis": hypothesis,
                "source_context": pre_context,
                "finding": f"CRITICAL: Caller worktree mutation detected during consultation! {mutation_reason}",
                "evidence": [],
                "rejected_alternatives": [],
                "confidence": "LOW",
                "unresolved": [mutation_reason],
                "recommended_next_action": "Inspect git status immediately and restore unauthorized changes.",
            }
            return envelope

        # 8. Parse and validate output
        parsed_model_data: dict[str, Any] | None = None
        if output_file.is_file():
            try:
                file_content = output_file.read_text(encoding="utf-8").strip()
                if file_content:
                    parsed_model_data = json.loads(file_content)
            except (json.JSONDecodeError, OSError):
                pass

    if not parsed_model_data and raw_stdout.strip():
        try:
            parsed_model_data = json.loads(raw_stdout.strip())
        except json.JSONDecodeError:
            # Look for JSON block in stdout
            json_match = re.search(r"\{.*\}", raw_stdout, re.DOTALL)
            if json_match:
                try:
                    parsed_model_data = json.loads(json_match.group(0))
                except json.JSONDecodeError:
                    pass

    if returncode != 0 and not parsed_model_data:
        err_msg = sanitize_text(raw_stderr.strip() or f"Codex exited with code {returncode}")
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "FAILED",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": pre_context,
            "finding": f"Sol investigation failed: {err_msg}",
            "evidence": [],
            "rejected_alternatives": [],
            "confidence": "LOW",
            "unresolved": [err_msg],
            "recommended_next_action": "Review local error logs or proceed with manual worker investigation.",
        }
        return envelope

    if not parsed_model_data or not isinstance(parsed_model_data, dict):
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "INCONCLUSIVE",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": pre_context,
            "finding": "Sol did not return a valid structured response.",
            "evidence": [],
            "rejected_alternatives": [],
            "confidence": "LOW",
            "unresolved": ["Model output was unparseable as JSON."],
            "recommended_next_action": "Refine investigation goal or inspect repository directly.",
        }
        return envelope

    # 9. Normalize & sanitize model data
    finding = str(parsed_model_data.get("finding", "")).strip() or "No finding text provided."
    confidence = str(parsed_model_data.get("confidence", "MEDIUM")).upper()
    if confidence not in {"HIGH", "MEDIUM", "LOW"}:
        confidence = "MEDIUM"

    raw_evidence = parsed_model_data.get("evidence", [])
    evidence_list: list[dict[str, Any]] = []
    if isinstance(raw_evidence, list):
        for item in raw_evidence:
            if isinstance(item, dict):
                evidence_list.append({
                    "path": str(item.get("path", "unknown")),
                    "line_range": str(item["line_range"]) if item.get("line_range") else None,
                    "observation": str(item.get("observation", "")),
                })
            elif isinstance(item, str):
                evidence_list.append({
                    "path": "repository",
                    "line_range": None,
                    "observation": item,
                })

    rejected_alts = parsed_model_data.get("rejected_alternatives", [])
    if not isinstance(rejected_alts, list):
        rejected_alts = []
    rejected_alts = [str(x) for x in rejected_alts if isinstance(x, (str, int, float))]

    unresolved = parsed_model_data.get("unresolved", [])
    if not isinstance(unresolved, list):
        unresolved = []
    unresolved = [str(x) for x in unresolved if isinstance(x, (str, int, float))]

    next_action = str(parsed_model_data.get("recommended_next_action", "")).strip() or "Review findings and decide action."

    status = "SUCCESS" if finding and evidence_list else "INCONCLUSIVE"

    envelope = {
        "schema_version": SCHEMA_VERSION,
        "status": status,
        "investigation_goal": goal,
        "caller_hypothesis": hypothesis,
        "source_context": pre_context,
        "finding": finding,
        "evidence": evidence_list,
        "rejected_alternatives": rejected_alts,
        "confidence": confidence,
        "unresolved": unresolved,
        "recommended_next_action": next_action,
    }

    # 10. Sanitize all secrets from envelope
    sanitized_envelope = sanitize_data(envelope)

    # 11. Validate final envelope
    if SCHEMA_PATH.is_file():
        try:
            full_schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
            val_errors = validate_schema(sanitized_envelope, full_schema)
            if val_errors:
                sanitized_envelope["status"] = "INCONCLUSIVE"
                sanitized_envelope["unresolved"].extend(val_errors)
        except Exception:
            pass

    return sanitized_envelope


def format_terminal_report(result: dict[str, Any]) -> str:
    """Format structured result as human-readable terminal output."""
    status = result.get("status", "UNKNOWN")
    goal = result.get("investigation_goal", "")
    hypo = result.get("caller_hypothesis")
    ctx = result.get("source_context", {})
    finding = result.get("finding", "")
    confidence = result.get("confidence", "UNKNOWN")
    evidence = result.get("evidence", [])
    rejected = result.get("rejected_alternatives", [])
    unresolved = result.get("unresolved", [])
    action = result.get("recommended_next_action", "")

    lines: list[str] = [
        "============================================================",
        f"  ask_sol Investigation Report — {status}",
        "============================================================",
        f"Goal: {goal}",
    ]
    if hypo:
        lines.append(f"Caller Hypothesis (untrusted): {hypo}")
    lines.extend([
        f"Source Binding: HEAD={ctx.get('head_sha', '')[:8]} | {ctx.get('dirty_digest', '')}",
        f"Confidence: {confidence}",
        "",
        "--- Finding ---",
        finding,
        "",
    ])

    if evidence:
        lines.append("--- Evidence ---")
        for ev in evidence:
            loc = ev.get("path", "")
            if ev.get("line_range"):
                loc += f":{ev['line_range']}"
            lines.append(f"  • [{loc}] {ev.get('observation', '')}")
        lines.append("")

    if rejected:
        lines.append("--- Rejected Alternatives ---")
        for r in rejected:
            lines.append(f"  ✗ {r}")
        lines.append("")

    if unresolved:
        lines.append("--- Unresolved Uncertainties ---")
        for u in unresolved:
            lines.append(f"  ? {u}")
        lines.append("")

    if action:
        lines.append("--- Recommended Next Action ---")
        lines.append(action)
        lines.append("")

    lines.append("============================================================")
    return "\n".join(lines)


def main() -> None:
    """CLI entrypoint for ask_sol."""
    parser = argparse.ArgumentParser(
        description="Shared, harness-neutral GPT-5.6 Sol investigation escalation tool.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  ask_sol "Why is test_verify_rwe_snapshot failing on clean worktree?"
  ask_sol "Is AdvisorBroker dead code or referenced in dispatch?" --hypothesis "I suspect only dispatch_engine uses it"
  ask_sol "Determine root cause for PostgreSQL connection leak" --task-id "issue-566" --json
""",
    )
    parser.add_argument(
        "goal",
        nargs="?",
        help="Investigation goal, question, or uncertainty to investigate.",
    )
    parser.add_argument(
        "-g",
        "--goal",
        dest="flag_goal",
        help="Investigation goal (alternative to positional argument).",
    )
    parser.add_argument(
        "-H",
        "--hypothesis",
        help="Caller's preliminary hypothesis or diagnosis (treated as untrusted by Sol).",
    )
    parser.add_argument(
        "-t",
        "--task-id",
        "--issue",
        dest="task_id",
        help="Current task or issue identity for consultation loop tracking.",
    )
    parser.add_argument(
        "-w",
        "--worktree",
        type=pathlib.Path,
        default=pathlib.Path.cwd(),
        help="Worktree directory to investigate (default: current directory).",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=pathlib.Path,
        help="Path to write structured JSON result envelope.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output machine-readable JSON to stdout instead of human-readable report.",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=DEFAULT_TIMEOUT_SECONDS,
        help=f"Subprocess timeout in seconds (default: {DEFAULT_TIMEOUT_SECONDS}).",
    )
    parser.add_argument(
        "--codex-bin",
        default="codex",
        help="Path or name of Codex CLI binary (default: codex).",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Bypass consultation budget check for the current state.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Preflight capability and context binding without invoking Sol.",
    )

    args = parser.parse_args()
    goal = args.goal or args.flag_goal
    if not goal:
        parser.error("Investigation goal is required (provide as positional argument or with -g/--goal).")

    try:
        result = execute_sol_investigation(
            goal=goal,
            hypothesis=args.hypothesis,
            task_id=args.task_id,
            worktree=args.worktree,
            codex_bin=args.codex_bin,
            timeout_seconds=args.timeout,
            force=args.force,
            dry_run=args.dry_run,
        )
    except AskSolRecursionError as exc:
        sys.stderr.write(f"Error: {exc}\n")
        sys.exit(1)
    except AskSolError as exc:
        sys.stderr.write(f"Error: {exc}\n")
        sys.exit(1)
    except Exception as exc:
        sys.stderr.write(f"Unexpected error: {exc}\n")
        sys.exit(1)

    if args.output:
        try:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(result, indent=2), encoding="utf-8")
        except OSError as exc:
            sys.stderr.write(f"Failed to write output file '{args.output}': {exc}\n")

    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print(format_terminal_report(result))

    status = result.get("status")
    if status in {"FAILED", "MUTATION_DETECTED", "REJECTED"}:
        sys.exit(1)
    sys.exit(0)


if __name__ == "__main__":
    main()
