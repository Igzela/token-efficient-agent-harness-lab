"""Artifact Gate for Stage 2 — verify task artifacts deterministically."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .task_records import TaskRecordBundle
from .validators import validate_completion_record, validate_handoff_pack


@dataclass(frozen=True)
class ArtifactCheck:
    name: str
    passed: bool
    message: str


@dataclass(frozen=True)
class ArtifactGateResult:
    ok: bool
    checks: tuple[ArtifactCheck, ...]
    missing_artifacts: tuple[str, ...]
    schema_violations: tuple[str, ...]
    forbidden_violations: tuple[str, ...]


class ArtifactGate:
    """Verify artifact existence, schema, evidence refs, and file policy."""

    def evaluate(
        self,
        bundle: TaskRecordBundle,
        allowed_files: tuple[str, ...] | None = None,
        forbidden_files: tuple[str, ...] | None = None,
    ) -> ArtifactGateResult:
        checks: list[ArtifactCheck] = []
        missing: list[str] = []
        schema_violations: list[str] = []
        forbidden: list[str] = []

        self._check_completion_schema(bundle, checks, schema_violations)
        self._check_handoff_schema(bundle, checks, schema_violations)
        self._check_artifact_existence(bundle, checks, missing)
        self._check_evidence_refs(bundle, checks, missing)
        self._check_handoff_pack_ref(bundle, checks, missing)
        self._check_allowed_files(bundle, allowed_files, checks, missing)
        self._check_forbidden_files(bundle, forbidden_files, checks, forbidden)

        ok = all(c.passed for c in checks)
        return ArtifactGateResult(
            ok=ok,
            checks=tuple(checks),
            missing_artifacts=tuple(missing),
            schema_violations=tuple(schema_violations),
            forbidden_violations=tuple(forbidden),
        )

    def _check_completion_schema(
        self,
        bundle: TaskRecordBundle,
        checks: list[ArtifactCheck],
        violations: list[str],
    ) -> None:
        result = validate_completion_record(bundle.completion)
        if result.ok:
            checks.append(ArtifactCheck("completion_schema", True, "completion.json valid"))
        else:
            msg = "; ".join(result.errors)
            violations.append(msg)
            checks.append(ArtifactCheck("completion_schema", False, msg))

    def _check_handoff_schema(
        self,
        bundle: TaskRecordBundle,
        checks: list[ArtifactCheck],
        violations: list[str],
    ) -> None:
        result = validate_handoff_pack(bundle.handoff_pack)
        if result.ok:
            checks.append(ArtifactCheck("handoff_schema", True, "handoff_pack.json valid"))
        else:
            msg = "; ".join(result.errors)
            violations.append(msg)
            checks.append(ArtifactCheck("handoff_schema", False, msg))

    def _check_artifact_existence(
        self,
        bundle: TaskRecordBundle,
        checks: list[ArtifactCheck],
        missing: list[str],
    ) -> None:
        refs = bundle.completion.get("artifact_refs", [])
        if not isinstance(refs, list) or not refs:
            checks.append(ArtifactCheck("artifact_existence", True, "no artifact_refs to check"))
            return

        for ref in refs:
            path = ref.get("path", "") if isinstance(ref, dict) else ""
            if not path:
                missing.append("<empty path>")
                checks.append(ArtifactCheck("artifact_existence", False, "artifact_ref has no path"))
                continue
            full_path = bundle.task_dir / path
            if not full_path.exists():
                missing.append(path)
                checks.append(ArtifactCheck("artifact_existence", False, f"not found: {path}"))

        if all(c.passed for c in checks if c.name == "artifact_existence"):
            checks.append(ArtifactCheck("artifact_existence", True, "all artifacts exist"))

    def _check_evidence_refs(
        self,
        bundle: TaskRecordBundle,
        checks: list[ArtifactCheck],
        missing: list[str],
    ) -> None:
        refs = bundle.handoff_pack.get("evidence_refs", [])
        if not isinstance(refs, list) or not refs:
            checks.append(ArtifactCheck("evidence_refs", False, "evidence_refs empty or missing"))
            missing.append("evidence_refs")
            return

        all_ok = True
        for ref in refs:
            if not isinstance(ref, dict) or not ref.get("path"):
                all_ok = False
                missing.append("<invalid evidence_ref>")

        if all_ok:
            checks.append(ArtifactCheck("evidence_refs", True, "evidence_refs valid"))
        else:
            checks.append(ArtifactCheck("evidence_refs", False, "some evidence_refs invalid"))

    def _check_handoff_pack_ref(
        self,
        bundle: TaskRecordBundle,
        checks: list[ArtifactCheck],
        missing: list[str],
    ) -> None:
        ref = bundle.completion.get("handoff_pack_ref")
        if not ref:
            checks.append(ArtifactCheck("handoff_pack_ref", True, "no handoff_pack_ref to check"))
            return
        full_path = bundle.task_dir / ref
        if full_path.exists():
            checks.append(ArtifactCheck("handoff_pack_ref", True, f"handoff_pack_ref exists: {ref}"))
        else:
            missing.append(ref)
            checks.append(ArtifactCheck("handoff_pack_ref", False, f"handoff_pack_ref not found: {ref}"))

    def _check_allowed_files(
        self,
        bundle: TaskRecordBundle,
        allowed_files: tuple[str, ...] | None,
        checks: list[ArtifactCheck],
        missing: list[str],
    ) -> None:
        if allowed_files is None:
            checks.append(ArtifactCheck("allowed_files", True, "no allowed_files constraint"))
            return

        artifact_paths = []
        for ref in bundle.completion.get("artifact_refs", []):
            if isinstance(ref, dict) and ref.get("path"):
                artifact_paths.append(ref["path"])

        uncovered = [p for p in artifact_paths if not any(p.startswith(a) or a.startswith(p) for a in allowed_files)]
        if not uncovered:
            checks.append(ArtifactCheck("allowed_files", True, "all artifacts covered by allowed_files"))
        else:
            for p in uncovered:
                missing.append(f"not in allowed_files: {p}")
            checks.append(ArtifactCheck("allowed_files", False, f"{len(uncovered)} artifact(s) not in allowed_files"))

    def _check_forbidden_files(
        self,
        bundle: TaskRecordBundle,
        forbidden_files: tuple[str, ...] | None,
        checks: list[ArtifactCheck],
        forbidden: list[str],
    ) -> None:
        if forbidden_files is None:
            checks.append(ArtifactCheck("forbidden_files", True, "no forbidden_files constraint"))
            return

        violations = []
        for ref in bundle.completion.get("artifact_refs", []):
            if isinstance(ref, dict) and ref.get("path"):
                path = ref["path"]
                if any(path.startswith(f) or f.startswith(path) for f in forbidden_files):
                    violations.append(path)

        if not violations:
            checks.append(ArtifactCheck("forbidden_files", True, "no forbidden_files violations"))
        else:
            forbidden.extend(violations)
            checks.append(ArtifactCheck("forbidden_files", False, f"{len(violations)} forbidden violation(s)"))
