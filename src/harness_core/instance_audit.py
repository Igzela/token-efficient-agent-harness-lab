"""Read-only harness instance auditor.

This module inspects a target repository that is trying to use the
Token-Efficient Agent Harness as a project governance layer. It performs
static, read-only checks against the target repository's docs/harness control
files and AGENTS.md policy.

It does not execute target project code, call providers, start sandboxes, or
write to the target repository.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
import json
import re
from typing import Any


REQUIRED_FILES = (
    "AGENTS.md",
    "docs/harness/PROJECT_BOARD.md",
    "docs/harness/TASK_QUEUE.md",
    "docs/harness/QUALITY_GATES.md",
    "docs/harness/DECISION_RECORD.md",
    "docs/harness/RISK_REGISTER.md",
)

OPTIONAL_RECOMMENDED_FILES = (
    "docs/harness/FINAL_GATE.md",
    "docs/harness/EVIDENCE_INDEX.md",
)


@dataclass(frozen=True)
class AuditCheck:
    """Single audit check result."""

    check_id: str
    status: str
    message: str
    evidence: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "check_id": self.check_id,
            "status": self.status,
            "message": self.message,
            "evidence": list(self.evidence),
        }


@dataclass(frozen=True)
class InstanceAuditReport:
    """Read-only audit result for one target project instance."""

    target_repo: str
    verdict: str
    checks: list[AuditCheck]
    warnings: list[str]
    blockers: list[str]
    recommended_next_actions: list[str]

    def to_dict(self) -> dict[str, Any]:
        return {
            "target_repo": self.target_repo,
            "verdict": self.verdict,
            "checks": [check.to_dict() for check in self.checks],
            "warnings": list(self.warnings),
            "blockers": list(self.blockers),
            "recommended_next_actions": list(self.recommended_next_actions),
        }

    def to_json(self, *, indent: int = 2) -> str:
        return json.dumps(self.to_dict(), ensure_ascii=False, indent=indent, sort_keys=True)


@dataclass
class _AuditState:
    target_repo: Path
    checks: list[AuditCheck] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    blockers: list[str] = field(default_factory=list)

    def add_check(self, check_id: str, status: str, message: str, evidence: list[str] | None = None) -> None:
        self.checks.append(AuditCheck(check_id, status, message, evidence or []))

    def warn(self, message: str) -> None:
        if message not in self.warnings:
            self.warnings.append(message)

    def block(self, message: str) -> None:
        if message not in self.blockers:
            self.blockers.append(message)


def audit_instance(target_repo: str | Path) -> InstanceAuditReport:
    """Audit a target repository as a harness project instance.

    The audit is static and read-only. It uses pathlib file reads only.
    """

    root = Path(target_repo).expanduser().resolve()
    state = _AuditState(root)

    if not root.exists() or not root.is_dir():
        state.block(f"Target repository not found or not a directory: {root}")
        state.add_check("target_repo", "FAIL", "target repository is unavailable")
        return _finalize(state)

    _check_required_files(state)
    _check_optional_files(state)
    _check_agents_policy(state)
    _check_project_board(state)
    _check_task_queue(state)
    _check_quality_gates(state)
    _check_risk_register(state)
    _check_closeout_reports(state)

    return _finalize(state)


def format_report(report: InstanceAuditReport) -> str:
    """Return a human-readable audit report."""

    lines = [
        "=" * 72,
        "Harness App MVP0 — Read-Only Project Instance Audit",
        "=" * 72,
        f"Target repo: {report.target_repo}",
        f"Verdict: {report.verdict}",
        "",
        "Checks:",
    ]
    for check in report.checks:
        lines.append(f"- [{check.status}] {check.check_id}: {check.message}")
        for item in check.evidence:
            lines.append(f"  - {item}")

    lines.append("")
    lines.append("Warnings:")
    if report.warnings:
        lines.extend(f"- {warning}" for warning in report.warnings)
    else:
        lines.append("- None")

    lines.append("")
    lines.append("Blockers:")
    if report.blockers:
        lines.extend(f"- {blocker}" for blocker in report.blockers)
    else:
        lines.append("- None")

    lines.append("")
    lines.append("Recommended next actions:")
    for action in report.recommended_next_actions:
        lines.append(f"- {action}")

    return "\n".join(lines)


def _read_text(root: Path, rel_path: str) -> str:
    path = root / rel_path
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def _exists(root: Path, rel_path: str) -> bool:
    return (root / rel_path).is_file()


def _contains_all(text: str, terms: tuple[str, ...]) -> bool:
    lowered = text.lower()
    return all(term.lower() in lowered for term in terms)


def _contains_any(text: str, terms: tuple[str, ...]) -> bool:
    lowered = text.lower()
    return any(term.lower() in lowered for term in terms)


def _check_required_files(state: _AuditState) -> None:
    missing = [path for path in REQUIRED_FILES if not _exists(state.target_repo, path)]
    if missing:
        for path in missing:
            state.block(f"Missing required harness control file: {path}")
        state.add_check("required_files", "FAIL", "required harness control files are missing", missing)
    else:
        state.add_check("required_files", "PASS", "all required harness control files are present", list(REQUIRED_FILES))


def _check_optional_files(state: _AuditState) -> None:
    missing = [path for path in OPTIONAL_RECOMMENDED_FILES if not _exists(state.target_repo, path)]
    if missing:
        state.warn("Missing optional recommended control files: " + ", ".join(missing))
        state.add_check("optional_files", "WARN", "some optional recommended control files are missing", missing)
    else:
        state.add_check("optional_files", "PASS", "optional recommended control files are present", list(OPTIONAL_RECOMMENDED_FILES))


def _check_agents_policy(state: _AuditState) -> None:
    text = _read_text(state.target_repo, "AGENTS.md")
    if not text:
        state.add_check("agents_policy", "FAIL", "AGENTS.md is missing or unreadable")
        return

    evidence: list[str] = []

    if _contains_all(text, ("execution adapter",)):
        evidence.append("agent is described as execution adapter")
    else:
        state.warn("AGENTS.md does not clearly describe the agent as an execution adapter")

    if _contains_all(text, ("not", "governance authority")) or _contains_all(text, ("not the governance authority",)):
        evidence.append("agent is not governance authority")
    else:
        state.block("AGENTS.md does not clearly deny governance authority to the agent")

    if _contains_any(text, ("human authorisation", "human authorization", "explicit human", "requires human")):
        evidence.append("human authority is referenced")
    else:
        state.block("AGENTS.md does not require human authority for governance decisions")

    if re.search(r"push(?:ing)?\s+(?:directly\s+)?to\s+`?(?:main|master)`?\s+without\s+approval", text, re.IGNORECASE):
        state.block("AGENTS.md allows pushing main/master without approval")
    elif _contains_any(text, ("pushing directly to `main`", "pushing directly to main", "push directly to main", "push to main")):
        if _contains_any(text, ("pause", "must not", "requires", "before", "approval")):
            evidence.append("main/master push requires pause or approval")
        else:
            state.block("AGENTS.md mentions main/master push without an approval guard")
    else:
        state.warn("AGENTS.md does not explicitly mention main/master push restrictions")

    if _contains_any(text, ("provider", "llm provider", "real llm")):
        if _contains_any(text, ("must not", "not allowed", "without approval", "requires approval", "do not connect")):
            evidence.append("provider integration is guarded")
        else:
            state.warn("AGENTS.md mentions provider integration without a clear guard")
    else:
        state.warn("AGENTS.md does not mention provider integration boundaries")

    if _contains_any(text, ("fully automated", "default fully automated", "ordinary engineering work")):
        if _contains_any(text, ("pause for explicit human confirmation", "must still pause", "before")):
            state.warn("AGENTS.md allows broad automation but includes pause conditions")
            evidence.append("broad automation has pause conditions")
        else:
            state.block("AGENTS.md allows broad automation without pause conditions")

    if _contains_any(text, ("active yaml", "active state", "user/project state")):
        if _contains_any(text, ("approval", "human", "must not", "pause")):
            evidence.append("active state mutation is guarded")
        else:
            state.block("AGENTS.md does not guard active state mutation")
    else:
        state.warn("AGENTS.md does not explicitly guard active user/project state mutation")

    status = "FAIL" if any("AGENTS.md" in blocker for blocker in state.blockers) else ("WARN" if evidence else "WARN")
    if status != "FAIL" and evidence:
        status = "PASS_WITH_NOTES" if any("AGENTS.md" in warning for warning in state.warnings) else "PASS"
    state.add_check("agents_policy", status, "AGENTS.md execution adapter policy reviewed", evidence)


def _check_project_board(state: _AuditState) -> None:
    text = _read_text(state.target_repo, "docs/harness/PROJECT_BOARD.md")
    if not text:
        state.add_check("project_board", "FAIL", "PROJECT_BOARD.md is missing or unreadable")
        return

    evidence: list[str] = []
    if _contains_all(text, ("todo", "ready", "running", "review", "done")):
        evidence.append("task state vocabulary exists")
    else:
        state.warn("PROJECT_BOARD.md does not expose the expected task state vocabulary")

    if _contains_any(text, ("phase", "sealed baseline", "closeout")):
        evidence.append("phase/closeout status appears documented")
    else:
        state.warn("PROJECT_BOARD.md does not clearly expose phase or closeout status")

    malformed_rows = _find_malformed_markdown_table_rows(text)
    if malformed_rows:
        state.warn("PROJECT_BOARD.md has structurally suspicious table rows: " + "; ".join(malformed_rows[:5]))

    future_done = re.search(r"\|\s*(?:P5|CA-8|Stage 5)[^|]*\|[^|]*\|\s*(?:\*\*)?done", text, re.IGNORECASE)
    if future_done and not _contains_any(text, ("P5-000 remains blocked", "CA-8 has not started", "Stage 5 not started")):
        state.block("Future phase appears marked done without clear closeout/blocking evidence")

    if _contains_any(text, ("ready-with-approval", "blocked", "pending_human", "pending GPT/human")):
        evidence.append("approval/blocking statuses are visible")
    else:
        state.warn("PROJECT_BOARD.md does not show approval/blocking statuses")

    status = "FAIL" if any("Future phase" in blocker for blocker in state.blockers) else ("PASS_WITH_NOTES" if malformed_rows else "PASS")
    state.add_check("project_board", status, "project board sanity check complete", evidence)


def _check_task_queue(state: _AuditState) -> None:
    text = _read_text(state.target_repo, "docs/harness/TASK_QUEUE.md")
    if not text:
        state.add_check("task_queue", "FAIL", "TASK_QUEUE.md is missing or unreadable")
        return

    evidence: list[str] = []
    slice_count = len(re.findall(r"^###\s+", text, flags=re.MULTILINE))
    if slice_count > 0:
        evidence.append(f"execution slices found: {slice_count}")
    else:
        state.block("TASK_QUEUE.md has no execution slices")

    status_count = len(re.findall(r"\*\*Status\*\*\s*:", text))
    goal_count = len(re.findall(r"\*\*Goal\*\*\s*:", text))
    if status_count == 0 or goal_count == 0:
        state.warn("TASK_QUEUE.md slices may be missing Status/Goal fields")
    else:
        evidence.append(f"Status fields: {status_count}; Goal fields: {goal_count}")

    if _contains_any(text, ("ready-with-approval", "blocked", "paused", "retired")):
        evidence.append("non-executable statuses are present")
    else:
        state.warn("TASK_QUEUE.md does not show blocked/approval status vocabulary")

    if re.search(r"\*\*Status\*\*\s*:\s*(ready-with-approval|blocked)", text, re.IGNORECASE):
        evidence.append("approval-gated or blocked slices detected")

    status = "FAIL" if any("TASK_QUEUE" in blocker for blocker in state.blockers) else "PASS_WITH_NOTES" if state.warnings else "PASS"
    state.add_check("task_queue", status, "task queue sanity check complete", evidence)


def _check_quality_gates(state: _AuditState) -> None:
    text = _read_text(state.target_repo, "docs/harness/QUALITY_GATES.md")
    if not text:
        state.add_check("quality_gates", "FAIL", "QUALITY_GATES.md is missing or unreadable")
        return

    evidence: list[str] = []
    checks = {
        "unknown_error requires human review": ("unknown_error", "human review"),
        "provider or LLM boundary present": ("provider",),
        "active state mutation requires approval": ("active", "human"),
        "auto modification is forbidden or reviewed": ("auto", "modify"),
        "read-only or evidence-only boundary present": ("read-only",),
    }
    for label, terms in checks.items():
        if _contains_all(text, terms):
            evidence.append(label)
        else:
            state.warn(f"QUALITY_GATES.md may be missing: {label}")

    status = "PASS_WITH_NOTES" if any("QUALITY_GATES" in warning for warning in state.warnings) else "PASS"
    state.add_check("quality_gates", status, "quality gate sanity check complete", evidence)


def _check_risk_register(state: _AuditState) -> None:
    text = _read_text(state.target_repo, "docs/harness/RISK_REGISTER.md")
    if not text:
        state.add_check("risk_register", "FAIL", "RISK_REGISTER.md is missing or unreadable")
        return

    evidence: list[str] = []
    required_risk_terms = {
        "active risks exist": ("active",),
        "mitigated risks exist": ("mitigated",),
        "provider/LLM premature integration risk exists": ("provider",),
        "scope drift risk exists": ("scope drift",),
        "mutation/active state risk exists": ("mutation",),
    }
    for label, terms in required_risk_terms.items():
        if _contains_all(text, terms):
            evidence.append(label)
        else:
            state.warn(f"RISK_REGISTER.md may be missing: {label}")

    status = "PASS_WITH_NOTES" if any("RISK_REGISTER" in warning for warning in state.warnings) else "PASS"
    state.add_check("risk_register", status, "risk register sanity check complete", evidence)


def _check_closeout_reports(state: _AuditState) -> None:
    harness_dir = state.target_repo / "docs" / "harness"
    if not harness_dir.is_dir():
        state.add_check("closeout_reports", "FAIL", "docs/harness directory is missing")
        return

    reports = sorted(harness_dir.glob("*CLOSEOUT_REPORT.md"))
    if not reports:
        state.warn("No closeout reports found under docs/harness")
        state.add_check("closeout_reports", "WARN", "no closeout reports found")
        return

    evidence: list[str] = []
    for report in reports:
        text = report.read_text(encoding="utf-8", errors="replace")
        rel = report.relative_to(state.target_repo).as_posix()
        status_match = re.search(r"\*\*Status\*\*\s*:\s*([^\n]+)", text)
        test_match = re.search(r"\*\*Test count\*\*\s*:\s*([^\n]+)", text)
        sealed_match = re.search(r"\*\*Sealed baseline candidate\*\*\s*:\s*([^\n]+)", text)
        parts = [rel]
        if status_match:
            parts.append(f"status={status_match.group(1).strip()}")
        if test_match:
            parts.append(f"tests={test_match.group(1).strip()}")
        if sealed_match:
            parts.append(f"sealed_candidate={sealed_match.group(1).strip()}")
        evidence.append("; ".join(parts))

    state.add_check("closeout_reports", "PASS", "closeout reports detected", evidence)


def _find_malformed_markdown_table_rows(text: str) -> list[str]:
    suspicious: list[str] = []
    for idx, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if "|" not in line:
            continue
        if stripped.startswith("|"):
            continue
        if stripped.startswith("-") or stripped.startswith("#"):
            continue
        if re.search(r"\b[A-Z0-9]+-[A-Z0-9]+\b\s*\|", stripped):
            suspicious.append(f"line {idx}: {stripped[:80]}")
    return suspicious


def _finalize(state: _AuditState) -> InstanceAuditReport:
    if state.blockers:
        verdict = "BLOCKED"
    elif state.warnings:
        verdict = "PASS_WITH_NOTES"
    else:
        verdict = "PASS"

    return InstanceAuditReport(
        target_repo=str(state.target_repo),
        verdict=verdict,
        checks=state.checks,
        warnings=state.warnings,
        blockers=state.blockers,
        recommended_next_actions=_recommended_next_actions(verdict, state),
    )


def _recommended_next_actions(verdict: str, state: _AuditState) -> list[str]:
    if verdict == "BLOCKED":
        return [
            "Fix blockers before allowing execution slices to proceed.",
            "Do not treat blocked or ready-with-approval work as executable.",
            "Re-run the read-only instance audit after corrections.",
        ]

    actions = [
        "Keep using the target repository as a controlled harness instance.",
        "Do not allow the execution adapter to approve its own work.",
        "Use human approval before active state mutation, provider integration, sandbox execution, or main-branch push.",
    ]
    if state.warnings:
        actions.insert(0, "Review warnings and convert high-friction manual controls into machine-readable indexes.")
    return actions
