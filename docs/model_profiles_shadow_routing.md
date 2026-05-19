# Model Profiles and Shadow Routing Track

## Overview

Defines model harness profiles for describing model capabilities (not credentials)
and shadow routing recommendations for diagnostic-only routing suggestions.

Shadow router only outputs recommendations — it never changes active routing policy.

## model_harness_profile Schema

Version: `model_harness_profile.v1`

| Field | Type | Constraint |
|-------|------|------------|
| `schema_version` | string | Always `model_harness_profile.v1` |
| `profile_id` | string | Unique identifier |
| `provider` | string | Provider name (no credentials) |
| `model_id` | string | Model identifier |
| `tier` | string | One of the tier enums |
| `tool_strictness` | string | One of the tool_strictness enums |
| `json_tolerance` | string | One of the json_tolerance enums |
| `reasoning_effort` | string | One of the reasoning_effort enums |
| `output_format_expectation` | string | Description of expected output format |
| `parallel_tool_preference` | string | One of the parallel_tool_preference enums |
| `escaping_quirks` | string | Description of escaping behavior |
| `cache_strategy` | string | One of the cache_strategy enums |
| `fallback_policy` | string | One of the fallback_policy enums |
| `context_window` | int | Must be positive |
| `cost_metadata` | object | Cost information (non-negative) |
| `allowed_tools` | list | Tools the model can use |
| `forbidden_previous_tools` | list | Tools that must not appear in previous context |

## Enums

| Enum | Values |
|------|--------|
| tier | cheap_executor, balanced_worker, strong_planner, verifier, advisor |
| tool_strictness | strict, tolerant, unsupported |
| json_tolerance | strict_json, tolerant_json, text_only |
| reasoning_effort | low, medium, high |
| parallel_tool_preference | none, allowed, preferred, forbidden |
| cache_strategy | no_cache, read_cache, write_cache, read_write_cache |
| fallback_policy | no_fallback, same_tier_only, lower_cost_allowed, higher_quality_allowed, human_required |
| enforcement_scope | prompt_assembly, gateway_validation, context_broker, all |

## shadow_routing_recommendation Schema

Version: `shadow_routing_recommendation.v1`

| Field | Type | Constraint |
|-------|------|------------|
| `recommendation_id` | string | Unique identifier |
| `task_family` | string | Task family |
| `variant_family` | string | Variant family |
| `success_criterion` | string | Success criterion |
| `candidate_profile_id` | string | Profile being recommended |
| `baseline_profile_id` | string | Current baseline profile |
| `rationale` | string | Non-empty explanation |
| `evidence_refs` | list | Evidence references |
| `expected_quality_delta` | number | Expected quality change |
| `expected_cost_delta` | number | Expected cost change |
| `risk_level` | string | One of the risk_level enums |
| `recommendation` | string | One of the recommendation enums |
| `admission_scope` | string | Must be `diagnostic` |
| `active_routing_allowed` | bool | Must be `false` |

## Key Rules

- **No credentials** in profiles — profiles describe capabilities, not auth.
- **Shadow = diagnostic only** — never modifies active routing policy.
- **active_routing_allowed = false** always.
- **forbidden_previous_tools** is a hard constraint, not a suggestion.
- **allowed_tools and forbidden_previous_tools** cannot share tool_ids.
- **cost_metadata** aligns with usage_ledger but no real prices required.
- **context_window** must be positive integer.

## Module

Helpers live in `src/harness_core/model_profiles.py`:

```python
from harness_core.model_profiles import (
    validate_model_harness_profile,
    validate_shadow_routing_recommendation,
    is_shadow_only,
    can_compare_with_usage_ledger,
    ModelHarnessProfile,
    ShadowRoutingRecommendation,
    CostMetadata,
    ForbiddenPreviousTool,
)
```
