# User-Style Mutation Evaluation Track

## Overview

This track adds realistic user-style mutation coverage to the existing
real-world read-only evaluation fixtures.  Each representative base fixture
is exercised through three input expression styles to test how the harness
handles formal, informal, and minimal task descriptions.

## Variant Types

| Variant | Description | Typical Admission |
|---------|-------------|-------------------|
| `formal_issue` | Structured, issue/spec-like description with all fields | admitted |
| `user_style_chat_request` | Natural language, possibly incomplete or colloquial | admitted or needs_clarification |
| `terse_ticket` | Minimal info, should not be silently admitted | needs_clarification or diagnostic |

## Hard Constraints

- No modifications to runtime main path.
- No modifications to `docs/stage0/events.jsonl`.
- No real model calls, no real agent execution.
- No new dependencies installed.
- Helper module is pure stdlib (fixture loading, schema validation, grouping).

## fixture_metadata Schema

Every mutation case carries a `fixture_metadata` object:

| Field | Type | Constraint |
|-------|------|------------|
| `fixture_id` | string | unique identifier |
| `source_type` | string | `synthetic`, `copied_real_read_only`, or `mutated_user_style` |
| `freshness` | string | ISO date or freeform freshness label |
| `estimated_human_minutes` | number | estimated minutes for a human to triage |
| `difficulty` | string | difficulty label |
| `contamination_risk` | string | `low`, `medium`, `high`, or `unknown` |
| `admission_scope` | string | `admitted` or `diagnostic` |

## Mutation Case Schema

Each mutation case contains:

| Field | Type | Description |
|-------|------|-------------|
| `case_id` | string | Unique case identifier |
| `base_fixture_id` | string | Reference to the base real-world fixture |
| `variant_type` | string | One of the three variant types |
| `user_prompt` | string | The simulated user input |
| `expected_task_family` | string | Expected task type classification |
| `expected_required_fields` | list | Fields that should be present in parsed output |
| `expected_missing_fields` | list | Fields expected to be absent (forterse/ incomplete inputs) |
| `admission_expectation` | string | Expected admission outcome |
| `evidence_refs` | list | Evidence references for audit |
| `fixture_metadata` | object | Metadata per the schema above |
| `schema_version` | string | Always `user_style_mutation.v1` |

## Admission Rules

1. **formal_issue** should always be `admitted` -- full information is present.
2. **user_style_chat_request** should be `admitted` or `needs_clarification`.
   It must never silently fail or be rejected without explanation.
3. **terse_ticket** with insufficient information should produce
   `needs_clarification` or `diagnostic`.  It must not be伪装成 `admitted`.

## Module

Helpers live in `src/harness_core/user_style_mutation.py`:

```python
from harness_core.user_style_mutation import (
    VARIANT_TYPES,
    ADMISSION_OUTCOMES,
    CONTAMINATION_RISKS,
    ADMISSION_SCOPES,
    MutationCase,
    FixtureMetadata,
    validate_mutation_case,
    validate_fixture_metadata,
    create_mutation_case,
    load_all_fixtures,
    group_by_admission,
    group_by_variant,
    group_by_base_fixture,
)
```
