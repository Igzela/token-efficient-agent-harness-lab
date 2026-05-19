# Context Pack v2 and Memory Boundary Track

## Overview

Implements offline schema, helper, and test coverage for Context Pack v2,
ensuring compatibility with v1.2 canonical wire schemas and v1.3.2
five-layer composition layout.

## Canonical Wire Schemas

| Schema | Version | Purpose |
|--------|---------|---------|
| `advisor_context_pack_v2` | `advisor_context_pack.v2` | Context for advisor broker calls |
| `model_context_pack_v2` | `model_context_pack.v2` | Context for model execution |
| `context_retrieval_request` | `context_retrieval_request.v1` | Request to fetch full content |
| `context_retrieval_result` | `context_retrieval_result.v1` | Result of content retrieval |

## Five-Layer Structure (context_layers)

The v1.3.2 five-layer structure is a composition layout embedded as
`context_layers` inside the v1.2 schemas. It does NOT replace them.

```yaml
context_layers:
  invariants:      # long-lived: project invariants, system rules, quality gates
  task_pack:       # medium-lived: current task objectives, constraints, success criteria
  dynamic_refs:    # short-lived: file paths, evidence refs, retrieval pointers
  memory_digest:   # medium-long-lived: historical decisions, verified conclusions, open questions
  recent_evidence: # very short-lived: recent tool_results, diffs, failure diagnostics
```

## Key Rules

- **Default minimal context.** Full content must be retrieved via explicit request.
- **No full artifact/run_log inline** unless retrieval_result content_mode=full and policy allows.
- **memory_digest** must have source_refs, expiry_policy, and conflict_resolution.
- **Context Pack Builder** only reads allowed refs — no bypassing retrieval_policy.
- **Orchestrator** does not write long-term memory directly.
- **Skill Extractor** does not auto-modify prompts.
- **budget_exceeded** is a valid retrieval_result status, not an uncontrolled error.
- **denied** is a valid retrieval_result status.

## Enums

| Enum | Values |
|------|--------|
| content_mode | summary, excerpt, full |
| retrieval_result status | fulfilled, partial, denied, not_found, budget_exceeded |
| freshness | current, stale, unknown |
| cache_policy | no_cache, read_cache_allowed, write_cache_allowed, read_write_cache_allowed |
| pack_prune_policy | preserve_invariants, drop_recent_evidence_first, drop_memory_digest_first, deny_if_over_budget |

## Module

Helpers live in `src/harness_core/context_pack.py`:

```python
from harness_core.context_pack import (
    validate_advisor_context_pack_v2,
    validate_model_context_pack_v2,
    validate_context_retrieval_request,
    validate_context_retrieval_result,
    validate_context_layers,
    validate_full_content_inline_denied,
    check_budget_compliance,
    apply_prune_policy,
    ContextBudget,
    ContextLayers,
    MemoryDigest,
    RetrievalPolicy,
)
```
