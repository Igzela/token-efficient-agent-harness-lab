#!/usr/bin/env python3
"""Security baseline checker for CA-7 sealed baseline.

Read-only, pure stdlib. Performs eight checks against the repository:
  1. Secret scan — regex scan for credential patterns in git-tracked files
  2. Import scan — AST-based scan for prohibited network/SDK imports
  3. Active routing guard — scan JSON for active_routing_allowed: true
  4. Governance boundary guard — verify governance fixtures exist
  5. Stage-0 event guard — verify events.jsonl exists and is intact
  6. Dormant automation guard — reject unattended-automation patterns that
     can bypass exact-head, review, packet, and permission discipline
  7. Removed plugin-surface guard — fail closed if the old in-memory plugin
     trust semantics are reintroduced into the production crate source
  8. Dormant surface heuristics — fail closed on new dormant production
     surfaces (module-level dead-code blankets, module islands,
     self-described placeholder modules, empty-executor functions, and
     duplicate canonical ownership claims) unless classified in
     DORMANT_SURFACE_CLASSIFICATION_ALLOWLIST

Exit code 0 = all checks pass, 1 = at least one check fails.
"""

from __future__ import annotations

import ast
import json
import os
import re
import shlex
import subprocess
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parent.parent

# Patterns that indicate a credential assignment (flagged unless in an
# allowlist of placeholder values).
SECRET_PATTERNS = [
    re.compile(r"""(?<![A-Za-z0-9_])api_key\s*=\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""(?<![A-Za-z0-9_])secret\s*=\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""(?<![A-Za-z0-9_])token\s*=\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""(?<![A-Za-z0-9_])password\s*=\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""(?<![A-Za-z0-9_])api[_-]?key\s*[:=]\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""(?<![A-Za-z0-9_])secret[_-]?key\s*[:=]\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""(?<![A-Za-z0-9_])access[_-]?token\s*[:=]\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""bearer\s+(?!token\b)[A-Za-z0-9._-]{10,}""", re.IGNORECASE),
]

# Placeholder values that should NOT trigger the secret scan.
PLACEHOLDER_VALUES = {
    "your-api-key-here",
    "your-api-key",
    "your-secret-here",
    "your-secret",
    "your-token-here",
    "your-token",
    "your-password-here",
    "your-password",
    "sk-placeholder",
    "placeholder",
    "changeme",
    "xxx",
    "todo",
    "fixme",
    "example",
    "test",
    "dummy",
    "mock",
    "<api_key>",
    "<secret>",
    "<token>",
    "<password>",
    "REPLACE_ME",
    "TODO",
    "FIXME",
}

# Python modules that indicate outbound network access or provider SDK usage.
PROHIBITED_IMPORTS = {
    "requests",
    "httpx",
    "aiohttp",
    "urllib",
    "urllib.request",
    "urllib.parse",
    "urllib.error",
    "socket",
    "boto3",
    "openai",
    "anthropic",
    "google.generativeai",
    "google.cloud",
    "azure",
    "azure.identity",
    "azure.storage",
}

# Paths to exclude from secret scanning (test files contain test strings
# that intentionally look like secrets).
SECRET_SCAN_EXCLUDE = {
    "tools/test_security_baseline.py",
}

# Per-file import allowlists for bounded operator HTTP clients and tests. These
# exceptions are stdlib transports for local harness/API integration or an
# explicitly gated live-acceptance entry point, not provider SDKs.
# Each key is a file path; the value is the set of imports allowed in that file.
ALLOWED_TEST_IMPORTS: dict[str, set[str]] = {
    "sdk/python/src/agent_control_plane_sdk/client.py": {"urllib.request", "urllib.error"},
    "sdk/python/tests/test_client.py": {"urllib.error"},
    "scripts/acp_local_doctor.py": {"socket"},
    "scripts/acp_ops_check.py": {"urllib.error", "urllib.request"},
    "scripts/acp_restore_smoke.py": {"urllib.error", "urllib.request"},
    "scripts/smoke_native_runtime.py": {"socket", "urllib.error", "urllib.request"},
    "scripts/demo_no_provider.py": {"socket", "urllib.error", "urllib.request"},
    "scripts/soak_ops_drill.py": {
        "urllib.request",
        "urllib.error",
    },
    "scripts/ga_rollback_drill.py": {
        "urllib.request",
        "urllib.error",
    },
    "scripts/ga_release_checklist.py": {
        "urllib.request",
        "urllib.error",
    },
    "scripts/live_e2e_validation.py": {
        "socket",
        "urllib.error",
        "urllib.request",
    },
    "scripts/real_output_pilots.py": {
        "socket",
        "urllib.error",
        "urllib.request",
    },
    "scripts/efficiency_live_benchmark.py": {
        "urllib.error",
        "urllib.request",
    },
}

# Paths to exclude from active routing guard (test fixtures contain
# intentional active_routing_allowed values for testing).
ACTIVE_ROUTING_EXCLUDE_PREFIXES = (
    "tests/fixtures/",
)

# ---------------------------------------------------------------------------
# Dormant automation guard configuration
# ---------------------------------------------------------------------------
# Repository-controlled automation (workflows and automation scripts) may not
# contain patterns that bypass exact-head, review, packet, or permission
# discipline. Every finding fails closed unless the exact file and pattern
# label pair is listed in AUTOMATION_GUARD_ALLOWLIST with a reviewable reason.
#
# The guard is semantic, not a raw-token blacklist: it rejects the specific
# unbound judgments that bypass exact-head evidence (latest-run polling with
# ``--limit 1`` as success evidence, ``gh run watch`` chained to an unbound
# ``gh run list``, and ``gh run watch`` with no run id at all). Explicit run
# ids, commit/head-bound queries, and informational bounded queries
# (``--limit`` >= 2) are legitimate and are NOT flagged.
DORMANT_AUTOMATION_PATTERNS = [
    (
        "dangerously-skip-permissions",
        re.compile(r"dangerously-skip-permissions"),
    ),
    (
        "gh run list --limit 1",
        re.compile(
            r"(?m)\bgh\s+run\s+list\b[^\n;&|]*"
            r"(?:--limit\s*=?\s*1|-L\s*1)\b"
        ),
    ),
    (
        "gh run watch chained to unbound gh run list",
        re.compile(r"\bgh\s+run\s+watch\b[^\n]{0,160}\$\(\s*gh\s+run\s+list\b"),
    ),
    (
        "gh run watch unbound",
        re.compile(r"(?m)\bgh\s+run\s+watch\b[^\n;&|]*"),
    ),
]

# Only repository-controlled automation surfaces are scanned. Plain-text docs
# may legitimately mention these strings in prose and are not scanned.
AUTOMATION_GUARD_SCAN_PREFIXES = (
    ".github/",
    "scripts/",
    "tools/",
)

# Generated, vendored, or build directories are never scanned.
AUTOMATION_GUARD_EXCLUDE_DIRS = {
    "vendor",
    "target",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    "build",
    "generated",
}

# Minimal reviewable allowlist: tracked path -> {pattern label: reason}.
# Kept empty by default; entries require an explicit ownership and expiry
# rationale so an exception cannot silently persist.
AUTOMATION_GUARD_ALLOWLIST: dict[str, dict[str, str]] = {
    "tools/check_security_baseline.py": {
        "dangerously-skip-permissions": "guard detector definition itself; "
        "scanning the guard's own pattern table would be self-referential",
        "gh run list --limit 1": "guard detector definition itself; "
        "scanning the guard's own pattern table would be self-referential",
        "gh run watch chained to unbound gh run list": "guard detector "
        "definition itself; scanning the guard's own pattern table would be "
        "self-referential",
        "gh run watch unbound": "guard detector definition itself; "
        "scanning the guard's own pattern table would be self-referential",
    },
    "tools/test_security_baseline.py": {
        "dangerously-skip-permissions": "positive/negative fixture tests "
        "for the guard detector; not an automation surface",
        "gh run list --limit 1": "positive/negative fixture tests "
        "for the guard detector; not an automation surface",
        "gh run watch chained to unbound gh run list": "positive/negative "
        "fixture tests for the guard detector; not an automation surface",
        "gh run watch unbound": "positive/negative fixture tests "
        "for the guard detector; not an automation surface",
    },
}

# ---------------------------------------------------------------------------
# Removed plugin-surface guard configuration
# ---------------------------------------------------------------------------
# The in-memory plugin system (infrastructure/plugin_system.rs and
# plugin_registry.rs) was deleted as a dormant surface: it never loaded or
# verified plugin binaries and its `official = unrestricted` trust semantics
# could be mistaken for a production security boundary. The guard is a
# composite legacy fingerprint rather than a single-token blacklist, so
# generic business identifiers (e.g. ``verified``, ``official``) remain legal:
#   - re-creating the deleted files themselves is always a finding;
#   - the ``empty = unrestricted`` semantic is always a finding;
#   - two or more legacy tokens in the same file signal a revival attempt.
PLUGIN_LEGACY_FINGERPRINT_TOKENS = (
    "TRUST_LEVEL_OFFICIAL",
    "TRUST_LEVEL_VERIFIED",
    "TRUST_LEVEL_COMMUNITY",
    "PLUGIN_SYSTEM_SCHEMA_VERSION",
    "ALL_KNOWN_PERMISSIONS",
)

PLUGIN_UNRESTRICTED_SEMANTIC = "empty = unrestricted"

PLUGIN_RESURRECTED_PATHS = (
    "engine/src/infrastructure/plugin_system.rs",
    "engine/src/infrastructure/plugin_registry.rs",
)

PLUGIN_SURFACE_SCAN_PREFIXES = (
    "engine/src/",
)

# ---------------------------------------------------------------------------
# Dormant surface heuristics configuration
# ---------------------------------------------------------------------------
# Fail-closed heuristic gate over the production crate source. A new dormant
# production surface is a governance regression: it accumulates code with no
# runtime path, invites a mistaken sense of capability, and contradicts the
# established authority/owner model. The gate flags:
#   (a) module-level `#![allow(dead_code)]` blankets in engine/src;
#   (b) top-level modules declared in lib.rs with no consumer outside their
#       own directory, lib.rs, engine/tests, or engine/src/bin;
#   (c) files whose own header describes them as a stub, placeholder,
#       reference-only, or not-wired module;
#   (d) executor-named functions whose entire body returns an empty value
#       (json!({}), Value::Null, empty collections, Default::default());
#   (e) conflicting "sole/single/canonical owner" claims across different
#       engine/src files.
# Findings are suppressed only by an explicit classification entry in
# DORMANT_SURFACE_CLASSIFICATION_ALLOWLIST carrying owner, reason, review
# condition, and expiry/recheck condition. `#[cfg(test)]` regions and
# engine/tests are not production surface.
DORMANT_SURFACE_SCAN_PREFIX = "engine/src/"

DORMANT_SURFACE_SELF_DESCRIPTORS = (
    re.compile(
        r"(?i)\b(?:this|the)\s+(?:module|file|component|surface)\s+"
        r"(?:is|acts as|serves as|declared as)\s+(?:a\s+)?"
        r"(?:stub|placeholder|reference-only|not wired|dormant)\b"
    ),
    re.compile(
        r"(?i)\b(?:marked|declared|treated|considered|flagged)\s+as\s+"
        r"(?:a\s+)?(?:stub|placeholder|reference-only|not wired|dormant)\b"
    ),
)

DORMANT_EXECUTOR_FN_RE = re.compile(
    r"\bfn\s+(?:execute|invoke|run|dispatch|handle|process)_\w+"
)
DORMANT_EMPTY_BODY_RE = re.compile(
    r"^\s*(?:return\s+)?(?:Ok\(\s*)?"
    r"(?:(?:[A-Za-z_][\w]*::)*json!\(\s*\{\s*\}|(?:[A-Za-z_][\w]*::)*Value::Null|\{\}\.into\(\)|HashMap::new\(\)|"
    r"BTreeMap::new\(\)|Vec::new\(\)|vec!\[\]|Default::default\(\))"
    r"(?:\s*\))?\s*;?\s*$"
)


def _cfg_expression_requires_test(expression: str) -> bool:
    """Return whether a cfg meta-expression can only match test builds."""
    tokens = re.findall(
        r"[A-Za-z_][A-Za-z0-9_]*|[(),=]|\"(?:\\.|[^\"\\])*\"",
        expression,
    )
    position = 0

    def parse() -> bool:
        nonlocal position
        if position >= len(tokens):
            return False
        token = tokens[position]
        position += 1
        if token == "(":
            value = parse()
            while position < len(tokens) and tokens[position] != ")":
                position += 1
            if position < len(tokens):
                position += 1
            return value
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", token):
            return False
        if position < len(tokens) and tokens[position] == "(":
            position += 1
            arguments = []
            while position < len(tokens) and tokens[position] != ")":
                arguments.append(parse())
                if position < len(tokens) and tokens[position] == ",":
                    position += 1
            if position < len(tokens):
                position += 1
            if token == "all":
                return any(arguments)
            if token == "any":
                return bool(arguments) and all(arguments)
            return False
        if position < len(tokens) and tokens[position] == "=":
            position += 1
            if position < len(tokens):
                position += 1
            return False
        return token == "test"

    return parse()

DORMANT_OWNERSHIP_CLAIM_RE = re.compile(
    r"(?i)\b(?:the\s+)?(?:sole|only|single|canonical|authoritative)\s+"
    r"(?:owner|authority|runtime|store|scheduler|evaluator)\s+"
    r"(?:of|for|over)\s+(?:the\s+)?(\w+(?:\s+\w+)?)"
)

# Baseline audit (2026-08-01, PR C): every engine/src top-level module has at
# least one consumer (wire_types is codegen output consumed by the SDK;
# efficiency_benchmark_runtime is consumed by engine/src/bin targets;
# http_server is consumed by engine/src/main.rs and integration tests).
# No module-level `#![allow(dead_code)]` remains; the two former blankets
# (schema.rs, workflow_runs/queue_lease.rs) were removed and their genuinely
# dead items deleted. Classification entries below document the remaining
# boundary cases; an entry expires when its recheck condition stops holding.
DORMANT_SURFACE_CLASSIFICATION_ALLOWLIST = [
    {
        "path": "engine/src/wire_types.rs",
        "classification": "generated",
        "owner": "codegen/generate_wire_types.py",
        "reason": "generated output consumed as "
        "sdk/python/src/agent_control_plane_sdk/wire_types.py; "
        "drift-guarded by scripts/check_wire_codegen_drift.sh",
        "review_condition": "file header still carries the DO NOT EDIT "
        "codegen marker",
        "expiry_or_recheck_condition": "recheck on any codegen or wire "
        "schema change",
    },
    {
        "path": "engine/src/efficiency_benchmark_runtime.rs",
        "classification": "bin_consumer",
        "owner": "engine/src/bin/efficiency_langgraph_runtime.rs, "
        "engine/src/bin/efficiency_native_runtime.rs",
        "reason": "benchmark runtime consumed exclusively by the "
        "efficiency binary targets; not reachable from the library",
        "review_condition": "binary targets still reference it via "
        "engine::efficiency_benchmark_runtime",
        "expiry_or_recheck_condition": "recheck when the efficiency "
        "benchmark binaries change or are removed",
    },
    {
        "path": "engine/src/quality/quality_gate.rs",
        "classification": "wired",
        "owner": "engine/src/read_only_planner.rs, "
        "engine/src/dispatch_engine.rs",
        "reason": "header comment documents local type stubs FOR removed "
        "placeholder modules; the file itself is a wired production gate "
        "consumed by read_only_planner and dispatch_engine",
        "review_condition": "at least one production consumer remains",
        "expiry_or_recheck_condition": "recheck if the header comment is "
        "rewritten or consumers change",
    },
]

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def git_ls_files(repo_root: Path) -> list[str]:
    """Return list of git-tracked file paths relative to repo root."""
    result = subprocess.run(
        ["git", "ls-files"],
        cwd=repo_root,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return []
    return [f for f in result.stdout.strip().splitlines() if f]


def is_text_file(path: Path) -> bool:
    """Heuristic: skip binary files by extension."""
    binary_ext = {
        ".pyc", ".pyo", ".so", ".o", ".a", ".dylib",
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico",
        ".pdf", ".zip", ".tar", ".gz", ".bz2", ".xz",
        ".whl", ".egg",
    }
    return path.suffix.lower() not in binary_ext


def extract_string_literals(filepath: Path) -> list[str]:
    """Extract all string literals from a Python file using AST."""
    try:
        source = filepath.read_text(encoding="utf-8", errors="replace")
        tree = ast.parse(source, filename=str(filepath))
    except (SyntaxError, UnicodeDecodeError):
        return []

    strings = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Constant) and isinstance(node.value, str):
            strings.append(node.value)
        elif isinstance(node, ast.JoinedStr):
            for value in node.values:
                if isinstance(value, ast.Constant) and isinstance(value.value, str):
                    strings.append(value.value)
    return strings


def extract_import_names(filepath: Path) -> list[str]:
    """Extract all imported module names from a Python file using AST."""
    try:
        source = filepath.read_text(encoding="utf-8", errors="replace")
        tree = ast.parse(source, filename=str(filepath))
    except (SyntaxError, UnicodeDecodeError):
        return []

    imports = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                imports.append(alias.name)
        elif isinstance(node, ast.ImportFrom):
            if node.module:
                imports.append(node.module)
    return imports


def find_json_files(repo_root: Path, tracked_files: list[str]) -> list[Path]:
    """Return paths to all tracked JSON files."""
    return [
        repo_root / f
        for f in tracked_files
        if f.endswith(".json") and (repo_root / f).is_file()
    ]


# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------

def check_secret_scan(repo_root: Path, tracked_files: list[str]) -> list[str]:
    """Scan git-tracked files for credential patterns."""
    findings = []
    text_files = [
        f for f in tracked_files
        if is_text_file(repo_root / f) and f not in SECRET_SCAN_EXCLUDE
    ]

    for rel_path in text_files:
        filepath = repo_root / rel_path
        try:
            content = filepath.read_text(encoding="utf-8", errors="replace")
        except (OSError, PermissionError):
            continue

        for line_num, line in enumerate(content.splitlines(), 1):
            stripped = line.strip()
            # Skip comments
            if stripped.startswith("#") or stripped.startswith("//"):
                continue
            line_matched = False  # deduplicate: one finding per line max
            for pattern in SECRET_PATTERNS:
                if line_matched:
                    break
                match = pattern.search(line)
                if match:
                    matched_text = match.group(0)
                    # Check if the value is a known placeholder
                    lower = matched_text.lower()
                    is_placeholder = any(
                        ph.lower() in lower for ph in PLACEHOLDER_VALUES
                    )
                    if not is_placeholder:
                        findings.append(
                            f"{rel_path}:{line_num}: {matched_text}"
                        )
                        line_matched = True
    return findings


def check_import_scan(repo_root: Path, tracked_files: list[str]) -> list[str]:
    """AST-based scan for prohibited network/SDK imports.

    Files in ALLOWED_TEST_IMPORTS are checked against their specific allowlist
    instead of the global prohibited set. This allows narrowly scoped stdlib
    HTTP modules for local integration and SDK clients while still flagging any
    newly added dangerous imports.
    """
    findings = []
    py_files = [f for f in tracked_files if f.endswith(".py") and (repo_root / f).is_file()]

    for rel_path in py_files:
        filepath = repo_root / rel_path
        imports = extract_import_names(filepath)
        allowed = ALLOWED_TEST_IMPORTS.get(rel_path, set())
        for mod_name in imports:
            if mod_name in allowed or any(mod_name.startswith(a + ".") for a in allowed):
                continue
            for prohibited in PROHIBITED_IMPORTS:
                if mod_name == prohibited or mod_name.startswith(prohibited + "."):
                    findings.append(f"{rel_path}: import {mod_name}")
                    break
    return findings


def check_active_routing(repo_root: Path, tracked_files: list[str]) -> list[str]:
    """Scan JSON files for active_routing_allowed: true.

    Excludes test fixtures (tests/fixtures/) which contain intentional
    active_routing_allowed values for testing purposes.
    """
    findings = []
    json_files = find_json_files(repo_root, tracked_files)

    for filepath in json_files:
        rel = filepath.relative_to(repo_root)
        # Skip test fixtures — they contain intentional test data
        if any(str(rel).startswith(prefix) for prefix in ACTIVE_ROUTING_EXCLUDE_PREFIXES):
            continue

        try:
            data = json.loads(filepath.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            continue

        if _has_active_routing(data):
            findings.append(f"{str(rel)}: active_routing_allowed: true found")
    return findings


def _has_active_routing(obj: object) -> bool:
    """Recursively check if active_routing_allowed: true exists in data."""
    if isinstance(obj, dict):
        for key, value in obj.items():
            if key == "active_routing_allowed" and value is True:
                return True
            if _has_active_routing(value):
                return True
    elif isinstance(obj, list):
        for item in obj:
            if _has_active_routing(item):
                return True
    return False


def check_governance_boundary(repo_root: Path) -> list[str]:
    """Verify governance fixtures exist and are well-formed."""
    findings = []
    governance_dir = repo_root / "tests" / "fixtures" / "governance"

    if not governance_dir.is_dir():
        findings.append("Governance fixtures directory not found")
        return findings

    required_fixtures = [
        "valid_all_gates_pass.json",
        "gate_scope_fail.json",
        "gate_approval_fail.json",
        "gate_rollback_fail.json",
        "gate_unknown_error_fail.json",
    ]

    for fixture_name in required_fixtures:
        fixture_path = governance_dir / fixture_name
        if not fixture_path.is_file():
            findings.append(f"Missing governance fixture: {fixture_name}")
            continue

        try:
            data = json.loads(fixture_path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError) as exc:
            findings.append(f"Invalid JSON in {fixture_name}: {exc}")
            continue

        # Validate required fields
        if "schema_version" not in data:
            findings.append(f"{fixture_name}: missing schema_version")
        if "gate_results" not in data:
            findings.append(f"{fixture_name}: missing gate_results")
        else:
            required_gates = [
                "evidence_gate", "approval_gate", "rollback_gate",
                "scope_gate", "unknown_error_gate",
            ]
            for gate in required_gates:
                if gate not in data["gate_results"]:
                    findings.append(f"{fixture_name}: missing gate {gate}")

    return findings


def check_stage0_event_guard(repo_root: Path) -> list[str]:
    """Verify events.jsonl exists and contains stage-0 events."""
    findings = []
    events_path = repo_root / "docs" / "stage0" / "events.jsonl"

    if not events_path.is_file():
        findings.append("docs/stage0/events.jsonl not found")
        return findings

    try:
        content = events_path.read_text(encoding="utf-8")
    except OSError as exc:
        findings.append(f"Cannot read events.jsonl: {exc}")
        return findings

    lines = [line.strip() for line in content.strip().splitlines() if line.strip()]
    if not lines:
        findings.append("events.jsonl is empty")
        return findings

    # Check that at least some events parse as JSON
    parsed = 0
    for line in lines:
        try:
            json.loads(line)
            parsed += 1
        except json.JSONDecodeError:
            continue

    if parsed == 0:
        findings.append("events.jsonl contains no valid JSON lines")

    return findings


def check_dormant_automation_guard(
    repo_root: Path, tracked_files: list[str]
) -> list[str]:
    """Scan repo-controlled workflows and automation scripts for unattended
    patterns that can bypass exact-head, review, packet, or permission
    discipline.

    Scanned surfaces are repository-controlled automation only:
    ``.github/`` workflows and ``scripts/`` / ``tools/`` executables.
    Generated, vendored, and build directories are never scanned. A finding
    is suppressed only when the exact file and pattern label appear in
    ``AUTOMATION_GUARD_ALLOWLIST`` with a reviewable reason; otherwise the
    check fails closed.
    """
    findings = []

    for rel_path in tracked_files:
        if not rel_path.startswith(AUTOMATION_GUARD_SCAN_PREFIXES):
            continue
        if any(
            part in AUTOMATION_GUARD_EXCLUDE_DIRS for part in Path(rel_path).parts
        ):
            continue

        filepath = repo_root / rel_path
        if not filepath.is_file():
            continue
        try:
            content = filepath.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue

        file_allowlist = AUTOMATION_GUARD_ALLOWLIST.get(rel_path, {})
        for label, pattern in DORMANT_AUTOMATION_PATTERNS:
            if label == "gh run list --limit 1":
                matched = _has_unbound_latest_run_list(content)
            elif label == "gh run watch chained to unbound gh run list":
                matched = _has_unbound_chained_watch(content)
            elif label == "gh run watch unbound":
                matched = _has_unbound_run_watch(content)
            else:
                matched = bool(pattern.search(content))
            if matched:
                if file_allowlist.get(label):
                    continue
                findings.append(f"{rel_path}: {label} pattern found")

    return findings


def _has_unbound_latest_run_list(content: str) -> bool:
    """Return true for latest-run polling that is not bound to a head/commit."""
    content = re.sub(r"\\[ \t]*\n", " ", content)
    for match in re.finditer(
        r"(?m)\bgh\s+run\s+list\b([^;&|\n]*(?:\\\n[^;&|\n]*)*)", content
    ):
        args = match.group(1).replace("\\\n", " ")
        if not _has_limit_one(args):
            continue
        if _has_nonempty_option_value(args, "--head") or _has_nonempty_option_value(
            args, "--commit"
        ):
            continue
        return True
    return False


def _has_unbound_run_watch(content: str) -> bool:
    """Return true when every run-watch command lacks an explicit numeric id."""
    content = re.sub(r"\\[ \t]*\n", " ", content)
    for match in re.finditer(
        r"(?m)\bgh\s+run\s+watch\b([^\n;&|]*)", content
    ):
        args = match.group(1)
        nested_lists = re.findall(r"\$\(\s*gh\s+run\s+list\b([^)]*)\)", args)
        if nested_lists:
            # The chained detector reports nested lists with their own
            # binding semantics; do not double-count the outer watch.
            continue
        args = re.sub(r"\$\([^)]*\)", "", args)
        try:
            tokens = shlex.split(args)
        except ValueError:
            return True
        positional = []
        index = 0
        while index < len(tokens):
            token = tokens[index]
            if token in {"--interval", "-i", "--repo", "-R"}:
                index += 2
                continue
            if token.startswith("-"):
                index += 1
                continue
            positional.append(token)
            index += 1
        if not positional or not re.fullmatch(r"\d+", positional[0]):
            return True
    return False


def _has_unbound_chained_watch(content: str) -> bool:
    """Return true only for a watch fed by an unbound latest-run list."""
    content = re.sub(r"\\[ \t]*\n", " ", content)
    for match in re.finditer(
        r"(?m)\bgh\s+run\s+watch\b([^;&|\n]*)\$\(\s*gh\s+run\s+list\b([^)]*)\)",
        content,
    ):
        nested = match.group(2)
        if not (
            _has_nonempty_option_value(nested, "--head")
            or _has_nonempty_option_value(nested, "--commit")
        ):
            return True
    return False


def _has_nonempty_option_value(args: str, option: str) -> bool:
    """Return true only when an option has a concrete non-option value."""
    pattern = rf"(?:^|\s){re.escape(option)}(?:=([^\s;&|]+)|\s+([^\s;&|]+))"
    match = re.search(pattern, args)
    if not match:
        return False
    value = match.group(1) or match.group(2) or ""
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        value = value[1:-1]
    return bool(value and not value.startswith("-"))


def _has_limit_one(args: str) -> bool:
    """Recognize shell-quoted and unquoted latest-run limit spellings."""
    try:
        tokens = shlex.split(args)
    except ValueError:
        return bool(
            re.search(
                r"(?:--limit\s*(?:=\s*)?[\"']?1[\"']?(?=\s|$)|"
                r"-L\s*[\"']?1[\"']?(?=\s|$))",
                args,
            )
        )
    for index, token in enumerate(tokens):
        if token in {"--limit", "-L"}:
            if index + 1 < len(tokens) and tokens[index + 1] == "1":
                return True
        elif token.startswith("--limit=") and token.removeprefix("--limit=") == "1":
            return True
    return False


def check_removed_plugin_surface_guard(
    repo_root: Path, tracked_files: list[str]
) -> list[str]:
    """Fail closed if the deleted in-memory plugin trust semantics reappear
    in the production crate source.

    The old plugin system was an in-memory manifest registry without binary
    loading, signature verification, sandboxing, or capability enforcement;
    its ``official`` trust level mapped to an empty permission set labeled
    ``empty = unrestricted`` and could be mistaken for a mature security
    boundary. Reintroducing that surface is a regression of the accepted
    cleanup. The guard uses a composite legacy fingerprint so that generic
    business identifiers (``official``, ``verified``) stay legal: only
    resurrection of the deleted files, the unrestricted semantic, or two or
    more legacy tokens in the production crate are flagged, even when split
    across files.
    """
    findings = []
    legacy_hits_by_file: dict[str, list[str]] = {}

    for rel_path in tracked_files:
        if not rel_path.startswith(PLUGIN_SURFACE_SCAN_PREFIXES):
            continue
        if any(
            part in AUTOMATION_GUARD_EXCLUDE_DIRS for part in Path(rel_path).parts
        ):
            continue

        if rel_path in PLUGIN_RESURRECTED_PATHS:
            findings.append(
                f"{rel_path}: deleted plugin surface file resurrected"
            )

        filepath = repo_root / rel_path
        if not filepath.is_file():
            continue
        try:
            content = filepath.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue

        semantic_content = _rust_semantic_content(content)
        if PLUGIN_UNRESTRICTED_SEMANTIC in semantic_content:
            findings.append(
                f"{rel_path}: 'empty = unrestricted' trust semantic found"
            )

        legacy_hits = [
            token for token in PLUGIN_LEGACY_FINGERPRINT_TOKENS
            if token in semantic_content
        ]
        if legacy_hits:
            legacy_hits_by_file[rel_path] = legacy_hits
        lower_content = semantic_content.lower()
        if (
            "plugin" in lower_content
            and (
                re.search(r"\b(?:pluginregistry|pluginsystem|pluginmanifest|plugintrust|trustlevel)\b", lower_content)
                or (
                    "trust" in lower_content
                    and ("permission" in lower_content or "registry" in lower_content)
                )
            )
            and re.search(r"\b(?:enum|struct|fn)\b", semantic_content)
        ):
            findings.append(
                f"{rel_path}: plugin trust/permission registry structural "
                "fingerprint found"
            )

    all_legacy_hits = sorted(
        {token for hits in legacy_hits_by_file.values() for token in hits}
    )
    if len(all_legacy_hits) >= 2:
        locations = ", ".join(
            f"{path}: {', '.join(hits)}"
            for path, hits in sorted(legacy_hits_by_file.items())
        )
        findings.append(
            "production crate contains a composite legacy plugin trust "
            f"fingerprint: {', '.join(all_legacy_hits)} ({locations})"
        )
    return findings


def _iter_rs_lines(repo_root: Path, tracked_files: list[str]):
    """Yield (rel_path, line) for engine/src rust files, skipping cfg(test)
    regions and generated/vendored directories."""
    for rel_path in tracked_files:
        if not rel_path.startswith(DORMANT_SURFACE_SCAN_PREFIX):
            continue
        if not rel_path.endswith(".rs"):
            continue
        if any(
            part in AUTOMATION_GUARD_EXCLUDE_DIRS for part in Path(rel_path).parts
        ):
            continue
        filepath = repo_root / rel_path
        if not filepath.is_file():
            continue
        try:
            content = filepath.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        yield rel_path, _strip_cfg_test_regions(content)


def _strip_cfg_test_regions(content: str) -> str:
    """Return Rust source with test-only items removed using lexical braces.

    Attributes are matched against a code mask so a ``cfg(test)`` string in a
    comment or literal cannot hide production code. Only unconditional
    ``cfg(test)`` and ``cfg(all(..., test, ...))`` regions are test-only;
    ``cfg(any(test, ...))`` and ``cfg(not(test))`` also describe production
    configurations and must remain visible to the production heuristics.
    """
    lines = content.splitlines()
    code_lines = _rust_code_mask(content).splitlines()
    out = [""] * len(lines)

    def emit(line_number: int, text: str) -> None:
        text = text.strip()
        if not text:
            return
        if out[line_number]:
            out[line_number] += " "
        out[line_number] += text

    in_test = False
    pending_test_item = False
    pending_item_text = ""
    pending_delimiter_depths = (0, 0, 0)
    pending_cfg_start_line = None
    pending_cfg_start_column = None
    pending_cfg_text = ""
    brace_depth = 0

    def consume_test_tail(
        line_number: int,
        line: str,
        tail: str,
        raw_tail_offset: int,
    ) -> None:
        nonlocal in_test
        nonlocal pending_test_item, pending_item_text
        nonlocal pending_delimiter_depths, brace_depth
        pending_item_text = tail
        pending_delimiter_depths = (0, 0, 0)
        consumed = False
        for token_index, token in _rust_top_level_tokens(tail):
            if token == "," and not _rust_comma_terminated_item(
                tail[:token_index]
            ):
                continue
            if token == "{":
                item_end = _rust_balanced_brace_end(tail, token_index)
                if item_end is None:
                    in_test = True
                    brace_depth = _rust_brace_delta(tail[token_index:])
                else:
                    suffix = line[raw_tail_offset + item_end :].strip()
                    if suffix:
                        emit(line_number, suffix)
                consumed = True
            else:
                suffix = line[raw_tail_offset + token_index + 1 :].strip()
                if suffix:
                    emit(line_number, suffix)
                consumed = True
            if consumed:
                pending_test_item = False
                pending_item_text = ""
                pending_delimiter_depths = (0, 0, 0)
                break
        if not consumed:
            pending_test_item = True
            pending_delimiter_depths = _rust_delimiter_depths(tail)

    for line_number, (line, code_line) in enumerate(zip(lines, code_lines)):
        if not in_test:
            if pending_cfg_start_line is not None:
                closing = re.search(r"\)\s*\]", code_line)
                if closing is None:
                    pending_cfg_text += "\n" + code_line
                    continue
                attribute_text = (
                    pending_cfg_text + "\n" + code_line[: closing.end()]
                )
                attribute = re.search(
                    r"#\[\s*cfg\s*\((.*)\)\s*\]",
                    attribute_text,
                    re.S,
                )
                expression = attribute.group(1) if attribute else ""
                test_only = bool(
                    attribute and _cfg_expression_requires_test(expression)
                )
                if test_only:
                    consume_test_tail(
                        line_number,
                        line,
                        code_line[closing.end() :],
                        closing.end(),
                    )
                else:
                    emit(
                        pending_cfg_start_line,
                        lines[pending_cfg_start_line][pending_cfg_start_column:],
                    )
                    for skipped_line in range(
                        pending_cfg_start_line + 1, line_number + 1
                    ):
                        emit(skipped_line, lines[skipped_line])
                pending_cfg_start_line = None
                pending_cfg_start_column = None
                pending_cfg_text = ""
                continue
            if pending_test_item:
                if not line.strip() or line.lstrip().startswith("#"):
                    continue
                item_prefix = pending_item_text
                if item_prefix and code_line.strip():
                    item_prefix += "\n"
                item_prefix += code_line
                for token_index, token in _rust_top_level_tokens(
                    code_line, pending_delimiter_depths
                ):
                    if token == "," and not _rust_comma_terminated_item(
                        pending_item_text + code_line[:token_index]
                    ):
                        continue
                    if token == "{":
                        item_end = _rust_balanced_brace_end(code_line, token_index)
                        if item_end is None:
                            in_test = True
                            pending_test_item = False
                            brace_depth = _rust_brace_delta(code_line[token_index:])
                        else:
                            suffix = line[item_end:].strip()
                            if suffix:
                                emit(line_number, suffix)
                            pending_test_item = False
                        pending_item_text = ""
                        pending_delimiter_depths = (0, 0, 0)
                        break
                    suffix = line[token_index + 1 :].strip()
                    if suffix:
                        emit(line_number, suffix)
                    pending_test_item = False
                    pending_item_text = ""
                    pending_delimiter_depths = (0, 0, 0)
                    break
                else:
                    pending_item_text = item_prefix
                    pending_delimiter_depths = _rust_delimiter_depths(
                        code_line, pending_delimiter_depths
                    )
                continue
            attr = re.search(
                r"#\[\s*cfg\s*\(([^\]]*)\)\s*\]",
                code_line,
            )
            expression = attr.group(1) if attr else ""
            test_only = bool(attr and _cfg_expression_requires_test(expression))
            if test_only:
                production_prefix = line[: attr.start()].strip()
                if production_prefix:
                    emit(line_number, production_prefix)
                consume_test_tail(
                    line_number,
                    line,
                    code_line[attr.end() :],
                    attr.end(),
                )
            elif attr is None and re.search(r"#\[\s*cfg\s*\(", code_line):
                cfg_start = re.search(r"#\[\s*cfg\s*\(", code_line)
                assert cfg_start is not None
                production_prefix = line[: cfg_start.start()].strip()
                if production_prefix:
                    emit(line_number, production_prefix)
                pending_cfg_start_line = line_number
                pending_cfg_start_column = cfg_start.start()
                pending_cfg_text = code_line[cfg_start.start() :]
            else:
                emit(line_number, line)
            continue
        item_end = _rust_region_end(code_line, brace_depth)
        if item_end is not None:
            suffix = line[item_end:].strip()
            if suffix:
                emit(line_number, suffix)
            in_test = False
            brace_depth = 0
        else:
            brace_depth += _rust_brace_delta(code_line)
    return "\n".join(out)


def _rust_top_level_tokens(
    code: str, initial_depths: tuple[int, int, int] = (0, 0, 0)
):
    """Yield item delimiters outside Rust (), [], and {} delimiters."""
    paren_depth, bracket_depth, brace_depth = initial_depths
    for index, current in enumerate(code):
        if current in "{;," and not any(
            (paren_depth, bracket_depth, brace_depth)
        ):
            yield index, current
        if current == "(":
            paren_depth += 1
        elif current == ")":
            paren_depth = max(0, paren_depth - 1)
        elif current == "[":
            bracket_depth += 1
        elif current == "]":
            bracket_depth = max(0, bracket_depth - 1)
        elif current == "{":
            brace_depth += 1
        elif current == "}":
            brace_depth = max(0, brace_depth - 1)


def _rust_delimiter_depths(
    code: str, initial_depths: tuple[int, int, int] = (0, 0, 0)
) -> tuple[int, int, int]:
    """Return unmatched Rust (), [], and {} delimiter depths for a line."""
    paren_depth, bracket_depth, brace_depth = initial_depths
    for current in code:
        if current == "(":
            paren_depth += 1
        elif current == ")":
            paren_depth = max(0, paren_depth - 1)
        elif current == "[":
            bracket_depth += 1
        elif current == "]":
            bracket_depth = max(0, bracket_depth - 1)
        elif current == "{":
            brace_depth += 1
        elif current == "}":
            brace_depth = max(0, brace_depth - 1)
    return paren_depth, bracket_depth, brace_depth


def _rust_balanced_brace_end(code: str, opening_index: int) -> int | None:
    """Return the offset immediately after a balanced Rust brace item."""
    depth = 0
    for index in range(opening_index, len(code)):
        current = code[index]
        if current == "{":
            depth += 1
        elif current == "}":
            depth -= 1
            if depth == 0:
                return index + 1
    return None


def _rust_region_end(code: str, initial_depth: int) -> int | None:
    """Return the offset after a test region closes, if it closes on this line."""
    depth = initial_depth
    for index, current in enumerate(code):
        if current == "{":
            depth += 1
        elif current == "}":
            depth -= 1
            if depth <= 0:
                return index + 1
    return None


def _rust_field_prefix(prefix: str) -> bool:
    """Identify a field prefix without treating function arguments as fields."""
    return bool(
        re.search(r"\b[A-Za-z_][A-Za-z0-9_]*\s*:\s*[^:]", prefix)
        and not re.search(
            r"\b(?:fn|struct|enum|mod|const|static|type|use|impl|trait)\b",
            prefix,
        )
    )


def _rust_comma_terminated_item(prefix: str) -> bool:
    """Identify a struct field or enum variant ending at a comma."""
    if _rust_field_prefix(prefix):
        return True
    return bool(
        re.fullmatch(
            r"\s*(?:pub(?:\([^)]*\))?\s+)?[A-Za-z_][A-Za-z0-9_]*"
            r"(?:\s*::\s*[A-Za-z_][A-Za-z0-9_]*)*"
            r"(?:\s*\([^{}]*\))?(?:\s*=\s*[^;]+)?\s*",
            prefix,
        )
    )


def _rust_code_mask(content: str) -> str:
    """Mask Rust comments and literals while preserving code punctuation."""
    chars = list(content)
    masked = list(content)
    index = 0
    block_depth = 0
    string = False
    char = False
    escaped = False
    raw_hashes: int | None = None
    while index < len(chars):
        current = chars[index]
        next_char = chars[index + 1] if index + 1 < len(chars) else ""
        if block_depth:
            if current == "/" and next_char == "*":
                block_depth += 1
                masked[index] = masked[index + 1] = " "
                index += 2
                continue
            if current == "*" and next_char == "/":
                block_depth -= 1
                masked[index] = masked[index + 1] = " "
                index += 2
                continue
            if current != "\n":
                masked[index] = " "
            index += 1
            continue
        if raw_hashes is not None:
            terminator = '"' + ("#" * raw_hashes)
            if content.startswith(terminator, index):
                for offset in range(len(terminator)):
                    masked[index + offset] = " "
                index += len(terminator)
                raw_hashes = None
            elif current != "\n":
                masked[index] = " "
                index += 1
            else:
                index += 1
            continue
        if string:
            if current == '"' and not escaped:
                masked[index] = " "
                string = False
            elif current != "\n":
                masked[index] = " "
            escaped = current == "\\" and not escaped
            if current != "\\":
                escaped = False
            index += 1
            continue
        if char:
            if current == "'" and not escaped:
                masked[index] = " "
                char = False
            elif current != "\n":
                masked[index] = " "
            escaped = current == "\\" and not escaped
            if current != "\\":
                escaped = False
            index += 1
            continue
        if current == "/" and next_char == "/":
            masked[index] = masked[index + 1] = " "
            index += 2
            while index < len(chars) and chars[index] != "\n":
                masked[index] = " "
                index += 1
            continue
        if current == "/" and next_char == "*":
            masked[index] = masked[index + 1] = " "
            block_depth = 1
            index += 2
            continue
        if current == "r":
            quote_index = index + 1
            while quote_index < len(chars) and chars[quote_index] == "#":
                quote_index += 1
            if quote_index < len(chars) and chars[quote_index] == '"':
                raw_hashes = quote_index - index - 1
                for offset in range(quote_index - index + 1):
                    masked[index + offset] = " "
                index = quote_index + 1
                continue
        if current == '"':
            masked[index] = " "
            string = True
        elif current == "'":
            char_end = index + (3 if index + 1 < len(chars) and chars[index + 1] == "\\" else 2)
            if char_end < len(chars) and chars[char_end] == "'":
                for offset in range(char_end - index + 1):
                    masked[index + offset] = " "
                index = char_end + 1
                continue
        index += 1
    return "".join(masked)


def _rust_brace_delta(line: str) -> int:
    """Count Rust braces while ignoring comments, strings, and char literals."""
    delta = 0
    index = 0
    in_string = False
    in_char = False
    escaped = False
    while index < len(line):
        char = line[index]
        next_char = line[index + 1] if index + 1 < len(line) else ""
        if not in_string and not in_char and char == "/" and next_char == "/":
            break
        if not in_string and not in_char and char == "/" and next_char == "*":
            end = line.find("*/", index + 2)
            if end == -1:
                break
            index = end + 2
            continue
        if not in_char and char == '"' and not escaped:
            in_string = not in_string
        elif not in_string and char == "'" and not escaped:
            in_char = not in_char
        elif not in_string and not in_char:
            if char == "{":
                delta += 1
            elif char == "}":
                delta -= 1
        escaped = char == "\\" and not escaped
        if char != "\\":
            escaped = False
        index += 1
    return delta


def _rust_semantic_content(content: str) -> str:
    """Remove test-only regions, comments, and literals before fingerprinting."""
    return _rust_code_mask(_strip_cfg_test_regions(content))


def check_dormant_surface_heuristics(
    repo_root: Path, tracked_files: list[str]
) -> list[str]:
    """Fail closed on new dormant production surfaces in engine/src.

    Categories: module-level dead-code blankets, lib.rs-declared module
    islands, self-described placeholder/stub/not-wired module headers,
    executor-named empty functions, and conflicting sole-owner claims.
    Findings are suppressed only by DORMANT_SURFACE_CLASSIFICATION_ALLOWLIST
    entries.
    """
    findings = _validate_dormant_surface_allowlist()
    allowed = {
        (entry["path"], entry["classification"])
        for entry in DORMANT_SURFACE_CLASSIFICATION_ALLOWLIST
        if isinstance(entry, dict)
        and isinstance(entry.get("path"), str)
        and isinstance(entry.get("classification"), str)
        and entry.get("path", "").strip()
        and entry.get("classification", "").strip()
    }

    def suppressed(rel_path: str, classification: str) -> bool:
        return (rel_path, classification) in allowed

    # (a) module-level dead-code blankets
    for rel_path, content in _iter_rs_lines(repo_root, tracked_files):
        for line_no, line in enumerate(content.splitlines(), 1):
            if line.strip() == "#![allow(dead_code)]":
                findings.append(
                    f"{rel_path}:{line_no}: module-level "
                    "#![allow(dead_code)] blanket in production crate"
                )

    # (b) module islands among top-level lib.rs modules
    lib_path = repo_root / "engine" / "src" / "lib.rs"
    try:
        lib_content = lib_path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        lib_content = ""
    lib_content = _strip_cfg_test_regions(lib_content)
    declared_modules = re.findall(
        r"^\s*(?:pub(?:\(crate\))?\s+)?mod\s+(\w+)\s*;",
        lib_content,
        re.M,
    )
    src_dir = repo_root / "engine" / "src"
    all_src_files = [
        f for f in tracked_files if f.startswith("engine/src/") and f.endswith(".rs")
    ]
    for module in declared_modules:
        module_file = (
            f"engine/src/{module}.rs"
            if (src_dir / f"{module}.rs").is_file()
            else f"engine/src/{module}/"
        )
        if suppressed(module_file, "generated") or suppressed(
            module_file, "bin_consumer"
        ):
            continue
        consumers = [
            f
            for f in all_src_files
            if f != f"engine/src/lib.rs"
            and not f.startswith(f"engine/src/{module}/")
            and not f.startswith(f"engine/src/{module}.")
            and not f.startswith("engine/src/bin/")
        ]
        referenced = False
        for f in consumers:
            try:
                content = (repo_root / f).read_text(
                    encoding="utf-8", errors="replace"
                )
            except OSError:
                continue
            content = _rust_semantic_content(content)
            if re.search(rf"\bcrate::{module}\b|\bengine::{module}\b", content):
                referenced = True
                break
        if not referenced:
            findings.append(
                f"engine/src/lib.rs: module '{module}' has no consumer "
                "outside its own directory, lib.rs, engine/tests, or "
                "engine/src/bin (dormant module island)"
            )

    # (c) self-described dormant module headers
    for rel_path, content in _iter_rs_lines(repo_root, tracked_files):
        header = "\n".join(content.splitlines()[:40])
        for descriptor in DORMANT_SURFACE_SELF_DESCRIPTORS:
            if descriptor.search(header):
                if suppressed(rel_path, "wired") or suppressed(
                    rel_path, "generated"
                ):
                    continue
                findings.append(
                    f"{rel_path}: header self-describes as dormant surface "
                    f"({descriptor.pattern})"
                )

    # (d) executor-named empty functions
    for rel_path, content in _iter_rs_lines(repo_root, tracked_files):
        clean = _rust_semantic_content(content)
        for match in DORMANT_EXECUTOR_FN_RE.finditer(clean):
            line_no = clean.count("\n", 0, match.start()) + 1
            brace = clean.find("{", match.end())
            if brace == -1:
                continue
            depth = 0
            body_end = -1
            for idx in range(brace, len(clean)):
                if clean[idx] == "{":
                    depth += 1
                elif clean[idx] == "}":
                    depth -= 1
                    if depth == 0:
                        body_end = idx
                        break
            if body_end == -1:
                continue
            body = clean[brace + 1 : body_end]
            if DORMANT_EMPTY_BODY_RE.search(body):
                if not (
                    suppressed(rel_path, "wired")
                    or suppressed(rel_path, "generated")
                ):
                    findings.append(
                        f"{rel_path}:{line_no}: executor-named function "
                        f"'{match.group(0)[3:]}' returns only an empty value "
                        "(no-op executor surface)"
                    )

    # (e) conflicting sole-owner claims
    claims: dict[str, list[str]] = {}
    for rel_path, content in _iter_rs_lines(repo_root, tracked_files):
        for match in DORMANT_OWNERSHIP_CLAIM_RE.finditer(content):
            claims.setdefault(match.group(1), []).append(rel_path)
    for claimed, paths in claims.items():
        unique = sorted(set(paths))
        unsuppressed = [
            path for path in unique if not suppressed(path, "sole_owner")
        ]
        if len(unsuppressed) > 1:
            findings.append(
                f"conflicting sole-owner claims for '{claimed}' across: "
                + ", ".join(unsuppressed)
            )

    return findings


def _validate_dormant_surface_allowlist() -> list[str]:
    required = (
        "path",
        "classification",
        "owner",
        "reason",
        "review_condition",
        "expiry_or_recheck_condition",
    )
    findings = []
    for index, entry in enumerate(DORMANT_SURFACE_CLASSIFICATION_ALLOWLIST):
        if not isinstance(entry, dict):
            findings.append(
                f"dormant classification allowlist entry {index} is not an object"
            )
            continue
        missing = []
        for field in required:
            value = entry.get(field)
            if not isinstance(value, str) or not value.strip():
                missing.append(field)
        if missing:
            findings.append(
                f"dormant classification allowlist entry {index} missing "
                + ", ".join(missing)
            )
    return findings


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

CHECK_LABELS = [
    "Secret scan",
    "Import scan (AST)",
    "Active routing guard",
    "Governance boundary guard",
    "Stage-0 event guard",
    "Dormant automation guard",
    "Removed plugin-surface guard",
    "Dormant surface heuristics",
]


def main() -> int:
    print("=" * 60)
    print("CA-7 Security Baseline Checker")
    print("=" * 60)
    print(f"Repository: {REPO_ROOT}")
    print()

    tracked_files = git_ls_files(REPO_ROOT)
    if not tracked_files:
        print("WARNING: Could not list git-tracked files")
        tracked_files = []

    checks = [
        lambda: check_secret_scan(REPO_ROOT, tracked_files),
        lambda: check_import_scan(REPO_ROOT, tracked_files),
        lambda: check_active_routing(REPO_ROOT, tracked_files),
        lambda: check_governance_boundary(REPO_ROOT),
        lambda: check_stage0_event_guard(REPO_ROOT),
        lambda: check_dormant_automation_guard(REPO_ROOT, tracked_files),
        lambda: check_removed_plugin_surface_guard(REPO_ROOT, tracked_files),
        lambda: check_dormant_surface_heuristics(REPO_ROOT, tracked_files),
    ]

    all_pass = True
    total = len(CHECK_LABELS)
    for index, (label, check_fn) in enumerate(
        zip(CHECK_LABELS, checks), start=1
    ):
        print(f"[{index}/{total}] {label}...")
        findings = check_fn()
        if findings:
            print("  FAIL:")
            for f in findings:
                print(f"    {f}")
            all_pass = False
        else:
            print("  PASS")

    # Summary
    print()
    print("=" * 60)
    if all_pass:
        print("RESULT: ALL CHECKS PASSED")
    else:
        print("RESULT: ONE OR MORE CHECKS FAILED")
    print("=" * 60)

    return 0 if all_pass else 1


if __name__ == "__main__":
    sys.exit(main())
