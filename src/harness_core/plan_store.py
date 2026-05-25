"""Append-only plan store for Harness App MVP3 planning state."""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any

from harness_core.resource_planner import ResourcePlan


APP_PLANS_SCHEMA_VERSION = "app_plans.v1"


class PlanStoreError(ValueError):
    """Raised when plan store state is invalid."""


def load_plans(path: str | Path) -> dict[str, Any]:
    store_path = Path(path)
    if not store_path.exists():
        return {"schema_version": APP_PLANS_SCHEMA_VERSION, "plans": []}
    try:
        data = json.loads(store_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise PlanStoreError("plan store is unreadable or invalid JSON") from exc
    if data.get("schema_version") != APP_PLANS_SCHEMA_VERSION:
        raise PlanStoreError("unsupported plan store schema version")
    if not isinstance(data.get("plans"), list):
        raise PlanStoreError("plan store plans must be a list")
    return data


def save_plan(path: str | Path, plan: ResourcePlan) -> dict[str, Any]:
    store_path = Path(path)
    data = load_plans(store_path)
    data["plans"].append(plan.to_dict())
    _atomic_write_json(store_path, data)
    return data


def get_plan(path: str | Path, plan_id: str) -> dict[str, Any] | None:
    data = load_plans(path)
    for plan in data["plans"]:
        if isinstance(plan, dict) and plan.get("plan_id") == plan_id:
            return plan
    return None


def _atomic_write_json(path: Path, data: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = path.with_name(f"{path.name}.tmp")
    tmp_path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.replace(tmp_path, path)
