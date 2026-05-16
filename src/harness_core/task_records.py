"""Task Record loading and validation for Stage 1 Week 4."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from json import JSONDecodeError
from pathlib import Path
from typing import Any

from .validators import (
    validate_completion_record,
    validate_handoff_pack,
    validate_replay_preflight_check,
)

REQUIRED_TASK_RECORD_FILES = (
    "task_spec.json",
    "completion.json",
    "handoff_pack.json",
    "events.jsonl",
)


@dataclass(frozen=True)
class TaskRecordBundle:
    task_dir: Path
    task_spec: dict[str, Any]
    completion: dict[str, Any]
    handoff_pack: dict[str, Any]
    events_path: Path
    run_log_path: Path | None = None
    run_log_text: str | None = None


@dataclass
class TaskRecordValidationReport:
    task_dir: Path
    ok: bool
    errors: tuple[str, ...] = field(default_factory=tuple)
    warnings: tuple[str, ...] = field(default_factory=tuple)
    bundle: TaskRecordBundle | None = None


class TaskRecordStore:
    """Read-only loader for Stage 0-style task record directories."""

    def __init__(self, root_path: str | Path):
        self.root_path = Path(root_path)

    def find_task_dirs(self) -> list[Path]:
        if not self.root_path.exists():
            return []
        return sorted(
            path
            for path in self.root_path.iterdir()
            if path.is_dir() and (path / "task_spec.json").exists()
        )

    def load_task_bundle(self, task_dir: str | Path) -> TaskRecordBundle:
        path = self._resolve_task_dir(task_dir)
        missing = [file_name for file_name in REQUIRED_TASK_RECORD_FILES if not (path / file_name).exists()]
        if missing:
            raise FileNotFoundError(f"task record missing required file(s): {', '.join(missing)}")

        run_log_path = path / "run_log.md"
        return TaskRecordBundle(
            task_dir=path,
            task_spec=_read_json_object(path / "task_spec.json"),
            completion=_read_json_object(path / "completion.json"),
            handoff_pack=_read_json_object(path / "handoff_pack.json"),
            events_path=path / "events.jsonl",
            run_log_path=run_log_path if run_log_path.exists() else None,
            run_log_text=run_log_path.read_text(encoding="utf-8") if run_log_path.exists() else None,
        )

    def validate_task_bundle(self, task_dir: str | Path) -> TaskRecordValidationReport:
        path = self._resolve_task_dir(task_dir)
        errors: list[str] = []
        warnings: list[str] = []

        missing = [file_name for file_name in REQUIRED_TASK_RECORD_FILES if not (path / file_name).exists()]
        for file_name in missing:
            errors.append(f"missing required file: {file_name}")
        if errors:
            return TaskRecordValidationReport(
                task_dir=path,
                ok=False,
                errors=tuple(errors),
                warnings=tuple(warnings),
            )

        try:
            bundle = self.load_task_bundle(path)
        except (FileNotFoundError, JSONDecodeError, ValueError) as exc:
            return TaskRecordValidationReport(
                task_dir=path,
                ok=False,
                errors=(str(exc),),
                warnings=tuple(warnings),
            )

        completion_result = validate_completion_record(bundle.completion)
        errors.extend(f"completion.json: {error}" for error in completion_result.errors)
        warnings.extend(f"completion.json: {warning}" for warning in completion_result.warnings)

        handoff_result = validate_handoff_pack(bundle.handoff_pack)
        errors.extend(f"handoff_pack.json: {error}" for error in handoff_result.errors)
        warnings.extend(f"handoff_pack.json: {warning}" for warning in handoff_result.warnings)

        events_result = validate_replay_preflight_check(bundle.events_path)
        errors.extend(f"events.jsonl: {error}" for error in events_result.errors)
        warnings.extend(f"events.jsonl: {warning}" for warning in events_result.warnings)

        return TaskRecordValidationReport(
            task_dir=path,
            ok=not errors,
            errors=tuple(errors),
            warnings=tuple(warnings),
            bundle=bundle,
        )

    def _resolve_task_dir(self, task_dir: str | Path) -> Path:
        path = Path(task_dir)
        if path.is_absolute():
            return path
        candidate = self.root_path / path
        if candidate.exists():
            return candidate
        return path


def _read_json_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.name} must contain a JSON object")
    return value
