"""Review Convergence Protocol — pure models, budgets, and state transitions.

Canonical owner for:
  MAX_SUBSTANTIVE_REVIEW_ROUNDS
  MAX_AUTONOMOUS_REPAIR_BATCHES
  REVIEW_PROTOCOL_VERSION
  ReviewFinding / ReviewDecision normalization
  R1 / repair-batch / R2 transition rules

Persistence remains in state_manager (GitHub Issue comments).
Capsule/project_context only project fields from durable state; they never
decide severity, disposition, repair, Ready, or merge.
"""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import asdict, dataclass, field
from typing import Any

REVIEW_PROTOCOL_VERSION = "review-convergence.v1"

# Substantive independent-review rounds (R1 + R2). Not CI repair attempts.
MAX_SUBSTANTIVE_REVIEW_ROUNDS = 2
# One autonomous repair batch between R1 and R2. Not equal to round count.
MAX_AUTONOMOUS_REPAIR_BATCHES = 1
INITIAL_AUTONOMOUS_REPAIRS_REMAINING = MAX_AUTONOMOUS_REPAIR_BATCHES

REVIEW_MODES = frozenset({"full", "repair_verification"})
SEVERITIES = frozenset({"blocker", "major", "minor", "note"})
DISPOSITIONS = frozenset({"block_current_head", "defer", "decision_required"})
SCOPE_RELATIONS = frozenset({"in_packet", "out_of_packet"})
FINDING_STATUSES = frozenset({"open", "resolved", "deferred"})
ADMISSION_REASONS = frozenset(
    {"repair_regression", "prior_evidence_unavailable", "hard_stop_miss"}
)

# Control verdicts used by the repository orchestrator wire.
# PASS is the only merge-authorizing verdict (see MERGE_AUTHORIZING in state_manager).
# DECISION_REQUIRED is non-authorizing and stops autonomous advancement.
# PASS_WITH_NOTES remains schema-valid historical / non-authorizing.
CONTROL_VERDICTS = frozenset(
    {"PASS", "PASS_WITH_NOTES", "BLOCKED", "FAIL", "DECISION_REQUIRED", "INVALIDATED"}
)
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
FINDING_ID_RE = re.compile(r"^[A-Za-z0-9._:/-]{1,160}$")

MAX_FINDINGS = 200
MAX_DEFERRED_NOTES = 50
MAX_NOTE_LEN = 2000
MAX_SUMMARY_LEN = 2000


class ConvergenceError(ValueError):
    """Invalid finding, decision, or transition."""


@dataclass(frozen=True)
class ReviewFinding:
    id: str
    axis: str
    evidence: str
    severity: str
    disposition: str
    scope_relation: str
    origin_head: str
    acceptance_condition: str
    status: str
    admission_reason: str | None = None

    def to_dict(self) -> dict[str, Any]:
        data = asdict(self)
        if data.get("admission_reason") is None:
            data.pop("admission_reason", None)
        return data


@dataclass(frozen=True)
class ReviewDecision:
    """Normalized internal review decision (transport adapters map into this)."""

    verdict: str
    summary: str
    reviewed_base: str
    reviewed_head: str
    reviewed_range: str
    review_mode: str
    review_round: int
    findings: tuple[ReviewFinding, ...] = ()
    prior_reviewed_head: str = ""
    # True when the transport supplied a structured findings array; False for
    # legacy string-list adapters that must expand prior blockers implicitly.
    findings_structured: bool = False
    # Reviewer-observed gates (non-authoritative for CI).
    security_ok: bool = True
    rollback_ok: bool = True
    observed_ci_status: str = "unknown"

    @property
    def open_blocker_ids(self) -> tuple[str, ...]:
        return tuple(
            f.id
            for f in self.findings
            if f.disposition == "block_current_head" and f.status == "open"
        )

    @property
    def deferred_note_ids(self) -> tuple[str, ...]:
        return tuple(
            f.id
            for f in self.findings
            if f.disposition == "defer" or f.status == "deferred"
        )

    @property
    def decision_required_ids(self) -> tuple[str, ...]:
        return tuple(
            f.id
            for f in self.findings
            if f.disposition == "decision_required" and f.status == "open"
        )

    @property
    def finding_ledger_digest(self) -> str:
        return ledger_digest(self.findings)


def ledger_digest(findings: tuple[ReviewFinding, ...] | list[ReviewFinding]) -> str:
    rows = []
    for finding in sorted(findings, key=lambda f: f.id):
        rows.append(
            {
                "id": finding.id,
                "disposition": finding.disposition,
                "status": finding.status,
                "severity": finding.severity,
                "origin_head": finding.origin_head,
                "acceptance_condition": finding.acceptance_condition,
            }
        )
    raw = json.dumps(rows, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def normalize_finding(raw: dict[str, Any]) -> ReviewFinding:
    if not isinstance(raw, dict):
        raise ConvergenceError("finding must be an object")
    fid = raw.get("id")
    if not isinstance(fid, str) or not FINDING_ID_RE.fullmatch(fid):
        raise ConvergenceError("finding id is invalid")
    for key in (
        "axis",
        "evidence",
        "severity",
        "disposition",
        "scope_relation",
        "origin_head",
        "acceptance_condition",
        "status",
    ):
        if key not in raw:
            raise ConvergenceError(f"finding missing {key}")
    allowed_keys = {
        "id",
        "axis",
        "evidence",
        "severity",
        "disposition",
        "scope_relation",
        "origin_head",
        "acceptance_condition",
        "status",
        "admission_reason",
    }
    unknown = sorted(set(raw) - allowed_keys)
    if unknown:
        raise ConvergenceError(f"finding has unexpected keys: {unknown}")
    if raw["severity"] not in SEVERITIES:
        raise ConvergenceError("invalid severity")
    if raw["disposition"] not in DISPOSITIONS:
        raise ConvergenceError("invalid disposition")
    if raw["scope_relation"] not in SCOPE_RELATIONS:
        raise ConvergenceError("invalid scope_relation")
    if raw["status"] not in FINDING_STATUSES:
        raise ConvergenceError("invalid status")
    admission = raw.get("admission_reason")
    if admission is not None and admission not in ADMISSION_REASONS:
        raise ConvergenceError("invalid admission_reason")
    origin = raw["origin_head"]
    if not isinstance(origin, str) or not origin:
        raise ConvergenceError("origin_head is invalid")
    return ReviewFinding(
        id=fid,
        axis=str(raw["axis"])[:200],
        evidence=str(raw["evidence"])[:MAX_NOTE_LEN * 6],
        severity=raw["severity"],
        disposition=raw["disposition"],
        scope_relation=raw["scope_relation"],
        origin_head=origin,
        acceptance_condition=str(raw["acceptance_condition"])[:MAX_NOTE_LEN * 2],
        status=raw["status"],
        admission_reason=admission,
    )


def findings_from_legacy_lists(
    *,
    blockers: list[str] | None,
    major_notes: list[str] | None,
    minor_notes: list[str] | None,
    origin_head: str,
) -> tuple[ReviewFinding, ...]:
    """Map historical string blockers/notes into structured findings."""
    findings: list[ReviewFinding] = []
    for index, text in enumerate(blockers or []):
        if not isinstance(text, str) or not text.strip():
            raise ConvergenceError("blocker text is invalid")
        findings.append(
            ReviewFinding(
                id=f"blocker-{index + 1}",
                axis="legacy",
                evidence=text[:MAX_NOTE_LEN],
                severity="blocker",
                disposition="block_current_head",
                scope_relation="in_packet",
                origin_head=origin_head,
                acceptance_condition=text[:MAX_NOTE_LEN],
                status="open",
            )
        )
    for index, text in enumerate(major_notes or []):
        findings.append(
            ReviewFinding(
                id=f"major-{index + 1}",
                axis="legacy",
                evidence=str(text)[:MAX_NOTE_LEN],
                severity="major",
                disposition="defer",
                scope_relation="in_packet",
                origin_head=origin_head,
                acceptance_condition="deferred residual risk",
                status="deferred",
            )
        )
    for index, text in enumerate(minor_notes or []):
        findings.append(
            ReviewFinding(
                id=f"minor-{index + 1}",
                axis="legacy",
                evidence=str(text)[:MAX_NOTE_LEN],
                severity="minor",
                disposition="defer",
                scope_relation="in_packet",
                origin_head=origin_head,
                acceptance_condition="deferred residual risk",
                status="deferred",
            )
        )
    if len(findings) > MAX_FINDINGS:
        raise ConvergenceError("too many findings")
    return tuple(findings)


def decision_from_legacy_artifact(
    artifact: dict[str, Any],
    *,
    review_mode: str = "full",
    review_round: int = 1,
    base_sha: str = "",
    prior_reviewed_head: str = "",
) -> ReviewDecision:
    """Map repository review_schema artifact (legacy or extended) to ReviewDecision."""
    if not isinstance(artifact, dict):
        raise ConvergenceError("artifact must be an object")
    verdict = artifact.get("verdict")
    if verdict not in CONTROL_VERDICTS - {"INVALIDATED"}:
        raise ConvergenceError(f"unsupported verdict {verdict!r}")
    head = artifact.get("reviewed_head_sha") or artifact.get("reviewed_head")
    if not isinstance(head, str) or not HEX40.fullmatch(head):
        raise ConvergenceError("reviewed head is invalid")
    summary = artifact.get("summary")
    if not isinstance(summary, str) or not (1 <= len(summary) <= MAX_SUMMARY_LEN):
        raise ConvergenceError("summary is invalid")
    if "findings" in artifact and artifact["findings"] is not None:
        findings = tuple(normalize_finding(item) for item in artifact["findings"])
        structured = True
    else:
        findings = findings_from_legacy_lists(
            blockers=artifact.get("blockers") or [],
            major_notes=artifact.get("major_notes") or [],
            minor_notes=artifact.get("minor_notes") or [],
            origin_head=head,
        )
        structured = False
    mode = artifact.get("review_mode") or review_mode
    if mode not in REVIEW_MODES:
        raise ConvergenceError("review_mode is invalid")
    base = artifact.get("reviewed_base") or base_sha or ("0" * 40)
    if base and not HEX40.fullmatch(str(base)):
        # Allow empty/unknown base only when not supplied as hex.
        if base_sha and HEX40.fullmatch(base_sha):
            base = base_sha
        else:
            base = "0" * 40
    reviewed_range = artifact.get("reviewed_range") or f"{base}...{head}"
    security_ok = artifact.get("security_ok", True)
    rollback_ok = artifact.get("rollback_ok", True)
    if type(security_ok) is not bool or type(rollback_ok) is not bool:
        raise ConvergenceError("security_ok/rollback_ok must be booleans")
    # Model-authored ci_green is observation only; never authoritative.
    if "observed_ci_status" in artifact:
        observed = str(artifact.get("observed_ci_status") or "unknown")
    elif "ci_green" in artifact:
        observed = "model_reported_green" if artifact.get("ci_green") is True else "model_reported_not_green"
    else:
        observed = "unknown"
    decision = ReviewDecision(
        verdict=verdict,
        summary=summary,
        reviewed_base=str(base),
        reviewed_head=head,
        reviewed_range=str(reviewed_range),
        review_mode=mode,
        review_round=int(artifact.get("review_round") or review_round),
        findings=findings,
        prior_reviewed_head=str(
            artifact.get("prior_reviewed_head") or prior_reviewed_head or ""
        ),
        findings_structured=structured,
        security_ok=security_ok,
        rollback_ok=rollback_ok,
        observed_ci_status=observed,
    )
    validate_decision_cross_fields(decision)
    return decision


def validate_decision_cross_fields(decision: ReviewDecision) -> None:
    if decision.review_mode not in REVIEW_MODES:
        raise ConvergenceError("invalid review_mode")
    if decision.verdict != "INVALIDATED" and not (
        1 <= decision.review_round <= MAX_SUBSTANTIVE_REVIEW_ROUNDS
    ):
        raise ConvergenceError(
            "review_round must be within the substantive review round budget"
        )
    open_blockers = decision.open_blocker_ids
    open_decisions = decision.decision_required_ids
    if decision.verdict == "PASS":
        if open_blockers:
            raise ConvergenceError("PASS cannot have open blockers")
        if open_decisions:
            raise ConvergenceError("PASS cannot have open decision_required")
        if not decision.security_ok or not decision.rollback_ok:
            raise ConvergenceError("PASS requires security_ok and rollback_ok")
        # ci_green is intentionally NOT required here — trusted CI is external.
    elif decision.verdict == "PASS_WITH_NOTES":
        if open_blockers:
            raise ConvergenceError("PASS_WITH_NOTES cannot have open blockers")
    elif decision.verdict == "BLOCKED":
        if not open_blockers:
            raise ConvergenceError("BLOCKED requires open blockers")
    elif decision.verdict == "DECISION_REQUIRED":
        r2_blocked = (
            decision.review_mode == "repair_verification" and bool(open_blockers)
        )
        if not open_decisions and not r2_blocked:
            raise ConvergenceError(
                "DECISION_REQUIRED requires open decision_required or R2 open blockers"
            )
    if len(decision.deferred_note_ids) > MAX_DEFERRED_NOTES:
        raise ConvergenceError("too many deferred notes")


@dataclass(frozen=True)
class ReviewRoundState:
    """Bounded durable convergence fields for ReviewState v3."""

    review_protocol_version: str
    review_mode: str
    review_round: int
    prior_reviewed_head: str
    reviewed_base: str
    reviewed_head: str
    reviewed_range: str
    verdict: str
    findings: tuple[dict[str, Any], ...]
    finding_ledger_digest: str
    open_blocker_ids: tuple[str, ...]
    deferred_note_ids: tuple[str, ...]
    decision_required_ids: tuple[str, ...]
    autonomous_repairs_remaining: int
    stop_reason: str
    artifact_sha256: str = ""
    review_workflow_run_id: int | None = None
    summary: str = ""
    # Legacy projection for older readers.
    blockers: tuple[str, ...] = ()
    major_notes: tuple[str, ...] = ()
    minor_notes: tuple[str, ...] = ()

    def to_persistence_fields(self) -> dict[str, Any]:
        return {
            "review_protocol_version": self.review_protocol_version,
            "review_mode": self.review_mode,
            "review_round": self.review_round,
            "prior_reviewed_head": self.prior_reviewed_head,
            "base_sha": self.reviewed_base,
            "head_sha": self.reviewed_head,
            "reviewed_range": self.reviewed_range,
            "verdict": self.verdict,
            "findings": list(self.findings),
            "finding_ledger_digest": self.finding_ledger_digest,
            "open_blocker_ids": list(self.open_blocker_ids),
            "deferred_note_ids": list(self.deferred_note_ids),
            "decision_required_ids": list(self.decision_required_ids),
            "autonomous_repairs_remaining": self.autonomous_repairs_remaining,
            "stop_reason": self.stop_reason,
            "artifact_sha256": self.artifact_sha256,
            "review_workflow_run_id": self.review_workflow_run_id,
            "summary": self.summary,
            "blockers": list(self.blockers),
            "major_notes": list(self.major_notes),
            "minor_notes": list(self.minor_notes),
        }


def initial_r1_state(
    decision: ReviewDecision,
    *,
    artifact_sha256: str = "",
    review_workflow_run_id: int | None = None,
) -> ReviewRoundState:
    """R1: full mode, round 1, repairs remaining based on verdict."""
    if decision.review_mode != "full":
        raise ConvergenceError("R1 requires review_mode=full")
    open_blockers = decision.open_blocker_ids
    if decision.verdict == "PASS":
        remaining = INITIAL_AUTONOMOUS_REPAIRS_REMAINING
        stop = ""
    elif decision.verdict in {"BLOCKED", "FAIL"} and open_blockers:
        remaining = INITIAL_AUTONOMOUS_REPAIRS_REMAINING  # one batch available
        stop = ""
    elif decision.verdict == "DECISION_REQUIRED":
        remaining = 0
        stop = "decision_required"
    else:
        remaining = 0
        stop = "review_blocked"
    return _state_from_decision(
        decision,
        review_round=1,
        autonomous_repairs_remaining=remaining,
        stop_reason=stop,
        artifact_sha256=artifact_sha256,
        review_workflow_run_id=review_workflow_run_id,
    )


def after_repair_batch_consumed(
    prior: ReviewRoundState,
    *,
    new_head_sha: str,
) -> ReviewRoundState:
    """After the single repair batch is pushed: remaining=0, preserve ledger identity."""
    if prior.autonomous_repairs_remaining <= 0:
        raise ConvergenceError("no autonomous repair batches remaining")
    if prior.review_round >= MAX_SUBSTANTIVE_REVIEW_ROUNDS:
        raise ConvergenceError("cannot repair after final substantive review round")
    # Invalidate old head evidence; next review is R2 on new_head.
    return ReviewRoundState(
        review_protocol_version=REVIEW_PROTOCOL_VERSION,
        review_mode="repair_verification",
        review_round=prior.review_round + 1,
        prior_reviewed_head=prior.reviewed_head,
        reviewed_base=prior.reviewed_base,
        reviewed_head=new_head_sha,
        reviewed_range=f"{prior.reviewed_base}...{new_head_sha}",
        verdict="INVALIDATED",
        findings=prior.findings,  # preserve prior ledger identity for R2
        finding_ledger_digest=prior.finding_ledger_digest,
        open_blocker_ids=prior.open_blocker_ids,
        deferred_note_ids=prior.deferred_note_ids,
        decision_required_ids=prior.decision_required_ids,
        autonomous_repairs_remaining=0,
        stop_reason="awaiting_r2",
        artifact_sha256="",
        review_workflow_run_id=None,
        summary=f"prior evidence for head {prior.reviewed_head} invalidated by repair head {new_head_sha}",
        blockers=prior.blockers,
        major_notes=prior.major_notes,
        minor_notes=prior.minor_notes,
    )


def apply_r2_decision(
    prior: ReviewRoundState,
    decision: ReviewDecision,
    *,
    artifact_sha256: str = "",
    review_workflow_run_id: int | None = None,
) -> ReviewRoundState:
    """R2: repair_verification; PASS or DECISION_REQUIRED (no R3)."""
    if decision.review_mode != "repair_verification":
        raise ConvergenceError("R2 requires review_mode=repair_verification")
    open_blockers = decision.open_blocker_ids
    open_decisions = decision.decision_required_ids
    if decision.verdict == "PASS" and not open_blockers and not open_decisions:
        stop = ""
        verdict = "PASS"
    elif open_decisions or open_blockers or decision.verdict in {"BLOCKED", "FAIL", "DECISION_REQUIRED"}:
        stop = "decision_required"
        verdict = "DECISION_REQUIRED"
    else:
        stop = "review_blocked"
        verdict = decision.verdict if decision.verdict in CONTROL_VERDICTS else "DECISION_REQUIRED"
    # Validate prior blockers are not silently dropped. Legacy string-list
    # adapters (no structured findings) implicitly resolve prior blockers on
    # PASS; structured artifacts must carry every prior blocker id explicitly.
    prior_open = set(prior.open_blocker_ids)
    current_map = {f.id: f for f in decision.findings}
    missing_prior = prior_open - set(current_map)
    merged_findings = list(decision.findings)
    if missing_prior and decision.findings_structured:
        raise ConvergenceError(
            f"prior blocker {sorted(missing_prior)[0]} silently disappeared at R2"
        )
    if missing_prior and decision.verdict == "PASS":
        for fid in sorted(missing_prior):
            merged_findings.append(
                ReviewFinding(
                    id=fid,
                    axis="legacy",
                    evidence="resolved at R2 (legacy adapter)",
                    severity="blocker",
                    disposition="block_current_head",
                    scope_relation="in_packet",
                    origin_head=prior.reviewed_head or decision.reviewed_head,
                    acceptance_condition="resolved",
                    status="resolved",
                )
            )
        for fid in prior.deferred_note_ids:
            if fid in {f.id for f in merged_findings}:
                continue
            merged_findings.append(
                ReviewFinding(
                    id=fid,
                    axis="legacy",
                    evidence="deferred note carried from R1",
                    severity="note",
                    disposition="defer",
                    scope_relation="in_packet",
                    origin_head=prior.reviewed_head or decision.reviewed_head,
                    acceptance_condition="deferred residual risk",
                    status="deferred",
                )
            )
        current_map = {f.id: f for f in merged_findings}
        decision = ReviewDecision(
            verdict=decision.verdict,
            summary=decision.summary,
            reviewed_base=decision.reviewed_base,
            reviewed_head=decision.reviewed_head,
            reviewed_range=decision.reviewed_range,
            review_mode=decision.review_mode,
            review_round=decision.review_round,
            findings=tuple(merged_findings),
            prior_reviewed_head=decision.prior_reviewed_head,
            findings_structured=decision.findings_structured,
            security_ok=decision.security_ok,
            rollback_ok=decision.rollback_ok,
            observed_ci_status=decision.observed_ci_status,
        )
    for fid in prior_open:
        if fid not in current_map:
            raise ConvergenceError(f"prior blocker {fid} silently disappeared at R2")
        if current_map[fid].status not in {"resolved", "open"}:
            raise ConvergenceError(f"prior blocker {fid} has invalid R2 status")
    for fid, finding in current_map.items():
        if (
            finding.disposition == "block_current_head"
            and finding.status == "open"
            and fid not in prior_open
            and finding.admission_reason not in ADMISSION_REASONS
        ):
            raise ConvergenceError(
                f"R2 new blocker {fid} requires admission_reason"
            )
    prior_head = (
        prior.prior_reviewed_head
        if prior.verdict == "INVALIDATED" and prior.prior_reviewed_head
        else prior.reviewed_head
    )
    return _state_from_decision(
        decision,
        review_round=min(MAX_SUBSTANTIVE_REVIEW_ROUNDS, max(2, decision.review_round)),
        autonomous_repairs_remaining=0,
        stop_reason=stop,
        artifact_sha256=artifact_sha256,
        review_workflow_run_id=review_workflow_run_id,
        force_verdict=verdict,
        prior_reviewed_head=prior_head,
    )


def derive_next_review_attempt(state: dict[str, Any] | None, head_sha: str) -> dict[str, Any]:
    """Derive mode/round/ledger for the next review dispatch (including retry-review)."""
    if not state:
        return {
            "review_mode": "full",
            "review_round": 1,
            "prior_reviewed_head": "",
            "autonomous_repairs_remaining": INITIAL_AUTONOMOUS_REPAIRS_REMAINING,
            "open_blocker_ids": [],
            "finding_ledger_digest": "",
            "allowed": True,
            "deny_reason": "",
        }
    stop = str(state.get("stop_reason") or "")
    verdict = str(state.get("verdict") or "")
    if stop == "decision_required" or verdict == "DECISION_REQUIRED":
        return {
            "review_mode": state.get("review_mode") or "repair_verification",
            "review_round": int(state.get("review_round") or 2),
            "prior_reviewed_head": state.get("prior_reviewed_head") or "",
            "autonomous_repairs_remaining": 0,
            "open_blocker_ids": list(state.get("open_blocker_ids") or []),
            "finding_ledger_digest": state.get("finding_ledger_digest") or "",
            "allowed": False,
            "deny_reason": "decision_required_requires_human_authority",
        }
    round_n = int(state.get("review_round") or 1)
    remaining = int(
        state.get("autonomous_repairs_remaining", INITIAL_AUTONOMOUS_REPAIRS_REMAINING)
    )
    prior_head = str(state.get("head_sha") or state.get("reviewed_head") or "")
    prior_open = list(state.get("open_blocker_ids") or [])
    prior_digest = str(state.get("finding_ledger_digest") or "")
    head_changed = bool(prior_head and prior_head != head_sha)
    if verdict == "INVALIDATED" and round_n <= 1 and remaining > 0:
        # Fresh invalidation (prior non-blocked review, new head): restart R1.
        return {
            "review_mode": "full",
            "review_round": 1,
            "prior_reviewed_head": prior_head or "",
            "autonomous_repairs_remaining": INITIAL_AUTONOMOUS_REPAIRS_REMAINING,
            "open_blocker_ids": [],
            "finding_ledger_digest": "",
            "allowed": True,
            "deny_reason": "",
        }
    if verdict == "INVALIDATED" or (head_changed and remaining == 0 and verdict != "PASS"):
        # Post-repair / budget-spent marker: R2 repair verification.
        if round_n > MAX_SUBSTANTIVE_REVIEW_ROUNDS:
            return {
                "review_mode": "repair_verification",
                "review_round": round_n,
                "prior_reviewed_head": state.get("prior_reviewed_head") or prior_head,
                "autonomous_repairs_remaining": 0,
                "open_blocker_ids": prior_open,
                "finding_ledger_digest": prior_digest,
                "allowed": False,
                "deny_reason": "substantive_review_budget_exhausted",
            }
        return {
            "review_mode": "repair_verification",
            "review_round": min(MAX_SUBSTANTIVE_REVIEW_ROUNDS, max(2, round_n)),
            "prior_reviewed_head": state.get("prior_reviewed_head") or prior_head,
            "autonomous_repairs_remaining": 0,
            "open_blocker_ids": prior_open,
            "finding_ledger_digest": prior_digest,
            "allowed": True,
            "deny_reason": "",
        }
    if head_changed and prior_open and round_n < MAX_SUBSTANTIVE_REVIEW_ROUNDS and remaining > 0:
        # Review-repair head (no explicit invalidation ran): the changed head
        # with open prior blockers consumes the single autonomous repair batch
        # and routes the next review to R2 repair verification.
        return {
            "review_mode": "repair_verification",
            "review_round": MAX_SUBSTANTIVE_REVIEW_ROUNDS,
            "prior_reviewed_head": prior_head,
            "autonomous_repairs_remaining": 0,
            "open_blocker_ids": prior_open,
            "finding_ledger_digest": prior_digest,
            "allowed": True,
            "deny_reason": "",
        }
    if head_changed and verdict == "PASS":
        # New head after a terminal PASS starts a fresh review surface.
        return {
            "review_mode": "full",
            "review_round": 1,
            "prior_reviewed_head": prior_head or "",
            "autonomous_repairs_remaining": INITIAL_AUTONOMOUS_REPAIRS_REMAINING,
            "open_blocker_ids": [],
            "finding_ledger_digest": "",
            "allowed": True,
            "deny_reason": "",
        }
    # Same head retry after BLOCKED without consuming repair — still R1-like if round 1
    # but retry-review must not create infinite R1 loops past budget.
    if round_n >= MAX_SUBSTANTIVE_REVIEW_ROUNDS and verdict not in {"PASS", "INVALIDATED"}:
        return {
            "review_mode": "repair_verification",
            "review_round": round_n,
            "prior_reviewed_head": state.get("prior_reviewed_head") or "",
            "autonomous_repairs_remaining": 0,
            "open_blocker_ids": prior_open,
            "finding_ledger_digest": prior_digest,
            "allowed": False,
            "deny_reason": "r2_complete_requires_human_authority_for_retry",
        }
    if not head_changed and verdict == "PASS":
        # A PASSed head cannot be automatically re-reviewed; operator action
        # and explicit authority are required to reopen it.
        return {
            "review_mode": state.get("review_mode") or "full",
            "review_round": round_n,
            "prior_reviewed_head": state.get("prior_reviewed_head") or "",
            "autonomous_repairs_remaining": remaining,
            "open_blocker_ids": prior_open,
            "finding_ledger_digest": prior_digest,
            "allowed": False,
            "deny_reason": "pass_terminal_requires_human_authority",
        }
    return {
        "review_mode": state.get("review_mode") or "full",
        "review_round": round_n,
        "prior_reviewed_head": state.get("prior_reviewed_head") or "",
        "autonomous_repairs_remaining": remaining,
        "open_blocker_ids": prior_open,
        "finding_ledger_digest": prior_digest,
        "allowed": True,
        "deny_reason": "",
    }


def _state_from_decision(
    decision: ReviewDecision,
    *,
    review_round: int,
    autonomous_repairs_remaining: int,
    stop_reason: str,
    artifact_sha256: str = "",
    review_workflow_run_id: int | None = None,
    force_verdict: str | None = None,
    prior_reviewed_head: str | None = None,
) -> ReviewRoundState:
    findings = tuple(f.to_dict() for f in decision.findings)
    blockers = tuple(
        f.evidence for f in decision.findings
        if f.disposition == "block_current_head" and f.status == "open"
    )
    major_notes = tuple(
        f.evidence for f in decision.findings
        if f.severity == "major" and (f.disposition == "defer" or f.status == "deferred")
    )
    minor_notes = tuple(
        f.evidence for f in decision.findings
        if f.severity in {"minor", "note"}
        and (f.disposition == "defer" or f.status == "deferred")
    )
    return ReviewRoundState(
        review_protocol_version=REVIEW_PROTOCOL_VERSION,
        review_mode=decision.review_mode,
        review_round=review_round,
        prior_reviewed_head=(
            prior_reviewed_head
            if prior_reviewed_head is not None
            else decision.prior_reviewed_head
        ),
        reviewed_base=decision.reviewed_base,
        reviewed_head=decision.reviewed_head,
        reviewed_range=decision.reviewed_range,
        verdict=force_verdict or decision.verdict,
        findings=findings,
        finding_ledger_digest=decision.finding_ledger_digest,
        open_blocker_ids=decision.open_blocker_ids,
        deferred_note_ids=decision.deferred_note_ids,
        decision_required_ids=decision.decision_required_ids,
        autonomous_repairs_remaining=autonomous_repairs_remaining,
        stop_reason=stop_reason,
        artifact_sha256=artifact_sha256,
        review_workflow_run_id=review_workflow_run_id,
        summary=decision.summary,
        blockers=blockers,
        major_notes=major_notes,
        minor_notes=minor_notes,
    )


def project_capsule_fields(state: dict[str, Any] | None, *, expected_head: str | None) -> dict[str, Any]:
    """Bounded non-authoritative capsule projection. Never decides severity/repair."""
    unavailable = {
        "review_protocol_version": None,
        "review_mode": None,
        "review_round": None,
        "prior_reviewed_head": None,
        "reviewed_head": None,
        "finding_ledger_digest": None,
        "open_blocker_ids": [],
        "deferred_note_ids": [],
        "autonomous_repairs_remaining": None,
        "stop_reason": None,
        "review_state": "unavailable",
        "availability": "unavailable",
    }
    if not state or not isinstance(state, dict):
        return unavailable
    version = state.get("version")
    if version not in (2, 3):
        out = dict(unavailable)
        out["review_state"] = "conflict"
        out["availability"] = "conflict"
        out["unavailable_reason"] = "unsupported_review_state_version"
        return out
    head = state.get("head_sha") or state.get("reviewed_head")
    if expected_head and head and head != expected_head:
        out = dict(unavailable)
        out["review_state"] = "conflict"
        out["availability"] = "conflict"
        out["unavailable_reason"] = "review_state_head_mismatch"
        out["reviewed_head"] = head
        return out
    if version == 2:
        # Legacy read-only projection: no convergence fields.
        return {
            "review_protocol_version": None,
            "review_mode": None,
            "review_round": None,
            "prior_reviewed_head": None,
            "reviewed_head": head,
            "finding_ledger_digest": None,
            "open_blocker_ids": [],
            "deferred_note_ids": [],
            "autonomous_repairs_remaining": None,
            "stop_reason": None,
            "review_state": str(state.get("verdict") or "legacy_v2"),
            "availability": "legacy",
            "verdict": state.get("verdict"),
        }
    return {
        "review_protocol_version": state.get("review_protocol_version") or REVIEW_PROTOCOL_VERSION,
        "review_mode": state.get("review_mode"),
        "review_round": state.get("review_round"),
        "prior_reviewed_head": state.get("prior_reviewed_head") or "",
        "reviewed_head": head,
        "finding_ledger_digest": state.get("finding_ledger_digest") or "",
        "open_blocker_ids": list(state.get("open_blocker_ids") or [])[:MAX_FINDINGS],
        "deferred_note_ids": list(state.get("deferred_note_ids") or [])[:MAX_DEFERRED_NOTES],
        "autonomous_repairs_remaining": state.get("autonomous_repairs_remaining"),
        "stop_reason": state.get("stop_reason") or "",
        "review_state": str(state.get("verdict") or "unknown"),
        "availability": "confirmed",
        "verdict": state.get("verdict"),
    }
