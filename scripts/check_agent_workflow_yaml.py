"""Parse every agent workflow with the explicitly supplied CI YAML parser."""

from pathlib import Path

import yaml


for workflow in sorted(Path(".github/workflows").glob("agent-*.yml")):
    if not isinstance(yaml.safe_load(workflow.read_text(encoding="utf-8")), dict):
        raise SystemExit(f"workflow is not a mapping: {workflow}")
    print(workflow)
