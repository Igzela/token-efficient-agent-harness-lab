#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import subprocess
from dataclasses import asdict, dataclass
from pathlib import Path


SECRET_PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    ("anthropic_token", re.compile(r"\btp-[A-Za-z0-9]{24,}\b")),
    ("openrouter_key", re.compile(r"\bsk-or-v1-[A-Za-z0-9]{24,}\b")),
    ("openai_key", re.compile(r"\bsk-[A-Za-z0-9]{32,}\b")),
    ("google_key", re.compile(r"\bAIza[A-Za-z0-9_-]{20,}\b")),
    ("aws_access_key", re.compile(r"\bAKIA[0-9A-Z]{16}\b")),
    ("local_admin_key", re.compile(r"\bharness_[0-9a-fA-F]{64}\b")),
]

SENSITIVE_ASSIGNMENT = re.compile(
    r"(?i)\b(api[_-]?key|auth[_-]?token|access[_-]?token|secret|password|credential)\b\s*=\s*([^#\s].*)"
)

PLACEHOLDER_VALUES = {
    "",
    "***",
    "<64 hex chars>",
    "<api key>",
    "<api_key>",
    "<secret>",
    "<token>",
    "changeme",
    "example",
    "placeholder",
    "replace_me",
    "todo",
}

DEFAULT_EXTRA_FILES = [
    ".env",
    ".env.local",
    ".env.production-like.local",
]

DEFAULT_EXCLUDE_PREFIXES = (
    "engine/tests/",
    "sdk/python/tests/",
)

DEFAULT_EXCLUDE_FILES = {
    "tools/test_security_baseline.py",
}


@dataclass
class Finding:
    file: str
    line: int
    kind: str
    preview: str


def repo_root_from_script() -> Path:
    return Path(__file__).resolve().parents[1]


def git_tracked_files(repo_root: Path) -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files"],
        cwd=repo_root,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        return []
    return [repo_root / line for line in result.stdout.splitlines() if line.strip()]


def is_text_file(path: Path) -> bool:
    return path.suffix.lower() not in {
        ".a",
        ".bz2",
        ".dll",
        ".dylib",
        ".egg",
        ".gif",
        ".gz",
        ".ico",
        ".jpeg",
        ".jpg",
        ".o",
        ".pdf",
        ".png",
        ".pyc",
        ".so",
        ".tar",
        ".whl",
        ".xz",
        ".zip",
    }


def normalize_value(raw: str) -> str:
    value = raw.strip()
    if " " in value:
        value = value.split()[0]
    return value.rstrip(",;").strip('"').strip("'").strip()


def allowed_assignment_value(value: str) -> bool:
    normalized = normalize_value(value)
    code_expr = normalized.rstrip(",;")
    lowered = normalized.lower()
    if lowered in PLACEHOLDER_VALUES:
        return True
    if "<" in normalized and ">" in normalized:
        return True
    if normalized.startswith("${") and normalized.endswith("}"):
        return True
    if normalized.startswith("$"):
        return True
    if re.fullmatch(r"[A-Z][A-Z0-9_]{2,}", normalized):
        return True
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)*", code_expr):
        return True
    if any(marker in normalized for marker in ("(", ")", "?", "{", "}")):
        return True
    return False


def redact(line: str) -> str:
    redacted = line.rstrip("\n")
    for _, pattern in SECRET_PATTERNS:
        redacted = pattern.sub("***", redacted)
    match = SENSITIVE_ASSIGNMENT.search(redacted)
    if match and not allowed_assignment_value(match.group(2)):
        redacted = f"{redacted[:match.start(2)]}***"
    return redacted[:160]


def scan_file(repo_root: Path, path: Path) -> list[Finding]:
    findings: list[Finding] = []
    if not path.exists() or not path.is_file() or not is_text_file(path):
        return findings
    try:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return findings
    rel = str(path.relative_to(repo_root)) if path.is_relative_to(repo_root) else str(path)
    if rel in DEFAULT_EXCLUDE_FILES or rel.startswith(DEFAULT_EXCLUDE_PREFIXES):
        return findings
    for idx, line in enumerate(lines, start=1):
        for kind, pattern in SECRET_PATTERNS:
            if pattern.search(line):
                findings.append(Finding(rel, idx, kind, redact(line)))
        match = SENSITIVE_ASSIGNMENT.search(line)
        if match and not allowed_assignment_value(match.group(2)):
            findings.append(Finding(rel, idx, "sensitive_assignment", redact(line)))
    return findings


def collect_paths(repo_root: Path, explicit_paths: list[Path]) -> list[Path]:
    if explicit_paths:
        return [path if path.is_absolute() else repo_root / path for path in explicit_paths]
    paths = git_tracked_files(repo_root)
    for rel in DEFAULT_EXTRA_FILES:
        candidate = repo_root / rel
        if candidate.exists() and candidate not in paths:
            paths.append(candidate)
    return paths


def main() -> int:
    parser = argparse.ArgumentParser(description="Scan Agent Control Plane files for committed secrets.")
    parser.add_argument("paths", nargs="*", type=Path, help="Optional files to scan.")
    parser.add_argument("--repo-root", type=Path, default=repo_root_from_script())
    parser.add_argument("--json", action="store_true", help="Print machine-readable JSON.")
    args = parser.parse_args()

    repo_root = args.repo_root.resolve()
    findings: list[Finding] = []
    for path in collect_paths(repo_root, args.paths):
        findings.extend(scan_file(repo_root, path.resolve()))

    if args.json:
        print(json.dumps({"findings": [asdict(item) for item in findings]}, indent=2))
    elif findings:
        print("Secret scan findings:")
        for finding in findings:
            print(f"- {finding.file}:{finding.line} [{finding.kind}] {finding.preview}")
    else:
        print("Secret scan passed.")
    return 1 if findings else 0


if __name__ == "__main__":
    raise SystemExit(main())
