# Packaging Readiness

This document describes the packaging metadata added to the project.

## What this is

Package-readiness metadata for `token-efficient-agent-harness-lab`. The `pyproject.toml` declares the project name, version, Python requirement, and package discovery under `src/`.

## What this is NOT

- **Not published to PyPI** — this is internal package metadata only.
- **No new dependencies added** — `dependencies = []` is intentional.
- **No runtime changes** — existing code and `events.jsonl` are untouched.
- **No model changes** — no model integrations or API calls added.

## Testing

Tests run the same way as before:

```bash
PYTHONPATH=src uv run --no-project python -m unittest discover -s tests
```

The `tests/test_packaging_metadata.py` file contains 10 tests validating `pyproject.toml` structure via `tomllib`.
