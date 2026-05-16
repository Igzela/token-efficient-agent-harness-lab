"""Skill Extractor for Stage 3 — deterministic skill extraction from logs and advisor records."""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .advisor import AdvisorResponse
from .task_records import TaskRecordBundle


@dataclass(frozen=True)
class SkillRecord:
    skill_id: str
    source_task_id: str
    skill_type: str  # fix_pattern | approach | config_template | test_pattern
    title: str
    description: str
    applicable_when: str
    evidence_refs: tuple[str, ...]
    confidence: float
    extracted_from: str  # "run_log" | "retrospective" | "advisor"


@dataclass(frozen=True)
class SkillLibrary:
    skills: tuple[SkillRecord, ...]


def _skill_id_from_content(content: str) -> str:
    h = hashlib.sha256(content.encode("utf-8")).hexdigest()[:12]
    return f"skill_{h}"


_RUN_LOG_PATTERNS = [
    (r"(?:Fixed by|Fix)[:\s]+(.+)", "fix_pattern"),
    (r"(?:Root cause)[:\s]+(.+)", "fix_pattern"),
    (r"(?:Approach)[:\s]+(.+)", "approach"),
    (r"(?:Lesson learned)[:\s]+(.+)", "approach"),
]

_RETROSPECTIVE_PATTERNS = [
    (r"(?:What worked)[:\s]+(.+)", "approach"),
    (r"(?:What didn't(?:'t| work))[:\s]+(.+)", "fix_pattern"),
    (r"(?:Lesson learned)[:\s]+(.+)", "approach"),
]


class SkillExtractor:
    """Extract reusable skills from task bundles and advisor responses."""

    def extract_from_bundle(
        self, bundle: TaskRecordBundle
    ) -> tuple[SkillRecord, ...]:
        skills: list[SkillRecord] = []
        task_id = bundle.task_spec.get("task_id", "unknown")

        if bundle.run_log_text:
            skills.extend(self._extract_from_run_log(bundle.run_log_text, task_id))

        retrospective = self._load_retrospective(bundle.task_dir)
        if retrospective:
            skills.extend(
                self._extract_from_retrospective(retrospective, task_id)
            )

        completion_skills = self._extract_from_completion(bundle.completion, task_id)
        skills.extend(completion_skills)

        return tuple(skills)

    def extract_from_advisor(
        self, response: AdvisorResponse, task_id: str
    ) -> tuple[SkillRecord, ...]:
        skills: list[SkillRecord] = []
        if response.recommended_action:
            content = f"recommended_action: {response.recommended_action}"
            skills.append(
                SkillRecord(
                    skill_id=_skill_id_from_content(content),
                    source_task_id=task_id,
                    skill_type="approach",
                    title=f"Advisor recommendation for {response.call_type}",
                    description=response.recommended_action,
                    applicable_when=f"when {response.call_type} is needed",
                    evidence_refs=(f"advisor:{response.call_type}",),
                    confidence=response.confidence,
                    extracted_from="advisor",
                )
            )
        if response.do_not_do:
            content = f"do_not_do: {response.do_not_do}"
            skills.append(
                SkillRecord(
                    skill_id=_skill_id_from_content(content),
                    source_task_id=task_id,
                    skill_type="fix_pattern",
                    title=f"Advisor warning for {response.call_type}",
                    description=response.do_not_do,
                    applicable_when=f"avoid during {response.call_type}",
                    evidence_refs=(f"advisor:{response.call_type}",),
                    confidence=response.confidence,
                    extracted_from="advisor",
                )
            )
        return tuple(skills)

    def _extract_from_run_log(
        self, text: str, task_id: str
    ) -> tuple[SkillRecord, ...]:
        skills: list[SkillRecord] = []
        for pattern, skill_type in _RUN_LOG_PATTERNS:
            match = re.search(pattern, text, re.IGNORECASE | re.MULTILINE)
            if match:
                description = match.group(1).strip()
                content = f"{skill_type}:{description}"
                skills.append(
                    SkillRecord(
                        skill_id=_skill_id_from_content(content),
                        source_task_id=task_id,
                        skill_type=skill_type,
                        title=f"Run log insight: {skill_type}",
                        description=description,
                        applicable_when="when similar task is encountered",
                        evidence_refs=("run_log",),
                        confidence=0.7,
                        extracted_from="run_log",
                    )
                )
        return tuple(skills)

    def _extract_from_retrospective(
        self, text: str, task_id: str
    ) -> tuple[SkillRecord, ...]:
        skills: list[SkillRecord] = []
        for pattern, skill_type in _RETROSPECTIVE_PATTERNS:
            match = re.search(pattern, text, re.IGNORECASE | re.MULTILINE)
            if match:
                description = match.group(1).strip()
                content = f"retro:{skill_type}:{description}"
                skills.append(
                    SkillRecord(
                        skill_id=_skill_id_from_content(content),
                        source_task_id=task_id,
                        skill_type=skill_type,
                        title=f"Retrospective insight: {skill_type}",
                        description=description,
                        applicable_when="when similar project pattern is encountered",
                        evidence_refs=("retrospective",),
                        confidence=0.75,
                        extracted_from="retrospective",
                    )
                )
        return tuple(skills)

    def _extract_from_completion(
        self, completion: dict[str, Any], task_id: str
    ) -> tuple[SkillRecord, ...]:
        skills: list[SkillRecord] = []
        failure_code = completion.get("failure_code")
        if failure_code:
            content = f"failure:{failure_code}"
            skills.append(
                SkillRecord(
                    skill_id=_skill_id_from_content(content),
                    source_task_id=task_id,
                    skill_type="fix_pattern",
                    title=f"Failure code pattern: {failure_code}",
                    description=f"Task encountered failure code {failure_code}",
                    applicable_when=f"when {failure_code} occurs",
                    evidence_refs=("completion",),
                    confidence=0.6,
                    extracted_from="completion",
                )
            )
        return tuple(skills)

    def _load_retrospective(self, task_dir: Path) -> str | None:
        retro_path = task_dir / "retrospective.md"
        if retro_path.exists():
            return retro_path.read_text(encoding="utf-8")
        return None


class SkillStore:
    """Persist and retrieve skill records as JSON files."""

    def __init__(self, store_dir: Path):
        self._store_dir = store_dir
        self._store_dir.mkdir(parents=True, exist_ok=True)

    def save(self, skill: SkillRecord) -> None:
        path = self._store_dir / f"{skill.skill_id}.json"
        path.write_text(
            json.dumps(
                {
                    "skill_id": skill.skill_id,
                    "source_task_id": skill.source_task_id,
                    "skill_type": skill.skill_type,
                    "title": skill.title,
                    "description": skill.description,
                    "applicable_when": skill.applicable_when,
                    "evidence_refs": skill.evidence_refs,
                    "confidence": skill.confidence,
                    "extracted_from": skill.extracted_from,
                },
                indent=2,
                sort_keys=True,
            ),
            encoding="utf-8",
        )

    def load(self, skill_id: str) -> SkillRecord | None:
        path = self._store_dir / f"{skill_id}.json"
        if not path.exists():
            return None
        data = json.loads(path.read_text(encoding="utf-8"))
        return SkillRecord(
            skill_id=data["skill_id"],
            source_task_id=data["source_task_id"],
            skill_type=data["skill_type"],
            title=data["title"],
            description=data["description"],
            applicable_when=data["applicable_when"],
            evidence_refs=tuple(data["evidence_refs"]),
            confidence=data["confidence"],
            extracted_from=data["extracted_from"],
        )

    def list_skills(self) -> tuple[SkillRecord, ...]:
        skills: list[SkillRecord] = []
        for path in sorted(self._store_dir.glob("skill_*.json")):
            data = json.loads(path.read_text(encoding="utf-8"))
            skills.append(
                SkillRecord(
                    skill_id=data["skill_id"],
                    source_task_id=data["source_task_id"],
                    skill_type=data["skill_type"],
                    title=data["title"],
                    description=data["description"],
                    applicable_when=data["applicable_when"],
                    evidence_refs=tuple(data["evidence_refs"]),
                    confidence=data["confidence"],
                    extracted_from=data["extracted_from"],
                )
            )
        return tuple(skills)

    def search(self, query: str) -> tuple[SkillRecord, ...]:
        all_skills = self.list_skills()
        query_lower = query.lower()
        return tuple(
            s
            for s in all_skills
            if query_lower in s.title.lower()
            or query_lower in s.description.lower()
            or query_lower in s.skill_type.lower()
        )
