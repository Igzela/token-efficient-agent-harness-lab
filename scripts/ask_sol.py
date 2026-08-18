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
import fcntl
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
DEFAULT_TIMEOUT_SECONDS = 360
MAX_OUTPUT_CHARS = 100_000

ENV_ACTIVE_FLAG = "ASK_SOL_ACTIVE"
ENV_DEPTH_COUNT = "ASK_SOL_DEPTH"
MAX_DEPTH = 1

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent
SCHEMA_PATH = SCRIPT_DIR / "agent-control" / "ask_sol_schema.json"
MODEL_SCHEMA_PATH = SCRIPT_DIR / "agent-control" / "ask_sol_model_schema.json"

# Minimal environment variable allowlist for the Codex CLI subprocess
CODEX_ALLOWED_ENV_VARS = {
    # System essentials
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "TEMP",
    "TMP",
    # Codex-specific configuration
    "CODEX_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_RUNTIME_DIR",
    # Network/Proxy/Certificates (if present and non-credential-bearing)
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
}

PROXY_VARS = {
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
}

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
    """Raised when local Codex CLI or required schema does not meet requirements."""


class AskSolGitContextError(AskSolError):
    """Raised when Git repository context or HEAD cannot be determined."""


class AskSolBudgetError(AskSolError):
    """Raised when consultation limit is reached or budget tracker fails."""


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


def _is_credential_bearing_proxy_url(url_val: str) -> bool:
    """Check if a proxy URL contains embedded username/password userinfo (e.g. http://user:pass@host)."""
    if not url_val:
        return False
    stripped = url_val.split("://", 1)[-1] if "://" in url_val else url_val
    authority = stripped.split("/", 1)[0]
    return "@" in authority


def build_clean_child_env(
    cur_depth: int,
    base_env: dict[str, str] | None = None,
) -> dict[str, str]:
    """Build a strictly minimal, safe environment for the Codex subprocess.

    Never forwards parent environment variables containing API keys, tokens,
    credentials, or arbitrary caller configuration. Rejects credential-bearing proxy URLs.
    """
    source = os.environ if base_env is None else base_env
    child_env: dict[str, str] = {}
    for key in CODEX_ALLOWED_ENV_VARS:
        val = source.get(key)
        if val is not None:
            if key in PROXY_VARS and _is_credential_bearing_proxy_url(val):
                # Omit credential-bearing proxy URLs
                continue
            child_env[key] = val

    # Set ask_sol recursion and depth control variables
    child_env[ENV_ACTIVE_FLAG] = "1"
    child_env[ENV_DEPTH_COUNT] = str(cur_depth + 1)
    return child_env


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


def _hash_file_content(filepath: pathlib.Path, rel_path: str) -> str:
    """Compute SHA-256 digest of a file content in streaming chunks.

    Fails closed if the file cannot be opened or read.
    """
    h = hashlib.sha256()
    try:
        with filepath.open("rb") as f:
            while chunk := f.read(65536):
                h.update(chunk)
        return h.hexdigest()
    except (OSError, IOError) as exc:
        raise AskSolGitContextError(f"Unreadable source file '{rel_path}': {exc}")


def compute_source_state_digest(worktree: pathlib.Path) -> tuple[str, dict[str, Any]]:
    """Compute exact source-state digest binding tracked diffs, status, untracked files, and content.

    Fails closed if git inspection commands or file reads fail.
    """
    try:
        status_res = subprocess.run(
            ["git", "status", "--porcelain=v1", "-uall"],
            capture_output=True,
            text=True,
            check=True,
            cwd=worktree,
        )
        status_out = status_res.stdout
    except (subprocess.SubprocessError, OSError) as exc:
        raise AskSolGitContextError(f"Failed to inspect git status in worktree: {exc}")

    try:
        diff_res = subprocess.run(
            ["git", "diff", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
            cwd=worktree,
        )
        diff_out = diff_res.stdout
    except (subprocess.SubprocessError, OSError) as exc:
        raise AskSolGitContextError(f"Failed to inspect git diff HEAD in worktree: {exc}")

    try:
        cached_diff_res = subprocess.run(
            ["git", "diff", "--cached", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
            cwd=worktree,
        )
        cached_diff_out = cached_diff_res.stdout
    except (subprocess.SubprocessError, OSError) as exc:
        raise AskSolGitContextError(f"Failed to inspect git diff --cached HEAD in worktree: {exc}")

    try:
        ls_others_res = subprocess.run(
            ["git", "ls-files", "--others", "--exclude-standard"],
            capture_output=True,
            text=True,
            check=True,
            cwd=worktree,
        )
        untracked_lines = [line.strip() for line in ls_others_res.stdout.splitlines() if line.strip()]
    except (subprocess.SubprocessError, OSError) as exc:
        raise AskSolGitContextError(f"Failed to list untracked files in worktree: {exc}")

    untracked_entries: list[tuple[str, str, str]] = []
    for rel_path in sorted(untracked_lines):
        full_path = worktree / rel_path
        if full_path.is_symlink():
            try:
                target = os.readlink(full_path)
                untracked_entries.append((rel_path, "symlink", target))
            except OSError as exc:
                raise AskSolGitContextError(f"Unreadable symlink target for '{rel_path}': {exc}")
        elif full_path.is_file():
            content_hash = _hash_file_content(full_path, rel_path)
            untracked_entries.append((rel_path, "file", content_hash))
        elif full_path.exists():
            try:
                stat_res = full_path.stat()
                untracked_entries.append((rel_path, "stat", f"mode:{stat_res.st_mode}"))
            except OSError as exc:
                raise AskSolGitContextError(f"Unreadable file entry '{rel_path}': {exc}")
        else:
            raise AskSolGitContextError(f"Untracked file '{rel_path}' listed by git does not exist.")

    is_clean = not status_out.strip() and not diff_out.strip() and not cached_diff_out.strip() and not untracked_entries
    if is_clean:
        digest = "clean"
    else:
        hasher = hashlib.sha256()
        hasher.update(b"status:\n")
        hasher.update(status_out.encode("utf-8"))
        hasher.update(b"\ndiff_head:\n")
        hasher.update(diff_out.encode("utf-8"))
        hasher.update(b"\ndiff_cached:\n")
        hasher.update(cached_diff_out.encode("utf-8"))
        hasher.update(b"\nuntracked:\n")
        for entry in untracked_entries:
            hasher.update(f"{entry[0]}:{entry[1]}:{entry[2]}\n".encode("utf-8"))
        digest = f"dirty:{hasher.hexdigest()[:16]}"

    details = {
        "status_lines": len([line for line in status_out.splitlines() if line.strip()]),
        "untracked_count": len(untracked_entries),
    }
    return digest, details


def get_git_context(worktree: pathlib.Path) -> dict[str, str]:
    """Capture exact git root, HEAD SHA, and exact source-state digest.

    Fails closed if the target directory is not a valid Git repository with a valid HEAD.
    """
    if not worktree.is_dir():
        raise AskSolGitContextError(f"Worktree path does not exist or is not a directory: {worktree}")

    try:
        root_res = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
            cwd=worktree,
        )
        repo_root = root_res.stdout.strip()
        if not repo_root:
            raise AskSolGitContextError("Git rev-parse --show-toplevel returned empty string.")
    except (subprocess.SubprocessError, OSError) as exc:
        raise AskSolGitContextError(f"Target directory is not a valid Git worktree: {exc}")

    try:
        head_res = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
            cwd=worktree,
        )
        head_sha = head_res.stdout.strip()
        if not re.match(r"^[0-9a-f]{40}$", head_sha):
            raise AskSolGitContextError(f"Invalid Git HEAD commit SHA: '{head_sha}'")
    except (subprocess.SubprocessError, OSError) as exc:
        raise AskSolGitContextError(f"Failed to resolve Git HEAD commit SHA: {exc}")

    dirty_digest, _ = compute_source_state_digest(worktree)

    # Derive stable, non-private repository and worktree identities
    repo_identity = pathlib.Path(repo_root).name
    worktree_digest = f"sha256:{hashlib.sha256(str(worktree.resolve()).encode('utf-8')).hexdigest()[:16]}"

    return {
        "repo_identity": repo_identity,
        "worktree_digest": worktree_digest,
        "head_sha": head_sha,
        "dirty_digest": dirty_digest,
        # Internal-only reference for local execution (not serialized into public envelope)
        "_internal_repo_root": repo_root,
        "_internal_worktree": str(worktree.resolve()),
    }


def verify_unmutated_worktree(
    worktree: pathlib.Path, pre_context: dict[str, str]
) -> tuple[bool, str]:
    """Verify that caller worktree has not been modified during consultation."""
    try:
        post_context = get_git_context(worktree)
    except AskSolGitContextError as exc:
        return False, f"Git state check failed post-consultation: {exc}"

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
    tracker_override: pathlib.Path | None = None,
) -> tuple[bool, int, str]:
    """Enforce and update consultation budget for current exact state atomically."""
    budget_file = _get_budget_file(worktree, tracker_override)
    current_task = task_id or "default"
    state_key = f"{current_task}:{context['head_sha']}:{context['dirty_digest']}"

    try:
        budget_file.parent.mkdir(parents=True, exist_ok=True)
        with open(budget_file, "a+", encoding="utf-8") as f:
            fcntl.flock(f, fcntl.LOCK_EX)
            f.seek(0)
            content = f.read().strip()
            if content:
                try:
                    store = json.loads(content)
                    if not isinstance(store, dict):
                        raise AskSolBudgetError("Budget tracker content is not a JSON object.")
                except json.JSONDecodeError as exc:
                    raise AskSolBudgetError(f"Budget tracker corruption detected: {exc}")
            else:
                store = {}

            record = store.get(state_key, {})
            saved_count = int(record.get("count", 0))

            if saved_count >= max_consultations:
                return (
                    False,
                    saved_count,
                    f"Consultation budget exhausted ({saved_count}/{max_consultations}) for state (HEAD={context['head_sha'][:8]}, {context['dirty_digest']}). Modify code, test, or update hypothesis before requesting another consultation.",
                )

            new_count = saved_count + 1
            store[state_key] = {
                "task_id": current_task,
                "head_sha": context["head_sha"],
                "dirty_digest": context["dirty_digest"],
                "count": new_count,
                "updated_at": time.time(),
            }

            f.seek(0)
            f.truncate()
            f.write(json.dumps(store, indent=2))
            f.flush()
            fcntl.flock(f, fcntl.LOCK_UN)
    except (OSError, IOError) as exc:
        raise AskSolBudgetError(f"Failed to access or lock budget tracker: {exc}")

    return True, new_count, f"Consultation {new_count}/{max_consultations} permitted"


def check_codex_capability(codex_bin: str = "codex") -> tuple[bool, str]:
    """Verify installed Codex CLI binary, required schema files, and capability flags."""
    # Verify canonical schema files exist and are valid JSON
    if not SCHEMA_PATH.is_file():
        return False, f"Required result schema file '{SCHEMA_PATH}' is missing."
    try:
        json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    except Exception as exc:
        return False, f"Result schema '{SCHEMA_PATH}' is invalid JSON: {exc}"

    if not MODEL_SCHEMA_PATH.is_file():
        return False, f"Required model response schema file '{MODEL_SCHEMA_PATH}' is missing."
    try:
        json.loads(MODEL_SCHEMA_PATH.read_text(encoding="utf-8"))
    except Exception as exc:
        return False, f"Model schema '{MODEL_SCHEMA_PATH}' is invalid JSON: {exc}"

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

    return f"""You are GPT-5.6 Sol, an expert independent repository investigator for {context.get('repo_identity', 'this repository')}.
Your role is to independently investigate the caller's uncertainty and provide evidence-grounded findings.

## Investigation Goal
{goal.strip()}

## Caller Hypothesis (UNTRUSTED - verify independently)
{hypo_text}

## Bound Investigation Context
- Repository Identity: {context.get('repo_identity', 'unknown')}
- Worktree Identity Digest: {context.get('worktree_digest', 'unknown')}
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
7. Return your structured findings according to the required schema. Evidence paths must be repository-relative.
"""


def _public_source_context(ctx: dict[str, str]) -> dict[str, str]:
    """Filter internal paths from public context dictionary."""
    return {
        "repo_identity": ctx["repo_identity"],
        "worktree_digest": ctx["worktree_digest"],
        "head_sha": ctx["head_sha"],
        "dirty_digest": ctx["dirty_digest"],
    }


def execute_sol_investigation(
    goal: str,
    hypothesis: str | None = None,
    task_id: str | None = None,
    worktree: pathlib.Path | None = None,
    codex_bin: str = "codex",
    timeout_seconds: int = DEFAULT_TIMEOUT_SECONDS,
    max_consultations: int = MAX_CONSULTATIONS_PER_ATTEMPT,
    budget_tracker_path: pathlib.Path | None = None,
    dry_run: bool = False,
) -> dict[str, Any]:
    """Execute bounded Sol investigation and return structured result envelope."""
    # 1. Check recursion
    check_recursion_guards()

    # 2. Context binding (fails closed on git failure)
    target_worktree = (worktree or pathlib.Path.cwd()).resolve()
    try:
        pre_context = get_git_context(target_worktree)
    except AskSolGitContextError as exc:
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "FAILED",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": {
                "repo_identity": target_worktree.name,
                "worktree_digest": f"sha256:{hashlib.sha256(str(target_worktree).encode('utf-8')).hexdigest()[:16]}",
                "head_sha": "0" * 40,
                "dirty_digest": "unprovable_git_context",
            },
            "finding": f"Investigation rejected: Git context discovery failed. {exc}",
            "evidence": [],
            "rejected_alternatives": [],
            "confidence": "LOW",
            "unresolved": [str(exc)],
            "recommended_next_action": "Run scripts/ask_sol inside a valid, intact Git worktree with a valid HEAD.",
        }
        return envelope

    public_ctx = _public_source_context(pre_context)

    # 3. Capability verification (fails closed before budget consumption)
    cap_ok, cap_msg = check_codex_capability(codex_bin)
    if not cap_ok:
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "FAILED",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": public_ctx,
            "finding": f"Codex CLI capability preflight failed: {cap_msg}",
            "evidence": [],
            "rejected_alternatives": [],
            "confidence": "LOW",
            "unresolved": [cap_msg],
            "recommended_next_action": "Verify Codex CLI installation and capabilities or proceed with local investigation.",
        }
        return envelope

    # 4. Dry-run return (consumes 0 budget slots)
    if dry_run:
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "SUCCESS",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": public_ctx,
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
            "recommended_next_action": "Execute scripts/ask_sol without --dry-run when ready.",
        }
        return envelope

    # 5. Budget enforcement (only reserved for actual attempted Sol invocation)
    try:
        permitted, count, budget_msg = check_and_record_budget(
            target_worktree,
            task_id,
            pre_context,
            max_consultations=max_consultations,
            tracker_override=budget_tracker_path,
        )
    except AskSolBudgetError as exc:
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "FAILED",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": public_ctx,
            "finding": f"Budget tracker failure: {exc}",
            "evidence": [],
            "rejected_alternatives": [],
            "confidence": "LOW",
            "unresolved": [str(exc)],
            "recommended_next_action": "Inspect local system temp directory and permissions.",
        }
        return envelope

    if not permitted:
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "REJECTED",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": public_ctx,
            "finding": f"Consultation rejected: {budget_msg}",
            "evidence": [],
            "rejected_alternatives": [],
            "confidence": "LOW",
            "unresolved": [budget_msg],
            "recommended_next_action": "Perform local investigation and make code/test progress before re-escalating.",
        }
        return envelope

    # 6. Build prompt
    prompt = build_sol_prompt(goal, hypothesis, pre_context, task_id)

    # 7. Execute codex exec in clean minimal environment
    with tempfile.TemporaryDirectory(prefix="ask_sol_") as tmpdir:
        tmp_path = pathlib.Path(tmpdir)
        output_file = tmp_path / "sol_output.json"

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
            str(MODEL_SCHEMA_PATH),
            "-o",
            str(output_file),
            prompt,
        ]

        # Use strict allowlist-only child environment
        child_env = build_clean_child_env(cur_depth=0)

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

        # 8. Post-consultation worktree non-mutation verification
        unmutated, mutation_reason = verify_unmutated_worktree(target_worktree, pre_context)
        if not unmutated:
            envelope = {
                "schema_version": SCHEMA_VERSION,
                "status": "MUTATION_DETECTED",
                "investigation_goal": goal,
                "caller_hypothesis": hypothesis,
                "source_context": public_ctx,
                "finding": f"CRITICAL: Caller worktree mutation detected during consultation! {mutation_reason}",
                "evidence": [],
                "rejected_alternatives": [],
                "confidence": "LOW",
                "unresolved": [mutation_reason],
                "recommended_next_action": "Inspect git status immediately and restore unauthorized changes.",
            }
            return envelope

        # 9. Parse output
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
                json_match = re.search(r"\{.*\}", raw_stdout, re.DOTALL)
                if json_match:
                    try:
                        parsed_model_data = json.loads(json_match.group(0))
                    except json.JSONDecodeError:
                        pass

    # Fail closed: NONZERO RETURNCODE MUST NEVER PRODUCE SUCCESS
    if returncode != 0:
        err_msg = sanitize_text(raw_stderr.strip() or f"Codex CLI exited with non-zero status ({returncode})")
        finding_text = f"Sol investigation failed (exit code {returncode}): {err_msg}"
        if parsed_model_data and isinstance(parsed_model_data, dict):
            partial_finding = parsed_model_data.get("finding")
            if partial_finding:
                finding_text += f"\n\nPartial model finding captured: {partial_finding}"
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "FAILED",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": public_ctx,
            "finding": finding_text,
            "evidence": [],
            "rejected_alternatives": [],
            "confidence": "LOW",
            "unresolved": [err_msg],
            "recommended_next_action": "Review Codex error output or proceed with manual worker investigation.",
        }
        return sanitize_data(envelope)

    if not parsed_model_data or not isinstance(parsed_model_data, dict):
        envelope = {
            "schema_version": SCHEMA_VERSION,
            "status": "INCONCLUSIVE",
            "investigation_goal": goal,
            "caller_hypothesis": hypothesis,
            "source_context": public_ctx,
            "finding": "Sol did not return a valid structured response.",
            "evidence": [],
            "rejected_alternatives": [],
            "confidence": "LOW",
            "unresolved": ["Model output was unparseable as JSON."],
            "recommended_next_action": "Refine investigation goal or inspect repository directly.",
        }
        return envelope

    # 10. Normalize & sanitize model data (only on returncode == 0)
    finding = str(parsed_model_data.get("finding", "")).strip() or "No finding text provided."
    confidence = str(parsed_model_data.get("confidence", "MEDIUM")).upper()
    if confidence not in {"HIGH", "MEDIUM", "LOW"}:
        confidence = "MEDIUM"

    raw_evidence = parsed_model_data.get("evidence", [])
    evidence_list: list[dict[str, Any]] = []
    repo_prefix = pre_context.get("_internal_repo_root", "")
    if isinstance(raw_evidence, list):
        for item in raw_evidence:
            if isinstance(item, dict):
                raw_path = str(item.get("path", "unknown"))
                if repo_prefix and raw_path.startswith(repo_prefix):
                    raw_path = os.path.relpath(raw_path, repo_prefix)
                evidence_list.append({
                    "path": raw_path,
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
        "source_context": public_ctx,
        "finding": finding,
        "evidence": evidence_list,
        "rejected_alternatives": rejected_alts,
        "confidence": confidence,
        "unresolved": unresolved,
        "recommended_next_action": next_action,
    }

    # 11. Sanitize all secrets from envelope
    sanitized_envelope = sanitize_data(envelope)

    # 12. Validate final envelope fail-closed
    try:
        full_schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
        val_errors = validate_schema(sanitized_envelope, full_schema)
        if val_errors:
            sanitized_envelope["status"] = "INCONCLUSIVE"
            sanitized_envelope["unresolved"].extend(val_errors)
    except Exception as exc:
        sanitized_envelope["status"] = "FAILED"
        sanitized_envelope["unresolved"].append(f"Result schema validation error: {exc}")

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
  scripts/ask_sol "Why is test_verify_rwe_snapshot failing on clean worktree?"
  scripts/ask_sol "Is AdvisorBroker dead code or referenced in dispatch?" --hypothesis "I suspect only dispatch_engine uses it"
  scripts/ask_sol "Determine root cause for PostgreSQL connection leak" --task-id "issue-566" --json
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
