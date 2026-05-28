#!/usr/bin/env python3
"""Security baseline checker for CA-7 sealed baseline.

Read-only, pure stdlib. Performs five checks against the repository:
  1. Secret scan — regex scan for credential patterns in git-tracked files
  2. Import scan — AST-based scan for prohibited network/SDK imports
  3. Active routing guard — scan JSON for active_routing_allowed: true
  4. Governance boundary guard — verify governance fixtures exist
  5. Stage-0 event guard — verify events.jsonl exists and is intact

Exit code 0 = all checks pass, 1 = at least one check fails.
"""

from __future__ import annotations

import ast
import json
import os
import re
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
    re.compile(r"""api_key\s*=\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""secret\s*=\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""token\s*=\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""password\s*=\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""api[_-]?key\s*[:=]\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""secret[_-]?key\s*[:=]\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""access[_-]?token\s*[:=]\s*['"][^'"]+['"]""", re.IGNORECASE),
    re.compile(r"""bearer\s+[A-Za-z0-9._-]+""", re.IGNORECASE),
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
    "tests/test_security_baseline.py",
    "tests/test_http_server.py",
    "tests/test_auth.py",
}

# Per-file import allowlists for test files that legitimately use stdlib
# network modules for local integration testing (not runtime providers).
# Each key is a file path; the value is the set of imports allowed in that file.
ALLOWED_TEST_IMPORTS: dict[str, set[str]] = {
    "tests/test_http_server.py": {"urllib.request", "urllib.error", "socket"},
}

# Paths to exclude from active routing guard (test fixtures contain
# intentional active_routing_allowed values for testing).
ACTIVE_ROUTING_EXCLUDE_PREFIXES = (
    "tests/fixtures/",
)


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
    instead of the global prohibited set. This allows test files to use stdlib
    network modules for local integration testing while still flagging any
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


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

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

    all_pass = True

    # Check 1: Secret scan
    print("[1/5] Secret scan...")
    secret_findings = check_secret_scan(REPO_ROOT, tracked_files)
    if secret_findings:
        print("  FAIL — credential patterns found:")
        for f in secret_findings:
            print(f"    {f}")
        all_pass = False
    else:
        print("  PASS")

    # Check 2: Import scan
    print("[2/5] Import scan (AST)...")
    import_findings = check_import_scan(REPO_ROOT, tracked_files)
    if import_findings:
        print("  FAIL — prohibited imports found:")
        for f in import_findings:
            print(f"    {f}")
        all_pass = False
    else:
        print("  PASS")

    # Check 3: Active routing guard
    print("[3/5] Active routing guard...")
    routing_findings = check_active_routing(REPO_ROOT, tracked_files)
    if routing_findings:
        print("  FAIL — active routing detected:")
        for f in routing_findings:
            print(f"    {f}")
        all_pass = False
    else:
        print("  PASS")

    # Check 4: Governance boundary guard
    print("[4/5] Governance boundary guard...")
    gov_findings = check_governance_boundary(REPO_ROOT)
    if gov_findings:
        print("  FAIL — governance boundary issues:")
        for f in gov_findings:
            print(f"    {f}")
        all_pass = False
    else:
        print("  PASS")

    # Check 5: Stage-0 event guard
    print("[5/5] Stage-0 event guard...")
    event_findings = check_stage0_event_guard(REPO_ROOT)
    if event_findings:
        print("  FAIL — event guard issues:")
        for f in event_findings:
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
