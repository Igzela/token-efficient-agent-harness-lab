# Tool / Error Taxonomy

## Overview

This document defines the canonical error taxonomy for the token-efficient agent
harness. Every tool execution failure must be classified into exactly one domain.
The classification drives retry policy, scoring attribution, and whether the
error can contribute to policy candidate adoption.

## Canonical Error Domains

| Domain | Description | Retryable | Counts Against Model | Requires Human Triage |
|--------|-------------|-----------|----------------------|----------------------|
| `tool_contract_error` | Tool schema violation or bad arguments | Yes (usually) | False (usually) | No |
| `environment_error` | OS, filesystem, network, dependency failure | Yes | False | No |
| `context_error` | Missing or corrupt context pack / prompt | Yes | No | No |
| `model_judgment_error` | Model chose wrong tool or wrong arguments | Yes | True | No |
| `evaluation_error` | Error in evaluation / scoring pipeline | No | False | Yes |
| `harness_bug` | Internal harness defect | No | False | Yes |
| `user_abort` | User-initiated cancellation | No | False | No |
| `provider_error` | Upstream API / provider failure | Yes | False | No |
| `timeout` | Execution exceeded time budget | Yes | False | No |
| `unknown_error` | Unclassified / unrecognised failure | **No (fail-hard)** | False | **Yes (mandatory)** |

## Domain Rules

### unknown_error

- **Must** have `retryable=false`. Unknown errors are never silently retried.
- **Must** have `requires_human_triage=true`. Every unknown error surfaces for
  human review before it can be reclassified.
- **Must not** drive policy candidate adoption. An unclassified error cannot be
  used to automatically adopt or reject a policy candidate.

### provider_error vs timeout

These two domains are intentionally distinct:

- `provider_error`: the provider returned an error (4xx/5xx, rate limit, auth
  failure). The harness may retry with backoff.
- `timeout`: the provider did not respond within the allowed time budget. This
  may indicate provider slowness or a network partition.

Both are retryable, but they carry different diagnostic signals and may trigger
different recovery strategies.

### tool_contract_error

Usually `counts_against_model=false` because the error originates from a
malformed tool definition, not from the model's reasoning. However, if the
model itself generates an illegal tool call (e.g. passes invalid JSON in a
tool argument), then `counts_against_model=true` is appropriate.

### model_judgment_error

`counts_against_model=true` by definition — the model made a decision that
caused the failure.

### user_abort

User aborts are not system failures. They carry `retryable=false` and
`counts_against_model=false`. The harness treats them as graceful terminations.

## error_record Schema

Version: `error_record.v1`

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `schema_version` | string | Always `"error_record.v1"` |
| `error_id` | string | Unique identifier (UUID recommended) |
| `error_domain` | string | One of the canonical domains above |
| `error_class` | string | Specific error class / type |
| `retryable` | bool | Whether this error should be retried |
| `counts_against_model` | bool | Whether this counts toward model quality scoring |
| `requires_human_triage` | bool | Whether this error needs human review |
| `tool_name` | string | Name of the tool that produced the error |
| `model_profile_id` | string | Model profile that was active |
| `context_pack_id` | string | Context pack associated with the error |
| `event_id` | string | Event store event ID |
| `evidence_refs` | list[string] | References to supporting evidence (log lines, artifacts) |
| `created_at` | string | ISO-8601 timestamp |

### Validation Rules

1. All required fields must be present.
2. `schema_version` must equal `"error_record.v1"`.
3. `error_domain` must be one of the 10 canonical domains.
4. `error_id` must be a non-empty string.
5. `retryable`, `counts_against_model`, `requires_human_triage` must be `bool`.
6. `evidence_refs` must be a `list`.
7. Domain-specific constraints (see table above) are enforced at validation time.

## Module

The schema and validation helpers live in `src/harness_core/error_taxonomy.py`.

```python
from harness_core.error_taxonomy import (
    ErrorDomain,
    ErrorRecord,
    CANONICAL_DOMAINS,
    create_error_record,
    validate_error_record,
    is_adoptable,
    is_retryable,
    requires_triage,
)
```
